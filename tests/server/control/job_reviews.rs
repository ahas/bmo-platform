//! A job's review (docs/job-review.md): an answered turn puts the job under review rather than ending it, its
//! checkout and lease kept; the orderer's moves refused where they do not fit — no turn, a job not under review,
//! the owner asked before the ceiling; the checks passed one at a time, a job of one stage ending once its last
//! check passes, a job of several going on to its next stage; a move's answer read through the agency's own
//! client while the job stays under review; and a rejection counted, its round moved on and its
//! rework turned out; the owner asked once for one question, an escalation made again taking the question that
//! waits, which stays open until a reply settles it; and the seat a job holds kept through its review and across
//! a restart, so its next stage runs on its agent before a job waiting for that agent starts.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::work;
use agency_server::control::{Front, router};
use agency_server::job_review::{Resumed, result_language};
use agency_server::now_ms;
use agency_server::order_run::dispatch::{self, Asked, Wanted};
use agency_server::outbox::{owed, restarted, send_due};
use agency_server::places::Ground;
use agency_server::queue::settings;
use agency_server::turn_marks::{self, Mark};
use agency_server::watches::job_events::{CURSOR_KEY, QUESTION_OWED};
use bmo_agency_client::{AgencyClient, JobReviewAsk, JobState};
use harness_providers::Machine;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::grounds::{deepseek_balance, seeded};
use crate::support::peers::{Peers, item, stand};
use crate::support::rows::repository;
use crate::support::scratch::Scratch;

// The pass with no gate deployed, so a word for a job goes as written.
async fn pass(
  db: &sea_orm::DatabaseConnection,
  vars: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
  agency_server::watches::job_events::pass(db, vars, &crate::support::gates::none()).await
}

const TOKEN: &str = "secret";

fn ground(scratch: &Scratch, home: &std::path::Path) -> Arc<Ground> {
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.join("data"),
  );

  Arc::new(Ground::new(&service, &machine, None))
}

// A ground naming the stand-in as its job service and its receiver, with the stand-in and its settings.
async fn ground_on_peers(
  scratch: &Scratch,
  home: &std::path::Path,
) -> (Arc<Ground>, Peers, BTreeMap<String, String>) {
  let (peers, vars) = stand().await;
  let service = Service::new(scratch.db().clone(), Config::read(|_| None).unwrap());
  let env = [
    ("HOME".to_owned(), home.to_string_lossy().into_owned()),
    ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
  ]
  .into_iter()
  .chain(vars.clone());
  let ground = Arc::new(Ground::new(&service, &Machine::with_env(env, home.join("data")), None));

  (ground, peers, vars)
}

