//! A job's run log asked of a running server in the shapes the job unit reads and writes it: its lines read
//! by the job across its runs, a note written into a run's log becoming the job's last line, and a turn
//! record kept and read back under its run.

use serde_json::{Value, json};

use crate::support::scratch::Scratch;
use crate::support::standing;

async fn get(port: u16, token: &str, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

async fn post(port: u16, token: &str, path: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .post(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .json(&body)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_server_reads_a_job_s_lines_and_keeps_its_notes_and_turn_records() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";
  let long = "x".repeat(2500);

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  // Job 12's lines across two runs: a call kept cut with its whole arguments in its detail, a call whose
  // arguments are no JSON, a branch's call under a parent, a result kept cut, the request lines and a
  // compaction. The times are now, since the run log's pruner keeps a window and the server prunes it.
  let now = agency_server::now_ms();
  scratch
    .run(
      r#"INSERT INTO "agent_log"
           ("id", "agent", "run", "at", "kind", "name", "parent", "ref", "text", "detail", "job_id",
            "source", "source_id", "detail_complete", "detail_length")
         VALUES
           (1, 'helper', 'unit-a', $2 + 0, 'tool', 'Bash', NULL, 'call-1', '{"command":"car', '{"command":"cargo check"}', 12, 'claude-code', 'req-1', 1, 25),
           (2, 'helper', 'unit-a', $2 + 1, 'tool-result', NULL, NULL, 'call-1', 'ok', NULL, 12, 'claude-code', 'req-1', 0, 90000),
           (3, 'helper', 'unit-a', $2 + 2, 'text', NULL, NULL, NULL, 'thinking aloud', NULL, 12, 'claude-code', 'req-1', 1, 14),
           (4, 'helper', 'unit-b', $2 + 3, 'tool', 'Agent', 'call-0', 'call-2', '{}', NULL, 12, 'claude-code', 'req-2', 1, 2),
           (5, 'helper', 'unit-b', $2 + 4, 'thinking', NULL, NULL, NULL, 'hmm', NULL, 12, 'claude-code', 'req-3', 1, 3),
           (6, 'helper', 'unit-b', $2 + 5, 'tool', 'Read', NULL, 'call-3', $1, NULL, 12, NULL, 'req-3', 1, 2500),
           (7, 'helper', 'unit-b', $2 + 6, 'compact', NULL, NULL, NULL, 'compacted', NULL, 12, NULL, NULL, 1, 9),
           (8, 'other', 'unit-c', $2 + 7, 'tool', 'Bash', NULL, 'call-9', '{}', NULL, 13, NULL, 'req-9', 1, 2)"#,
      vec![long.clone().into(), now.into()],
    )
    .await;
  scratch
    .run(r#"SELECT setval(pg_get_serial_sequence('agent_log', 'id'), 100)"#, vec![])
    .await;
  scratch
    .run(
      r#"UPDATE "agent_log" SET "metadata" =
           '{"toolUseResult":{"bashEditDiff":{"files":[],"moreFiles":0,"changedFiles":["/work/notes.txt"]}}}'
         WHERE "id" = 2"#,
      vec![],
    )
    .await;

  let (status, lines) = get(port, token, "/log-lines?jobId=12&limit=2").await;

  assert_eq!(status, 200, "{lines}");
  assert_eq!(lines["items"].as_array().unwrap().len(), 2);
  assert_eq!(lines["items"][0]["id"], json!(1));
  assert_eq!(lines["items"][0]["agentName"], json!("helper"));
  assert_eq!(lines["items"][0]["jobId"], json!(12));
  assert_eq!(lines["items"][0]["kind"], json!("tool"));
  assert_eq!(lines["items"][1]["id"], json!(2));
  assert_eq!(lines["items"][1]["metadata"]["toolUseResult"]["bashEditDiff"]["moreFiles"], json!(0));
  assert_eq!(
    lines["items"][1]["metadata"]["toolUseResult"]["bashEditDiff"]["changedFiles"][0],
    json!("/work/notes.txt")
  );
  assert_eq!(lines["items"][0]["metadata"], Value::Null);
  assert_eq!(lines["more"], json!(true), "{lines}");

  // The filters meet: the kinds named, an agent's lines alone, and the page just before a line, oldest first.
  let ids = |lines: &Value| -> Vec<i64> {
    lines["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|line| line["id"].as_i64().unwrap())
      .collect()
  };
  let (_, kinds) = get(port, token, "/log-lines?jobId=12&kinds=tool,compact").await;

  assert_eq!(ids(&kinds), [1, 4, 6, 7], "{kinds}");
  assert_eq!(kinds["more"], json!(false));

  let (_, other) = get(port, token, "/log-lines?agentName=other").await;

  assert_eq!(ids(&other), [8], "{other}");
  assert_eq!(ids(&get(port, token, "/log-lines?jobId=12&agentName=other").await.1), Vec::<i64>::new());

  let (_, earlier) = get(port, token, "/log-lines?jobId=12&before=6&limit=2").await;

  assert_eq!(ids(&earlier), [4, 5], "{earlier}");
  assert_eq!(earlier["more"], json!(true));
  assert_eq!(ids(&get(port, token, "/log-lines?jobId=12&after=2&before=5").await.1), [3, 4]);

  // No run, job or agent named: nothing to read.
  assert_eq!(get(port, token, "/log-lines").await.0, 400);

  // A note written under the job's run becomes its line, kept whole with its road and state.
  let note = json!({
    "agent": "helper", "run": "unit-b", "chat": -7, "job": 12, "text": "claude-code · m",
    "kind": "note", "source": "claude-code", "state": "running",
  });

  assert_eq!(post(port, token, "/log-lines", note.clone()).await.0, 201);

  assert_eq!(
    scratch
      .count(
        r#"SELECT count(*) FROM "agent_log" WHERE "kind" = 'note' AND "agent" = 'helper' AND "run" = 'unit-b'
           AND "chat_id" = -7 AND "job_id" = 12 AND "text" = 'claude-code · m' AND "source" = 'claude-code'
           AND "state" = 'running'"#,
        vec![],
      )
      .await,
    1
  );

  let mut stateless = note.clone();

  stateless["state"] = json!("dancing");
  assert_eq!(post(port, token, "/log-lines", stateless).await.0, 400);
  assert_eq!(
    post(port, token, "/log-lines", json!({ "agent": "helper", "run": "unit-b" })).await.0,
    400
  );

  // The note is read back as a line of its run.
  let (status, kept) = get(port, token, "/log-lines?run=unit-b").await;
  let note = kept["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|line| line["kind"] == "note")
    .expect("the note is kept");

  assert_eq!(status, 200, "{kept}");
  assert_eq!(note["text"], json!("claude-code · m"));
  assert_eq!(note["jobId"], json!(12));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_s_log_reads_across_its_runs_newest_before_after_and_at_a_moment() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  // Five lines in one run and five in the next, in order, and another agent's line between. The times
  // are now, not a fixed epoch, since the run log's pruner keeps a window and the server prunes it.
  let now = agency_server::now_ms();
  scratch
    .run(
      r#"INSERT INTO "agent_log" ("id", "agent", "run", "at", "kind", "text")
         VALUES
           (1, 'worker', 'unit-a', $1 + 0, 'text', 'a1'),
           (2, 'worker', 'unit-a', $1 + 1, 'text', 'a2'),
           (3, 'worker', 'unit-a', $1 + 2, 'text', 'a3'),
           (4, 'worker', 'unit-a', $1 + 3, 'text', 'a4'),
           (5, 'worker', 'unit-a', $1 + 4, 'text', 'a5'),
           (6, 'other', 'unit-x', $1 + 5, 'text', 'x1'),
           (7, 'worker', 'unit-b', $1 + 6, 'text', 'b1'),
           (8, 'worker', 'unit-b', $1 + 7, 'text', 'b2'),
           (9, 'worker', 'unit-b', $1 + 8, 'text', 'b3'),
           (10, 'worker', 'unit-b', $1 + 9, 'text', 'b4'),
           (11, 'worker', 'unit-b', $1 + 10, 'text', 'b5')"#,
      vec![now.into()],
    )
    .await;
  scratch
    .run(
      r#"UPDATE "agent_log" SET "metadata" =
           '{"toolUseResult":{"bashEditDiff":{"files":[],"moreFiles":0,"changedFiles":["/work/b5.txt"]}}}'
         WHERE "id" = 11"#,
      vec![],
    )
    .await;

  // The newest page: the last three lines, oldest first, with another page older.
  let (status, newest) = get(port, token, "/log/agents/worker/lines?limit=3").await;

  assert_eq!(status, 200, "{newest}");
  assert_eq!(
    newest["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|line| line["id"].as_i64().unwrap())
      .collect::<Vec<_>>(),
    [9, 10, 11]
  );
  assert_eq!(newest["hasMore"], json!(true));
  assert_eq!(newest["hasOlder"], json!(false));
  assert_eq!(newest["items"][0]["fields"]["run"], json!("unit-b"));
  assert_eq!(
    newest["items"][2]["metadata"]["toolUseResult"]["bashEditDiff"]["changedFiles"][0],
    json!("/work/b5.txt")
  );

  // The page before the oldest of those: three more, oldest first, still across the next run.
  let (status, older) = get(port, token, "/log/agents/worker/lines?limit=3&before=9").await;

  assert_eq!(status, 200, "{older}");
  assert_eq!(
    older["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|line| line["id"].as_i64().unwrap())
      .collect::<Vec<_>>(),
    [5, 7, 8]
  );
  assert_eq!(older["hasMore"], json!(true));
  assert!(
    older["items"]
      .as_array()
      .unwrap()
      .iter()
      .any(|line| line["fields"]["run"] == json!("unit-a")),
    "the page reaches back into the run before"
  );

  // The page after an id reads forward; the last of the log has no more.
  let (status, forward) = get(port, token, "/log/agents/worker/lines?limit=10&after=7").await;

  assert_eq!(status, 200, "{forward}");
  assert_eq!(
    forward["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|line| line["id"].as_i64().unwrap())
      .collect::<Vec<_>>(),
    [8, 9, 10, 11]
  );
  assert_eq!(forward["hasMore"], json!(false));

  // The page standing at a moment opens at the head of the run its line belongs to, and says older lines
  // stand before it and newer ones after.
  let (status, at) = get(port, token, &format!("/log/agents/worker/lines?limit=3&at={}", now + 7)).await;

  assert_eq!(status, 200, "{at}");
  assert_eq!(at["hasOlder"], json!(true));
  assert_eq!(at["hasMore"], json!(true));
  assert_eq!(at["items"][0]["id"], json!(7));

  // An agent with no line reads an empty page.
  let (status, none) = get(port, token, "/log/agents/no-such-agent/lines?limit=3").await;

  assert_eq!((status, none["items"].as_array().unwrap().len()), (200, 0));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_role_s_log_reads_its_agents_lines_and_a_job_s_lines_stand_under_its_item_s_agent() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let now = agency_server::now_ms();

  scratch
    .run(r#"INSERT INTO "role_agents" ("name", "role") VALUES ('agent-a', 'role-a')"#, vec![])
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt", "item_agent")
         VALUES (40, 4, 1, 'runner', 1, 'checkout-a', 'running', $1, 'the work', 'agent-a')"#,
      vec![now.into()],
    )
    .await;

  // A job's line is written under the agent its item named, not the checkout it runs in.
  let mut log = agency_server::run_log::RunLog::new(agency_server::run_log::Target {
    agent: "checkout-a".to_owned(),
    run: "job-40".to_owned(),
    chat_id: None,
    job_id: Some(40),
  });

  log
    .write(scratch.db(), agency_server::run_log::lines::Line::note("the job's note".to_owned()))
    .await
    .unwrap();
  scratch
    .run(
      r#"INSERT INTO "agent_log" ("agent", "run", "at", "kind", "text")
         VALUES ('role-a', 'unit-r', $1, 'text', 'the role own line'), ('stranger', 'unit-s', $1, 'text', 'not hers')"#,
      vec![now.into()],
    )
    .await;

  let texts = |page: &Value| -> Vec<String> {
    page["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|line| line["text"].as_str().unwrap_or_default().to_owned())
      .collect()
  };
  let (status, agent) = get(port, token, "/log/agents/agent-a/lines").await;

  assert_eq!(status, 200, "{agent}");
  assert_eq!(texts(&agent), ["the job's note"], "under the item's agent");

  let (status, checkout) = get(port, token, "/log/agents/checkout-a/lines").await;

  assert_eq!((status, texts(&checkout).len()), (200, 0), "nothing under the checkout");

  let (status, role) = get(port, token, "/log/agents/role-a/lines").await;

  assert_eq!(status, 200, "{role}");
  assert_eq!(
    texts(&role),
    ["the job's note", "the role own line"],
    "the role reads its agents' lines with its own"
  );

  // Each agent's lines are walked apart and merged by id, so a page cut by its limit takes the newest lines of the
  // role and of its agent together, and the page before it goes on where it stopped.
  scratch
    .run(
      r#"INSERT INTO "agent_log" ("agent", "run", "at", "kind", "text")
         VALUES ('agent-a', 'unit-m', $1, 'text', 'm1'), ('role-a', 'unit-m', $1, 'text', 'm2'),
                ('stranger', 'unit-s', $1, 'text', 'not hers'), ('agent-a', 'unit-m', $1, 'text', 'm3'),
                ('role-a', 'unit-m', $1, 'text', 'm4')"#,
      vec![now.into()],
    )
    .await;

  let (status, newest) = get(port, token, "/log/agents/role-a/lines?limit=3").await;

  assert_eq!(status, 200, "{newest}");
  assert_eq!(texts(&newest), ["m2", "m3", "m4"]);
  assert_eq!(newest["hasMore"], json!(true));

  let oldest = newest["items"][0]["id"].as_i64().unwrap();
  let (status, before) =
    get(port, token, &format!("/log/agents/role-a/lines?limit=3&before={oldest}")).await;

  assert_eq!(status, 200, "{before}");
  assert_eq!(texts(&before), ["the job's note", "the role own line", "m1"]);
  assert_eq!(before["hasMore"], json!(false));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_clock_note_reads_with_its_fields_and_one_filed_before_them_reads_them_back_out_of_its_words() {
  use agency_server::door::tool_clock::{ClockEvent, ClockNote};
  use agency_server::run_log::lines::Line;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt", "item_agent")
         VALUES (41, 4, 1, 'runner', 1, 'checkout-a', 'running', $1, 'the work', 'agent-a')"#,
      vec![agency_server::now_ms().into()],
    )
    .await;

  let mut log = agency_server::run_log::RunLog::new(agency_server::run_log::Target {
    agent: "checkout-a".to_owned(),
    run: "job-41".to_owned(),
    chat_id: None,
    job_id: Some(41),
  });
  let running = ClockNote {
    run: "job-41".to_owned(),
    call: Some("toolu_1".to_owned()),
    tool: "Bash".to_owned(),
    command: "sleep 600".to_owned(),
    pid: Some(4242),
    elapsed_ms: Some(62_000),
    event: ClockEvent::Running,
  };

  log.write(scratch.db(), Line::clock(&running)).await.unwrap();
  log
    .write(
      scratch.db(),
      Line::note("job-41: tool Bash still running pid=4242 after 122s: sleep 600".to_owned()),
    )
    .await
    .unwrap();
  log.write(scratch.db(), Line::note("the job's own note".to_owned())).await.unwrap();

  let (status, page) = get(port, token, "/log/agents/agent-a/lines").await;

  assert_eq!(status, 200, "{page}");

  let items = page["items"].as_array().unwrap();

  assert_eq!(items[0]["text"], json!(running.sentence()), "the words stay the sentence");
  assert_eq!(items[0]["metadata"], running.fields());
  assert_eq!(
    items[1]["metadata"]["toolClock"],
    json!({ "event": "running", "call": null, "tool": "Bash", "command": "sleep 600", "pid": 4242, "elapsedMs": 122_000 }),
    "an older line reads its fields back out of its words"
  );
  assert_eq!(items[2]["metadata"], Value::Null, "any other note carries none");
}