async fn front(ground: Arc<Ground>) -> u16 {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let app = router(Front::new(TOKEN.to_owned(), ground).unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  port
}

async fn call(port: u16, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
  let mut request = reqwest::Client::new()
    .request(method, format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(TOKEN);

  if let Some(body) = body {
    request = request.json(&body);
  }

  let answer = request.send().await.unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

// A repository's checkout with one running job on an item in `stages` stages, its assignment running.
async fn running(scratch: &Scratch, worker: &str, job: i64, item: i64, stages: &[&str]) {
  let now = now_ms();

  repository(scratch.db(), worker, &format!("/checkouts/{worker}"), &[]).await;
  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES ($1, $2, 'harness', $3, 'running', $4, $4)"#,
      vec![job.into(), item.into(), worker.into(), now.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "state", "started_at", "prompt", "review_stages", "review_stage", "review_turn")
         VALUES ($1, $2, 0, 'harness', $1, $3, 'running', $4, 'work', $5, 1, 'work')"#,
      vec![
        job.into(),
        item.into(),
        worker.into(),
        now.into(),
        serde_json::to_string(stages).unwrap().into(),
      ],
    )
    .await;
}

async fn read(port: u16, job: i64) -> Value {
  call(port, reqwest::Method::GET, &format!("/jobs/{job}"), None).await.1
}

async fn review(port: u16, agent: &str, job: i64, action: &str, text: Option<&str>) -> (u16, Value) {
  call(
    port,
    reqwest::Method::POST,
    "/job-reviews",
    Some(json!({ "agentName": agent, "jobId": job, "action": action, "text": text })),
  )
  .await
}

// The seat of the agent a job's turn was let in on, standing let in as a job's slot request does.
async fn seated(scratch: &Scratch, job: i64) {
  scratch
    .run(
      r#"INSERT INTO "role_slot_requests" ("token", "request_key", "owner", "source", "role", "path", "provider",
           "model", "effort", "capacity", "state", "enqueued_at", "class", "seat")
         VALUES ($1, $1, $2, 'job', 'coder', '/checkouts/', 'deepseek', 'deepseek-flash', '', 1, 'active', 0,
           'foreground', 'coder-05')"#,
      vec![format!("seat-{job}").into(), format!("job-{job}").into()],
    )
    .await;
}

async fn seat_state(scratch: &Scratch, job: i64) -> i64 {
  scratch
    .count(
      r#"SELECT COUNT(*) FROM "role_slot_requests" WHERE "owner" = $1 AND "state" = 'active'"#,
      vec![format!("job-{job}").into()],
    )
    .await
}

// Waits until the job stands in `state`, however its turn comes to it.
async fn until_state(port: u16, job: i64, state: &str) -> Value {
  for _ in 0..200 {
    let member = read(port, job).await;

    if member["state"] == state {
      return member;
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  panic!("job {job} never stood {state}: {}", read(port, job).await);
}

fn carrying(agent: &str, run: &str) {
  turn_marks::carrying(
    agent,
    run,
    Mark {
      chat_id: -5,
      privileged: false,
    },
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answered_turn_puts_the_job_under_review_and_keeps_its_checkout() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let port = front(ground.clone()).await;

  running(&scratch, "worker-r", 21, 61, &[]).await;
  seated(&scratch, 21).await;
  agency_server::job_review::reported(&ground, 21, "Done: the guard is mended and its test passes.", None)
    .await;

  // Its agent answers for it until it ends: the seat its turn was let in on stays the job's under review.
  assert_eq!(seat_state(&scratch, 21).await, 1);

  let member = read(port, 21).await;

  assert_eq!(member["state"], "reviewing", "{member}");
  assert_eq!(member["report"], "Done: the guard is mended and its test passes.");
  assert_eq!(
    (&member["review"]["check"], &member["review"]["round"], &member["review"]["rejections"]),
    (&json!("promises"), &json!(1), &json!(0))
  );

  // The job keeps its checkout while it is under review.
  let (_, view) = call(port, reqwest::Method::GET, "/repositories/worker-r", None).await;

  assert!(!view["checkouts"][0]["job"].is_null(), "{view}");
  assert_eq!(call(port, reqwest::Method::GET, "/assignments/21", None).await.1["state"], "running");

  let (_, listed) = call(port, reqwest::Method::GET, "/jobs?running=true", None).await;

  assert_eq!(listed["items"][0]["id"], 21, "{listed}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_checks_pass_one_at_a_time_and_a_job_of_one_stage_ends_after_its_last() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let port = front(ground.clone()).await;
  let agent = "reviewing-chat-agent";

  running(&scratch, "worker-s", 22, 62, &[]).await;
  agency_server::job_review::reported(&ground, 22, "All of it is done.", None).await;

  // No turn, no move.
  let (status, refused) = review(port, agent, 22, "pass", None).await;

  assert_eq!((status, refused["code"].as_str()), (409, Some("no-turn")), "{refused}");
  carrying(agent, "run-r1");

  // A move outside the four, and one that needs words and names none, are refused.
  let (status, refused) = review(port, agent, 22, "approve", None).await;

  assert_eq!((status, refused["code"].as_str()), (400, Some("bad-request")), "{refused}");

  let (status, refused) = review(port, agent, 22, "reject", None).await;

  assert_eq!((status, refused["code"].as_str()), (400, Some("bad-request")), "{refused}");

  // The owner is not asked before the work was sent back as often as the ceiling.
  let (status, refused) = review(port, agent, 22, "escalate", Some("Is this his call?")).await;

  assert_eq!((status, refused["code"].as_str()), (409, Some("escalation-early")), "{refused}");

  // The first check passed brings the second; the second passed ends a job of one stage with its report.
  let (status, member) = review(port, agent, 22, "pass", Some("Every promise is kept.")).await;

  assert_eq!(status, 200, "{member}");
  assert_eq!(
    (&member["state"], &member["review"]["check"]),
    (&json!("reviewing"), &json!("decisions"))
  );

  let (status, member) = review(port, agent, 22, "pass", None).await;

  assert_eq!(status, 200, "{member}");
  assert_eq!(member["state"], "ended", "{member}");
  assert_eq!(read(port, 22).await["report"], "All of it is done.");
  assert_eq!(call(port, reqwest::Method::GET, "/assignments/22", None).await.1["state"], "released");

  // A job no longer under review takes no move.
  let (status, refused) = review(port, agent, 22, "pass", None).await;

  assert_eq!((status, refused["code"].as_str()), (409, Some("not-reviewing")), "{refused}");
  turn_marks::ended(agent, "run-r1");
}

// The review tool reads the route's answer through the agency's own client: a move that leaves the job under
// review answers where the job stands, `reviewing` with the check due next and nothing of its report, and the
// move is made once. The report's words are told once a round: the round's next check names the report the
// first was told with instead of carrying it again (docs/job-review.md, 「The checks」).
#[tokio::test(flavor = "multi_thread")]
async fn a_move_answers_where_the_job_stands_and_the_round_s_next_check_names_the_report_told_before() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (ground, peers, vars) = ground_on_peers(&scratch, home.path()).await;
  let port = front(ground.clone()).await;
  let agent = "client-reviewing-agent";
  let report = "All of it is done: the guard is mended and its test passes.";

  peers.lock().unwrap().items.insert(66, item(66, "leased", json!({ "roomId": -5 })));
  running(&scratch, "worker-c", 26, 66, &["Investigate", "Implement"]).await;
  agency_server::job_review::reported(&ground, 26, report, None).await;
  carrying(agent, "run-c1");

  let client = AgencyClient::new(&format!("http://127.0.0.1:{port}"), TOKEN).unwrap();
  let ask = JobReviewAsk {
    agent_name: agent.to_owned(),
    job_id: 26,
    action: "pass".to_owned(),
    text: None,
    stage: None,
  };
  let job = client.create_job_reviews(&ask).await.unwrap();

  assert_eq!((job.id, job.item_id, job.state), (26, 66, JobState::Reviewing));
  assert_eq!(
    (job.review.check.as_deref(), job.review.round, job.review.stage, job.review.stages),
    (Some("decisions"), 1, Some(1), 2)
  );

  let passes: Vec<Value> = peers
    .lock()
    .unwrap()
    .calls
    .iter()
    .filter(|call| call.path == "/items/66/reviews" && call.body["kind"] == "pass")
    .map(|call| call.body.clone())
    .collect();

  assert_eq!(passes.len(), 1, "{passes:?}");
  assert_eq!(passes[0]["labels"]["check"], "promises");

  // The first check of the round carried the report whole; the second names it, carrying none of its words.
  send_due(scratch.db(), &vars).await.unwrap();

  let orders: Vec<String> = peers
    .lock()
    .unwrap()
    .instructed()
    .iter()
    .filter(|told| told["kind"] == "job-review:66")
    .map(|told| told["said"]["order"].as_str().unwrap_or_default().to_owned())
    .collect();
  let decisions = orders.iter().find(|order| order.contains("The check due now is decisions")).unwrap();

  assert!(!decisions.contains(report), "{decisions}");
  assert!(
    decisions.contains(
      "The worker's report:\n\nThe same report this round's first check was told with, unchanged. Where your \
       session no longer holds it, job_reviews on item #66 reads it."
    ),
    "{decisions}"
  );

  // The route's own answer carries the job's position alone: no report, no steps and no spend.
  let (status, member) = review(port, agent, 26, "pass", None).await;

  assert_eq!(status, 200, "{member}");
  let mut fields: Vec<&String> = member.as_object().unwrap().keys().collect();

  fields.sort();
  assert_eq!(fields, ["id", "itemId", "review", "state"]);
  assert_eq!(
    member["review"],
    json!({ "stage": 2, "stages": 2, "round": 1, "check": null, "rejections": 0, "question": null })
  );
  turn_marks::ended(agent, "run-c1");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rejection_is_counted_and_a_stage_passed_hands_the_next_one_over() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let port = front(ground.clone()).await;
  let agent = "staging-chat-agent";

  carrying(agent, "run-s1");

  // Sent back: counted, the round moves on, and the rework goes out as a turn of the job.
  running(&scratch, "worker-t", 23, 63, &[]).await;
  agency_server::job_review::reported(&ground, 23, "Done, but the docs are for later.", None).await;

  let (status, member) =
    review(port, agent, 23, "reject", Some("The docs were part of the brief: write them now.")).await;

  assert_eq!(status, 200, "{member}");
  assert_eq!((&member["review"]["rejections"], &member["review"]["round"]), (&json!(1), &json!(2)));

  // This ground stands no places, so the rework's turn fails as it goes out; that it went out is what is read.
  until_state(port, 23, "stopped").await;

  // Two stages: both checks of the first passed hand the second over as the next turn.
  running(&scratch, "worker-u", 24, 64, &["Change the screen", "Check it with the design skill"]).await;
  agency_server::job_review::reported(&ground, 24, "The screen is changed.", None).await;
  review(port, agent, 24, "pass", None).await;

  let (status, member) = review(port, agent, 24, "pass", None).await;

  assert_eq!(status, 200, "{member}");
  assert_eq!((&member["review"]["stage"], &member["review"]["round"]), (&json!(2), &json!(1)));
  assert_eq!(member["review"]["stages"], 2);
  assert_eq!(
    read(port, 24).await["review"]["stages"],
    json!(["Change the screen", "Check it with the design skill"])
  );
  // The second stage goes out as a turn the way the rework did; it is waited out, so no turn of it still holds the
  // store when the test's database is dropped.
  until_state(port, 24, "stopped").await;
  turn_marks::ended(agent, "run-s1");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_owner_s_word_holds_the_review_gives_the_checkout_up_past_its_wait_and_comes_back_as_a_check() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground(&scratch, home.path());
  let port = front(ground.clone()).await;
  let agent = "escalating-chat-agent";
  let vars = std::collections::BTreeMap::new();

  carrying(agent, "run-e1");
  running(&scratch, "worker-v", 25, 65, &[]).await;
  seated(&scratch, 25).await;
  agency_server::job_review::reported(&ground, 25, "Done, but one matter is the owner's.", None).await;

  // Past the ceiling the work may be put to the owner: refused here only because this ground names no job
  // service to ask the question in, never as too early.
  scratch.run(r#"UPDATE "jobs" SET "rejections" = 10 WHERE "id" = 25"#, vec![]).await;

  let (status, refused) = review(port, agent, 25, "escalate", Some("Is the colour his call?")).await;

  assert_eq!((status, refused["code"].as_str()), (502, Some("upstream-failed")), "{refused}");

  // While the owner's question waits, nothing of the review moves.
  scratch.run(r#"UPDATE "jobs" SET "review_question" = 77 WHERE "id" = 25"#, vec![]).await;

  let (status, refused) = review(port, agent, 25, "pass", None).await;

  assert_eq!((status, refused["code"].as_str()), (409, Some("awaits-owner")), "{refused}");

  // Past its due time the job gives its checkout up, its item still its own.
  agency_server::job_review::question_due(scratch.db(), &vars, 77).await.unwrap();

  let member = read(port, 25).await;

  assert_eq!(
    (&member["state"], &member["review"]["parked"]),
    (&json!("reviewing"), &json!(true)),
    "{member}"
  );
  assert_eq!(call(port, reqwest::Method::GET, "/assignments/25", None).await.1["state"], "released");

  // Paused so, it answers for its agent no longer: the seat is given back for a job waiting on that agent.
  assert_eq!(seat_state(&scratch, 25).await, 1);
  agency_server::control::work::paused_seats_given_back(&ground).await;
  assert_eq!(seat_state(&scratch, 25).await, 0);

  // The owner's answer comes back as the check due, and the review moves again.
  agency_server::job_review::ruled(
    scratch.db(),
    &vars,
    &json!({ "questionId": 77, "choice": "Send the work back", "text": "Settle the colour yourself." }),
  )
  .await;

  assert_eq!(read(port, 25).await["review"]["question"], Value::Null);

  // Sent back, the job takes its checkout again before its turn goes out.
  let (status, member) =
    review(port, agent, 25, "reject", Some("The owner says: settle the colour yourself.")).await;

  assert_eq!(status, 200, "{member}");

  let member = until_state(port, 25, "stopped").await;

  assert_eq!(member["review"]["parked"], false, "{member}");
  turn_marks::ended(agent, "run-e1");
}

// One question is one row told once (docs/job-review.md, 「The owner's word」): an escalation made again after its
// question landed but its answer was lost takes the question that waits; with no reply it stays open and the job
// waits on it; a settling reply alone lets the review move; and a question already answered is never taken again.
#[tokio::test(flavor = "multi_thread")]
async fn an_escalation_made_again_asks_the_owner_once_and_waits_until_a_reply_settles_it() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let home = tempfile::tempdir().unwrap();
  let (ground, peers, vars) = ground_on_peers(&scratch, home.path()).await;
  let port = front(ground.clone()).await;
  let agent = "asking-once-agent";
  let words = Some("Is the colour his call?");
  let first = format!("{QUESTION_OWED}1");

  peers.lock().unwrap().items.insert(67, item(67, "leased", json!({ "scope": "-670" })));
  settings::set(db, CURSOR_KEY, "0").await.unwrap();
  carrying(agent, "run-q1");
  running(&scratch, "worker-q", 27, 67, &[]).await;
  agency_server::job_review::reported(&ground, 27, "Done, but one matter is the owner's.", None).await;
  scratch.run(r#"UPDATE "jobs" SET "rejections" = 10 WHERE "id" = 27"#, vec![]).await;

  // The question lands, but its answer is lost on the way back.
  peers.lock().unwrap().lost.insert("POST /questions".to_owned());

  let (status, refused) = review(port, agent, 27, "escalate", words).await;

  assert_eq!((status, refused["code"].as_str()), (502, Some("upstream-failed")), "{refused}");
  peers.lock().unwrap().lost.clear();

  // Made again, the escalation takes the question that waits rather than asking the owner a second time.
  let (status, member) = review(port, agent, 27, "escalate", words).await;

  assert_eq!((status, &member["review"]["question"]), (200, &json!(1)), "{member}");
  assert_eq!(peers.lock().unwrap().questions.len(), 1, "one question, one row");

  // The escalation is the supervisor's move: asked of a person under the supervisors' own token.
  {
    let held = peers.lock().unwrap();
    let asked = held
      .calls
      .iter()
      .find(|call| call.method.as_str() == "POST" && call.path == "/questions")
      .expect("the question asked");

    assert_eq!(asked.body["addressee"], "person");
    assert_eq!(asked.body["choices"].as_array().map(Vec::len), Some(2), "a person is offered a choice");
    assert_eq!(asked.bearer.as_deref(), Some("supervisors-token"));
  }

  pass(db, &vars).await.unwrap();
  pass(db, &vars).await.unwrap();

  let told = peers.lock().unwrap().instructed();
  let questions_told: Vec<_> = told
    .iter()
    .filter(|said| said["kind"].as_str().is_some_and(|kind| kind.starts_with(QUESTION_OWED)))
    .collect();

  assert_eq!(questions_told.len(), 1, "told once: {questions_told:?}");

  // With no reply the question stays open: nothing here moves its state, and the job waits on it.
  {
    let held = peers.lock().unwrap();

    assert_eq!(held.questions[0]["state"], "open");
    assert!(
      !held
        .calls
        .iter()
        .any(|call| call.method.as_str() == "PATCH" && call.path.starts_with("/questions"))
    );
  }
  assert_eq!(read(port, 27).await["review"]["question"], json!(1));
  assert!(!owed(db, &first).await.unwrap().is_empty(), "its instruction still owed");

  // A settling reply answers it: the instruction is let go and the owner's word comes back as the check due.
  peers.lock().unwrap().settle(1, "Send the work back");
  pass(db, &vars).await.unwrap();

  assert!(owed(db, &first).await.unwrap().is_empty(), "its instruction let go");

  let member = read(port, 27).await;

  assert_eq!(
    (&member["state"], &member["review"]["question"]),
    (&json!("reviewing"), &Value::Null),
    "{member}"
  );

  // The same words put to the owner again once he answered are a new question: an answered one is never taken.
  let (status, member) = review(port, agent, 27, "escalate", words).await;

  assert_eq!((status, &member["review"]["question"]), (200, &json!(2)), "{member}");
  assert_eq!(peers.lock().unwrap().questions.len(), 2);
  turn_marks::ended(agent, "run-q1");
}

// One turn's ask of a job's seat, as the job's turn asks it: under the job's owner, at its checkout, naming the
// one agent its item names.
fn turn_ask(job: i64, turn: i64, agent: &str) -> Asked<'_> {
  Asked {
    request_key: format!("job-{job}-{turn}-1"),
    owner: format!("{}{job}", dispatch::JOB_OWNER),
    wanted: Wanted {
      role: "worker",
      path: Some("/checkouts/seated"),
      fixed: None,
      agent: Some(agent),
    },
    ..Asked::default()
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_s_next_stage_runs_on_its_agent_before_a_job_waiting_for_that_agent_starts() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let db = &ground.db;

  deepseek_balance(&ground, 5.0).await;

  let agents = agency_server::role_agents::of_role(db, "worker").await.unwrap();
  let agent = agents[0].name.as_str();
  let active = async |token: &str| dispatch::request(db, token).await.unwrap().unwrap().state == "active";

  // The first job's first stage is let in on the agent its item names.
  running(&scratch, "worker-first", 31, 71, &["Implement", "Verify"]).await;

  let first = work::job_seat(&ground, 31, &turn_ask(31, 1, agent)).await.unwrap().unwrap();

  assert_eq!(first.seat.as_deref(), Some(agent));

  // A second job naming the same agent waits for it.
  let second =
    dispatch::enqueue(db, &ground.events, &turn_ask(32, 1, agent), dispatch::JOB, async |_, _| Ok(()))
      .await
      .unwrap();

  assert!(dispatch::acquire(&ground, &second).await.unwrap().is_none());

  // The first stage's turn runs, its run seen standing by the line's pass, then ends with its report: the pass
  // reads that run gone, and agency restarts while the job stands under review.
  let mut standing = std::collections::HashSet::new();

  standing.insert("job-31".to_owned());

  for seen in [standing, std::collections::HashSet::new()] {
    let leases = dispatch::bound_leases(db).await.unwrap();

    dispatch::release_ended(db, &ground.events, &leases, &seen).await.unwrap();
  }

  agency_server::job_review::reported(&ground, 31, "Stage 1 is implemented.", None).await;
  dispatch::release_every_session(db, &ground.events).await.unwrap();
  work::paused_seats_given_back(&ground).await;

  assert!(active(&first.token).await, "the first job keeps its seat under review");

  let asked_again = dispatch::request(db, &second.token).await.unwrap().unwrap();

  assert!(
    dispatch::acquire(&ground, &asked_again).await.unwrap().is_none(),
    "the waiting job is not let in on a seat a reviewing job holds"
  );

  // The first job's next stage goes out on the seat it holds, asking nothing, while the second still waits.
  let next = work::job_seat(&ground, 31, &turn_ask(31, 2, agent)).await.unwrap().unwrap();

  assert_eq!(next.token, first.token, "the next stage runs on the seat its first stage held");
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "role_slot_requests" WHERE "owner" = 'job-31'"#, vec![])
      .await,
    1,
    "the next stage asks for no slot of its own"
  );
  assert_eq!(dispatch::request(db, &second.token).await.unwrap().unwrap().state, "waiting");

  // Only the first job's end gives the agent to the job that waited for it.
  agency_server::job_end::slot_given_back(db, &ground.events, 31, "ended").await;

  let asked_again = dispatch::request(db, &second.token).await.unwrap().unwrap();
  let let_in = dispatch::acquire(&ground, &asked_again).await.unwrap().unwrap();

  assert_eq!(let_in.seat.as_deref(), Some(agent));
}

// The order a check is told in names the language the run writes its results for a person in, read off the road
// its last turn answered on: a registration naming the languages it writes says them and that an English report
// is no reason to send the work back; a registration naming none, another road, and a job with no road kept name
// the brief's language (docs/job-review.md, 「The checks」).
#[tokio::test]
async fn the_review_names_the_language_the_run_writes_its_results_in() {
  let scratch = Scratch::new().await;

  scratch
    .run(
      r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
            "thinking", "walked", "added_at", "keeps_research", "languages")
         VALUES ('narrow', 'Narrow', 'served', 200000, 8000, 1, 1, 1, 0, '["English"]'),
                ('wide', 'Wide', 'served', 200000, 8000, 1, 1, 2, 0, NULL)"#,
      vec![],
    )
    .await;

  let road = |provider: &str, model: &str| Resumed {
    provider: provider.to_owned(),
    model: model.to_owned(),
    session: "s".to_owned(),
    effort: None,
    prefix: None,
  };
  let db = scratch.db();
  let brief = "Its results for a person are written in the language its brief names.";

  assert_eq!(
    result_language(db, Some(&road("registered", "narrow"))).await,
    "It ran on narrow, which writes a result for a person in English alone: where the brief names a language \
     outside these, its results for a person are written in English and the person reads them through a \
     translation, so an English report is no reason to send the work back."
  );

  for other in [road("registered", "wide"), road("claude-code", "narrow"), road("registered", "gone")] {
    assert_eq!(result_language(db, Some(&other)).await, brief, "{other:?}");
  }

  assert_eq!(result_language(db, None).await, brief);
}

// A check told while the receiver is cut — nothing listening at its address — stays owed with its order whole,
// and once the receiver stands again and agency starts again the order told names the run's result language: a
// run on a registration writing English alone is said to report in English, so its English report is not sent
// back for its language (docs/job-review.md, 「The checks」).
#[tokio::test(flavor = "multi_thread")]
async fn a_check_owed_across_a_cut_receiver_and_a_restart_names_the_run_s_result_language() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, live) = stand().await;
  // The receiver cut: its address holds nothing that listens, so the first try is refused at the connection.
  let cut = {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();

    format!("http://{}", listener.local_addr().unwrap())
  };
  let mut vars = live.clone();

  vars.insert("BMO_RECEIVER_URL".to_owned(), cut);

  let machine = Machine::with_env(
    vars.iter().map(|(name, value)| (name.clone(), value.clone())).chain([
      ("HOME".to_owned(), home.path().to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ]),
    home.path().join("data"),
  );
  let service = Service::new(
    scratch.db().clone(),
    Config::read(|name| HashMap::<String, String>::new().get(name).cloned()).unwrap(),
  );
  let ground = Ground::new(&service, &machine, None);

  peers.lock().unwrap().items.insert(67, item(67, "leased", json!({ "roomId": -5 })));
  scratch
    .run(
      r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
            "thinking", "walked", "added_at", "keeps_research", "languages")
         VALUES ('narrow', 'Narrow', 'served', 200000, 8000, 1, 1, 1, 0, '["English"]')"#,
      vec![],
    )
    .await;
  running(&scratch, "worker-l", 27, 67, &[]).await;
  scratch
    .run(
      r#"UPDATE "jobs" SET "review_session" = '{"provider":"registered","model":"narrow","session":"s-27"}'
         WHERE "id" = 27"#,
      vec![],
    )
    .await;

  agency_server::job_review::reported(&ground, 27, "The work is done, reported in English.", None).await;

  assert!(peers.lock().unwrap().instructed().is_empty(), "nothing reached a receiver that was cut");
  assert_eq!(owed(scratch.db(), "job-review:67").await.unwrap().len(), 1, "the check stays owed");

  // The receiver stands again and agency starts again: the owed check is told then, its order whole.
  restarted(scratch.db()).await.unwrap();
  assert_eq!(send_due(scratch.db(), &live).await.unwrap(), 1);

  let told = peers.lock().unwrap().instructed();
  let order = told.last().unwrap()["said"]["order"].as_str().unwrap().to_owned();

  assert_eq!(told.last().unwrap()["kind"], "job-review:67");
  assert!(
    order.contains(
      "so far. It ran on narrow, which writes a result for a person in English alone: where the brief names a \
       language outside these, its results for a person are written in English and the person reads them \
       through a translation, so an English report is no reason to send the work back.\n\nThe check due now is \
       promises:"
    ),
    "{order}"
  );
  assert!(
    order.contains("The worker's report:\n\nThe work is done, reported in English."),
    "{order}"
  );
  // The verdict is the call alone: a reply the round stands on is kept with the keep mark, never with an empty
  // answer, which reads as no answer at all.
  assert!(
    order.contains(
      "Give the verdict by calling review_job on job 27 with one of its actions: `ask`, `reject` or `pass`. \
       The call alone is the verdict, and the room's reply says nothing of it"
    ),
    "{order}"
  );
  assert!(order.contains("answer <<KEEP>> after the call to keep that reply as it is"), "{order}");
  assert!(!order.contains("<<SKIP>>"), "{order}");
}
