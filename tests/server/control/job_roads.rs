//! A job carried across its roads (docs/cache-fallback.md, 「One road resolution」; docs/api.md, 「assignments
//! and jobs」): a turn whose road stopped answering carried again on the next road, the same job, its prompt
//! opened with the line saying it continues the work and its report saying where it ran; at most once on
//! each provider; a failure of the work itself moving nothing; and a job whose item names its model carried
//! on that model alone, over the repository's pin, waiting for it rather than falling back; and a job's turn on
//! Claude Code carried by the account walk itself, going out on the next account where the first answers with
//! the firewall's 403, the refusal named in its log and its roads, and ending on the login failure a person
//! settles where every account refuses, rather than waiting.

use std::collections::{HashMap, VecDeque};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::support::grounds::{OrderEntry, deepseek_balance, set_provider_order, signed_in};
use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::claude_accounts::{Account, CREDENTIAL_FILE, machine_config_dir};
use agency_server::control::work::{Carried, Carrier, carry_job, continued, road_stopped};
use agency_server::job_run::claude::failure::{Failure as ClaudeFailure, of_unanswered};
use agency_server::job_run::claude::renewal::Renewing;
use agency_server::job_run::claude::turn::{Beside, JobMoves, ended};
use agency_server::job_run::claude::walk::{Attempt, PassedOver, Walking, on_accounts};
use agency_server::order::Entry;
use agency_server::places::Ground;
use agency_server::queue::routes::{clear_block_by_id, kept_blocks};
use agency_server::rest::{DeploymentResting, RestKind};
use agency_server::road::Road;
use agency_server::run_log::{RunLog, Target};
use agency_server::seeds::deployment;
use agent_session::{EngineError, Failure, Outcome, TurnEnd};
use harness_providers::Machine;
use sea_orm::DatabaseConnection;
use serde_json::json;
use std::time::Duration;

use crate::support::rows::repository;
use crate::support::scratch::Scratch;

fn ground_on(db: DatabaseConnection, home: &std::path::Path) -> Ground {
  ground_naming(db, home, &std::collections::BTreeMap::new())
}

// A ground as `ground_on` stands it, with the peers a test stands in for named beside its own variables.
fn ground_naming(
  db: DatabaseConnection,
  home: &std::path::Path,
  peers: &std::collections::BTreeMap<String, String>,
) -> Ground {
  let variables: HashMap<String, String> = HashMap::new();
  let service = Service::new(db, Config::read(|name| variables.get(name).cloned()).unwrap());
  let mut env = vec![
    ("HOME".to_owned(), home.to_string_lossy().into_owned()),
    ("PATH".to_owned(), "/nowhere".to_owned()),
  ];

  env.extend(peers.iter().map(|(name, value)| (name.clone(), value.clone())));

  Ground::new(&service, &Machine::with_env(env, home.join("data")), None)
}

fn limit() -> TurnEnd {
  TurnEnd {
    failure: Some(Failure::Limit {
      resets_at: None,
      account: None,
      silenced: false,
      model: None,
    }),
    ..TurnEnd::failed("job-7", "the login's window is full", None)
  }
}

/// A stand-in for the worker's session: each turn it is handed kept, each answered by the next end it holds.
struct Ends {
  ends: VecDeque<TurnEnd>,
  turns: Vec<(Road, Entry, String)>,
}

impl Carrier for Ends {
  async fn carry(&mut self, taken: &Road, entry: &Entry, prompt: &str) -> Result<TurnEnd, String> {
    self.turns.push((taken.clone(), entry.clone(), prompt.to_owned()));

    self.ends.pop_front().ok_or_else(|| "no end is left".to_owned())
  }
}

// What the resident program left where the API's firewall refused its request, as the jobs it ended wrote it.
const CLOUDFLARE: &str = "Please run /login · API Error: 403 Attention Required! | Cloudflare";

// The model the job's item names, which its Claude Code road goes out on.
const ITEM_MODEL: &str = "item-model";

// A job's turn on Claude Code carried by the account walk itself: each login the turn goes out on kept with
// the prompt it went out with, the logins named answering with the firewall's refusal and any other with the
// work done, and every login passed over told and kept as a job's turn tells and keeps it.
struct Walked<'g> {
  ground: &'g Ground,
  refusing: Vec<&'static str>,
  went_out: Vec<(String, String)>,
  prompt: String,
  log: RunLog,
}

impl Renewing for Walked<'_> {}

impl Attempt<String> for Walked<'_> {
  async fn attempt(&mut self, account: Option<Account>) -> Result<String, ClaudeFailure> {
    let id = account.expect("a login").id;

    self.went_out.push((id.clone(), self.prompt.clone()));

    if self.refusing.contains(&id.as_str()) {
      return Err(of_unanswered(CLOUDFLARE, "coder"));
    }

    Ok("Done.".to_owned())
  }

  async fn passed_over(&mut self, passed: &PassedOver<'_>) {
    let mut moves = JobMoves::of(self.ground, &mut self.log, Some(7), ITEM_MODEL);

    if let Some(prompt) = moves.passed_over(passed, &self.prompt.clone()).await {
      self.prompt = prompt;
    }
  }
}

// The job's turns, each carried account by account by the walk, its end the turn's end as a job's turn ends.
struct Walks<'g> {
  ground: &'g Ground,
  refusing: Vec<&'static str>,
  turns: Vec<Vec<(String, String)>>,
}

impl Carrier for Walks<'_> {
  async fn carry(&mut self, taken: &Road, _entry: &Entry, prompt: &str) -> Result<TurnEnd, String> {
    let mut walked = Walked {
      ground: self.ground,
      refusing: self.refusing.clone(),
      went_out: vec![],
      prompt: prompt.to_owned(),
      log: RunLog::new(Target {
        agent: "helper".to_owned(),
        run: "job-7".to_owned(),
        chat_id: None,
        job_id: Some(7),
      }),
    };
    let end = match on_accounts(
      &Walking {
        ground: self.ground,
        model: &taken.model,
      },
      "coder",
      None,
      &mut walked,
    )
    .await
    {
      Ok(answer) => TurnEnd::answered("job-7", answer),
      Err(failure) => ended("job-7", Err(failure), Beside::default()),
    };

    self.turns.push(walked.went_out);
    Ok(end)
  }
}

// The store standing, its seeded login named as the owner names his first and a second login after it, each
// signed in with a credential an hour from its moment, which lifts a mark of waiting for a person at once; and
// the job's item naming Claude Code's model, so the job has no other provider to move to.
async fn two_logins(scratch: &Scratch, home: &Path) -> Ground {
  let ground = standing(scratch, home).await;
  let ends = chrono::Utc::now().timestamp_millis() + 3_600_000;

  for (id, name, place) in [("default", "claude-01", 1_i64), ("second", "second", 2)] {
    let login = home.join("logins").join(id);

    std::fs::create_dir_all(&login).unwrap();
    std::fs::write(
      login.join(CREDENTIAL_FILE),
      json!({ "claudeAiOauth": { "accessToken": "a", "refreshToken": "r", "expiresAt": ends } })
        .to_string(),
    )
    .unwrap();
    scratch
      .run(
        r#"INSERT INTO "claude_accounts" ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at")
           VALUES ($1, $2, $3, 1, 'directory', $4, 1)
           ON CONFLICT ("id") DO UPDATE SET "name" = $2, "place" = $3, "credential_path" = $4"#,
        vec![id.into(), name.into(), place.into(), login.to_string_lossy().into_owned().into()],
      )
      .await;
  }

  item_names(scratch, "claude-code", ITEM_MODEL).await;
  ground
}

// The notes the job's log holds, in the order they were written.
async fn job_notes(scratch: &Scratch) -> Vec<String> {
  store::all(
    scratch.db(),
    r#"SELECT "text" FROM "agent_log" WHERE "job_id" = 7 AND "kind" = 'note' ORDER BY "id""#,
    vec![],
  )
  .await
  .unwrap()
  .iter()
  .map(|row| row.try_get("", "text").unwrap())
  .collect()
}

// A job whose first Claude account answers with the firewall's 403 goes out again, in the same turn, on the
// next account: the refusing account rests alone, the job's log names it and its words, its roads keep the
// account it moved to with that refusal as the reason, its prompt there says it continues the work, and its
// report says where it ran.
#[tokio::test(flavor = "multi_thread")]
async fn a_job_whose_first_account_answers_a_403_goes_out_on_the_next_with_the_refusal_named() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = two_logins(&scratch, home.path()).await;
  let mut carrier = Walks {
    ground: &ground,
    refusing: vec!["default"],
    turns: vec![],
  };

  let (state, outcome, report, _) =
    carry_job(&ground, &job(), &mut carrier).await.unwrap().expect("an end");

  assert_eq!((state.as_str(), outcome), ("ended", None), "the job answered on the next account");

  let turn = &carrier.turns[..];
  let refused = format!("Claude account claude-01 (default) refused the turn: \"{CLOUDFLARE}\"");

  assert_eq!(turn.len(), 1, "one turn, carried across the accounts inside it");
  assert_eq!(turn[0].iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["default", "second"]);
  assert_eq!(turn[0][0].1, job().prompt, "the first account takes the job's own prompt");
  // The login left cannot be asked, so its run log stands as the handoff the next login's turn opens on.
  assert!(turn[0][1].1.starts_with("## The last session's handoff"), "{}", turn[0][1].1);
  assert!(
    turn[0][1].1.contains("\n\nThis job already ran on claude-code · item-model (default)"),
    "{}",
    turn[0][1].1
  );
  assert!(turn[0][1].1.contains(&refused), "{}", turn[0][1].1);
  assert!(turn[0][1].1.ends_with(job().prompt), "{}", turn[0][1].1);

  assert_eq!(
    job_notes(&scratch).await,
    [format!("{refused}. The job goes out again on Claude account second.")]
  );

  let roads: serde_json::Value =
    serde_json::from_str(&job_column(&scratch, "roads").await.unwrap()).unwrap();

  assert_eq!(roads[0]["account"], "default");
  assert_eq!(roads[1]["account"], "second");
  assert_eq!(roads[1]["model"], ITEM_MODEL);
  assert_eq!(roads[1]["reason"], refused.as_str());

  let report = report.unwrap_or_default();

  assert!(report.starts_with("Done."), "{report}");
  assert!(
    report.contains("claude-code · item-model (default) until it stopped answering"),
    "{report}"
  );

  let rest: Option<String> = store::one(
    scratch.db(),
    r#"SELECT "value" FROM "settings" WHERE "key" = 'rest_until:default'"#,
    vec![],
  )
  .await
  .unwrap()
  .map(|row| row.try_get("", "value").unwrap());

  assert!(rest.is_some(), "the refusing account rests");
  assert_eq!(
    store::one(
      scratch.db(),
      r#"SELECT 1 AS "one" FROM "settings" WHERE "key" = 'rest_until:second'"#,
      vec![]
    )
    .await
    .unwrap()
    .map(|_| ()),
    None,
    "no other account rests for it"
  );
}

// A job every Claude account of which answers with the firewall's 403 ends failed, on the login failure naming
// each account and its words and saying a person must sign in, never on the last refusal's bare line and never
// waiting for the refusals' rests to end; its log names each refusal, that no account is left, and the
// failure its road stopped on.
#[tokio::test(flavor = "multi_thread")]
async fn a_job_every_claude_account_of_which_refuses_ends_on_the_login_failure_a_person_settles() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = two_logins(&scratch, home.path()).await;
  let mut carrier = Walks {
    ground: &ground,
    refusing: vec!["default", "second"],
    turns: vec![],
  };

  let ended = tokio::time::timeout(Duration::from_secs(20), carry_job(&ground, &job(), &mut carrier)).await;
  let (state, outcome, report, _) =
    ended.expect("the job ends rather than waits").unwrap().expect("an end");

  assert_eq!((state.as_str(), outcome.as_deref()), ("ended", Some("failed")));
  assert_eq!(carrier.turns.len(), 1, "one turn, on every account");
  assert_eq!(
    carrier.turns[0].iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
    ["default", "second"]
  );

  let report = report.unwrap_or_default();
  let failure = format!(
    "claude coder: every Claude account of the road refused this run, and a person must sign in to each \
     again: claude-01 (default) answered \"{CLOUDFLARE}\"; second answered \"{CLOUDFLARE}\""
  );

  assert!(report.starts_with(&failure), "{report}");
  let refused = |account: &str| format!("Claude account {account} refused the turn: \"{CLOUDFLARE}\"");

  assert_eq!(
    job_notes(&scratch).await,
    [
      format!("{}. The job goes out again on Claude account second.", refused("claude-01 (default)")),
      format!("{}. No Claude account of the road is left.", refused("second")),
      format!("The job's road claude-code · item-model (second) stopped answering: {failure}"),
    ]
  );
  assert_eq!(job_column(&scratch, "held_for").await, None, "it never waited");
}

// A store whose order is DeepSeek, its key in place, then Claude Code on its one seeded login, and a job in a
// repository's checkout on it; every other provider is missing on a machine with nothing on its path.
async fn standing(scratch: &Scratch, home: &std::path::Path) -> Ground {
  standing_naming(scratch, home, &std::collections::BTreeMap::new()).await
}

// The same store and job, on a ground that names the peers a test stands in for.
async fn standing_naming(
  scratch: &Scratch,
  home: &std::path::Path,
  peers: &std::collections::BTreeMap<String, String>,
) -> Ground {
  let data = home.join("data");
  let env = data.join("env");

  std::fs::create_dir_all(&env).unwrap();
  for (name, key) in [("deepseek.key", "a key\n"), ("llm-gateway.key", "a key\n")] {
    let file = env.join(name);

    std::fs::write(&file, key).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
  }

  let members = data.join("llm-gateway");

  std::fs::create_dir_all(&members).unwrap();
  std::fs::write(
    members.join("members.json"),
    r#"[{"name":"deepseek-flash","road":"deepseek","provider":"deepseek","model":"deepseek-flash","api_base":"https://api.deepseek.com/v1","key_file":"deepseek.key","context_tokens":1048576,"answer_tokens":32768,"thinking":true}]"#,
  )
  .unwrap();

  let order: Vec<OrderEntry> = ["deepseek", "claude-code", "codex", "openrouter", "registered"]
    .iter()
    .map(|name| OrderEntry {
      name: (*name).to_owned(),
      enabled: true,
    })
    .collect();

  let ground = ground_naming(scratch.db().clone(), home, peers);

  store::seed::settle(scratch.db(), &deployment(&ground.machine())).await.unwrap();
  signed_in(&machine_config_dir(&ground.machine()));
  crate::support::rows::seated_on_order(scratch.db(), "worker").await;
  set_provider_order(scratch.db(), &order).await.unwrap();
  std::fs::create_dir_all(home.join("helper")).unwrap();
  repository(scratch.db(), "helper", &home.join("helper").to_string_lossy(), &[]).await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "token_count", "started_at", "prompt")
         VALUES (7, 70, 1, 'harness', 1, 'helper', 'running', 0, 0, '# An item')"#,
      vec![],
    )
    .await;

  ground
}

fn job() -> Carried<'static> {
  Carried {
    id: 7,
    worker: "helper",
    holder: "harness",
    prompt: "# An item\n\nDo the work.",
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_road_that_stopped_answering_leaves_the_job_s_run_log_as_the_handoff_the_next_road_opens_on() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, vars) = crate::support::peers::stand().await;
  let ground = standing_naming(&scratch, home.path(), &vars).await;
  let mut log = RunLog::new(Target {
    agent: "helper".to_owned(),
    run: "job-7".to_owned(),
    chat_id: None,
    job_id: Some(7),
  });

  // The first road's session logged where the work stood before its road stopped answering.
  log
    .write(
      scratch.db(),
      agency_server::run_log::lines::Line::note("Edited src/area.rs; its tests still fail.".to_owned()),
    )
    .await;

  let mut carrier = Ends {
    ends: VecDeque::from([limit(), TurnEnd::answered("job-7", "Done.")]),
    turns: vec![],
  };
  carry_job(&ground, &job(), &mut carrier).await.unwrap().expect("an end");

  // The turn on the next road opens on the handoff, ahead of the line saying it continues the work.
  let next = &carrier.turns[1].2;
  let opened = next.find("## The last session's handoff").expect("the handoff opens the turn");

  assert!(next.contains("Edited src/area.rs; its tests still fail."), "{next}");
  assert!(opened < next.find("already ran on deepseek").unwrap(), "{next}");
  assert!(next.ends_with(job().prompt), "{next}");

  // The handoff stands on the item as a note of its own kind, said to come from the run log.
  let notes = peers.lock().unwrap().notes.get(&70).cloned().unwrap_or_default();

  assert_eq!(notes.len(), 1, "{notes:?}");
  assert_eq!(notes[0]["labels"], json!({ "kind": "handoff", "job": "7", "source": "log" }));
  assert!(
    notes[0]["text"].as_str().unwrap().contains("Edited src/area.rs; its tests still fail."),
    "{notes:?}"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_whose_road_stopped_answering_goes_out_again_on_the_next_road_and_says_so() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = standing(&scratch, home.path()).await;
  let mut carrier = Ends {
    ends: VecDeque::from([limit(), TurnEnd::answered("job-7", "Done.")]),
    turns: vec![],
  };

  let (state, outcome, report, _) =
    carry_job(&ground, &job(), &mut carrier).await.unwrap().expect("an end");

  assert_eq!((state.as_str(), outcome), ("ended", None));
  assert_eq!(
    carrier.turns.iter().map(|(road, _, _)| road.provider.as_str()).collect::<Vec<_>>(),
    vec!["deepseek", "claude-code"]
  );
  assert_eq!(carrier.turns[0].2, job().prompt, "the first road takes the job's own prompt");
  assert!(carrier.turns[1].2.contains("already ran on deepseek"), "{}", carrier.turns[1].2);
  assert!(carrier.turns[1].2.ends_with(job().prompt), "{}", carrier.turns[1].2);

  let report = report.unwrap_or_default();

  assert!(report.starts_with("Done."), "{report}");
  assert!(report.contains("The job ran on deepseek"), "{report}");

  let roads: String = store::one(scratch.db(), r#"SELECT "roads" FROM "jobs" WHERE "id" = 7"#, vec![])
    .await
    .unwrap()
    .unwrap()
    .try_get("", "roads")
    .unwrap();
  let roads: serde_json::Value = serde_json::from_str(&roads).unwrap();

  assert_eq!(roads[0]["provider"], "deepseek");
  assert_eq!(roads[0]["reason"], serde_json::Value::Null);
  assert_eq!(roads[1]["provider"], "claude-code");
  assert_eq!(roads[1]["reason"], "the login's window is full");
}

// A job carried with one turn its carrier answers `answer` with: the outcome and report that turn leaves.
async fn ended_with(answer: &str) -> (Option<String>, String) {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = standing(&scratch, home.path()).await;
  let mut carrier = Ends {
    ends: VecDeque::from([TurnEnd::answered("job-7", answer)]),
    turns: vec![],
  };
  let (_, outcome, report, _) = carry_job(&ground, &job(), &mut carrier).await.unwrap().expect("an end");

  (outcome, report.unwrap_or_default())
}

/// an answer that is a tool call written as text is no report, and an ordinary answer still stands as one
#[tokio::test(flavor = "multi_thread")]
async fn an_answer_written_as_a_tool_call_leaves_no_report_and_an_ordinary_answer_stands() {
  let raw = "<tool_call><function=Read><parameter=file_path>/tmp/one</parameter></function></tool_call>";
  let (outcome, report) = ended_with(raw).await;

  assert_eq!(outcome.as_deref(), Some("failed"));
  assert!(!report.contains("<tool_call>"), "the call's own text is no report: {report}");
  assert!(report.contains("tool call written as text"), "{report}");

  let (outcome, report) = ended_with("The work is done.").await;

  assert_eq!(outcome, None);
  assert_eq!(report, "The work is done.");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_goes_out_at_most_once_on_each_provider_and_then_ends_as_its_last_turn_did() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = standing(&scratch, home.path()).await;
  let mut carrier = Ends {
    ends: VecDeque::from([limit(), limit(), TurnEnd::answered("job-7", "never asked")]),
    turns: vec![],
  };

  let (_, outcome, report, _) = carry_job(&ground, &job(), &mut carrier).await.unwrap().expect("an end");

  assert_eq!(carrier.turns.len(), 2, "each provider once");
  assert_eq!(outcome.as_deref(), Some("failed"));
  assert!(report.unwrap_or_default().contains("The job ran on deepseek"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failure_of_the_work_itself_moves_nothing() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = standing(&scratch, home.path()).await;
  let mut carrier = Ends {
    ends: VecDeque::from([TurnEnd::failed("job-7", "the tests failed: 3 of 40", None)]),
    turns: vec![],
  };

  let (_, outcome, report, _) = carry_job(&ground, &job(), &mut carrier).await.unwrap().expect("an end");

  assert_eq!(carrier.turns.len(), 1);
  assert_eq!(outcome.as_deref(), Some("failed"));
  assert_eq!(report.as_deref(), Some("the tests failed: 3 of 40"));
}

// The job's row naming the model its item names, and its repository pinned to DeepSeek.
async fn item_names(scratch: &Scratch, provider: &str, model: &str) {
  scratch
    .run(
      r#"UPDATE "jobs" SET "item_provider" = $1, "item_model" = $2 WHERE "id" = 7"#,
      vec![provider.to_owned().into(), model.to_owned().into()],
    )
    .await;
  scratch
    .run(
      r#"UPDATE "repositories" SET "pinned_provider" = 'deepseek', "pinned_model" = 'deepseek-flash'
         WHERE "name" = 'helper'"#,
      vec![],
    )
    .await;
}

async fn job_column(scratch: &Scratch, column: &str) -> Option<String> {
  store::one(scratch.db(), &format!(r#"SELECT "{column}" FROM "jobs" WHERE "id" = 7"#), vec![])
    .await
    .unwrap()
    .unwrap()
    .try_get("", column)
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_whose_item_names_its_model_goes_out_on_it_over_the_pin_and_never_moves_off_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = standing(&scratch, home.path()).await;
  let mut carrier = Ends {
    ends: VecDeque::from([limit(), TurnEnd::answered("job-7", "never asked")]),
    turns: vec![],
  };

  item_names(&scratch, "claude-code", "item-model").await;

  let (_, outcome, _, _) = carry_job(&ground, &job(), &mut carrier).await.unwrap().expect("an end");

  assert_eq!(
    carrier
      .turns
      .iter()
      .map(|(road, _, _)| (road.provider.as_str(), road.model.as_str()))
      .collect::<Vec<_>>(),
    vec![("claude-code", "item-model")],
    "the item's model alone, over the pin, and nothing after its road stopped answering"
  );
  assert_eq!(outcome.as_deref(), Some("failed"));
  assert_eq!(job_column(&scratch, "chosen_by").await.as_deref(), Some("item"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_whose_item_s_model_cannot_answer_is_held_with_the_reason_and_never_falls_back() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = standing(&scratch, home.path()).await;
  let mut carrier = Ends {
    ends: VecDeque::from([TurnEnd::answered("job-7", "never asked")]),
    turns: vec![],
  };

  item_names(&scratch, "deepseek", "deepseek-flash").await;
  deepseek_balance(&ground, 0.0).await;

  let waited =
    tokio::time::timeout(std::time::Duration::from_secs(3), carry_job(&ground, &job(), &mut carrier)).await;

  assert!(waited.is_err(), "the job waits for its item's model");
  assert!(carrier.turns.is_empty(), "no turn went out on another road: {:?}", carrier.turns);

  let held = job_column(&scratch, "held_for").await.unwrap_or_default();

  assert!(held.contains("balance is spent"), "{held}");
}

// A job whose road cannot answer as it starts waits held with the reason, its item's orderer told once, and
// goes out as soon as a quota move says its road answers again, not at the policy's clock.
#[tokio::test(flavor = "multi_thread")]
async fn a_job_whose_road_cannot_answer_is_told_once_and_goes_out_as_its_road_reopens() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let _ = standing(&scratch, home.path()).await;
  let (peers, vars) = crate::support::peers::stand().await;
  let ground = {
    let service = Service::new(scratch.db().clone(), Config::read(|_| None).unwrap());
    let mut env = vec![
      ("HOME".to_owned(), home.path().to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/nowhere".to_owned()),
    ];

    env.extend(vars);
    Ground::new(&service, &Machine::with_env(env, home.path().join("data")), None)
  };
  let mut carrier = Ends {
    ends: VecDeque::from([TurnEnd::answered("job-7", "done")]),
    turns: vec![],
  };

  peers
    .lock()
    .unwrap()
    .items
    .insert(70, crate::support::peers::item(70, "leased", serde_json::json!({ "scope": "-400" })));
  item_names(&scratch, "deepseek", "deepseek-flash").await;
  deepseek_balance(&ground, 0.0).await;

  let started = std::time::Instant::now();
  let reopen = async {
    while job_column(&scratch, "held_for").await.is_none() {
      tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    deepseek_balance(&ground, 5.0).await;
    ground.events.publish(agency_server::rest::DeploymentResting::of(
      agency_server::rest::RestKind::Limit,
      "login-a",
      0,
    ));
  };
  let carried_job = job();
  let (carried, ()) = tokio::join!(carry_job(&ground, &carried_job, &mut carrier), reopen);
  let (_, outcome, _, _) = carried.unwrap().expect("an end");

  assert_eq!(outcome, None, "the turn answered: {:?}", carrier.turns);
  assert_eq!(carrier.turns.len(), 1);
  assert!(
    started.elapsed() < std::time::Duration::from_secs(30),
    "woken by the move, not the clock"
  );
  assert_eq!(job_column(&scratch, "held_for").await, None);

  let told = peers.lock().unwrap().instructed();

  assert_eq!(told.len(), 1, "{told:?}");
  assert_eq!(told[0]["kind"], "job-waiting:70");
  assert!(told[0]["said"]["order"].as_str().unwrap().contains("balance is spent"), "{told:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_on_its_role_s_order_or_its_repository_s_pin_says_which_chose_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = standing(&scratch, home.path()).await;
  let answered = || Ends {
    ends: VecDeque::from([TurnEnd::answered("job-7", "Done.")]),
    turns: vec![],
  };

  let mut carrier = answered();

  carry_job(&ground, &job(), &mut carrier).await.unwrap().expect("an end");
  assert_eq!(job_column(&scratch, "chosen_by").await.as_deref(), Some("order"));

  scratch
    .run(
      r#"UPDATE "repositories" SET "pinned_provider" = 'claude-code', "pinned_model" = 'pinned-model'
         WHERE "name" = 'helper'"#,
      vec![],
    )
    .await;

  let mut carrier = answered();

  carry_job(&ground, &job(), &mut carrier).await.unwrap().expect("an end");
  assert_eq!(carrier.turns[0].0.model, "pinned-model");
  assert_eq!(job_column(&scratch, "chosen_by").await.as_deref(), Some("pin"));
}

// A job of a role whose switched-on agents all name no road goes out on the road of the supervisor that asks
// it, at that supervisor's effort, and the road it went out on is kept on the job
// (docs/provider-order.md, 「Runs on another's road」; docs/role-agents.md, 「Which agent
// a run takes」).
#[tokio::test(flavor = "multi_thread")]
async fn a_job_of_a_role_whose_agents_name_no_road_goes_out_on_the_road_of_the_supervisor_that_asks_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = standing(&scratch, home.path()).await;

  // The worker's agents are switched on and name no road.
  scratch
    .run(
      r#"UPDATE "role_agents" SET "provider" = NULL, "model" = NULL WHERE "role" = 'worker'"#,
      vec![],
    )
    .await;
  // The supervisor that asks stands on DeepSeek's flash model at the high effort.
  crate::support::rows::supervisor(
    scratch.db(),
    "asking",
    &[
      ("provider", "deepseek".into()),
      ("model", "deepseek-flash".into()),
      ("effort", "high".into()),
    ],
  )
  .await;
  scratch
    .run(r#"UPDATE "jobs" SET "holder_name" = 'asking' WHERE "id" = 7"#, vec![])
    .await;
  deepseek_balance(&ground, 5.0).await;

  let mut carrier = Ends {
    ends: VecDeque::from([TurnEnd::answered("job-7", "Done.")]),
    turns: vec![],
  };
  let carried = Carried {
    id: 7,
    worker: "helper",
    holder: "asking",
    prompt: "# An item\n\nDo the work.",
  };
  let (state, outcome, _, _) = carry_job(&ground, &carried, &mut carrier).await.unwrap().expect("an end");

  assert_eq!((state.as_str(), outcome), ("ended", None));

  let (taken, entry, _) = &carrier.turns[0];

  assert_eq!((taken.provider.as_str(), taken.model.as_str()), ("deepseek", "deepseek-flash"));
  assert_eq!(entry.effort.as_deref(), Some("high"), "the supervisor's effort");
  assert_eq!(job_column(&scratch, "chosen_by").await.as_deref(), Some("order"));

  let roads: String = store::one(scratch.db(), r#"SELECT "roads" FROM "jobs" WHERE "id" = 7"#, vec![])
    .await
    .unwrap()
    .unwrap()
    .try_get("", "roads")
    .unwrap();
  let roads: serde_json::Value = serde_json::from_str(&roads).unwrap();

  assert_eq!(roads[0]["provider"], "deepseek");
  assert_eq!(roads[0]["model"], "deepseek-flash");
}

#[test]
fn a_road_stops_answering_on_its_quota_window_balance_or_credential_and_never_on_the_work() {
  assert!(road_stopped(&limit()).is_some());
  assert!(road_stopped(&TurnEnd::failed("r", "API Error: 402 insufficient balance", None)).is_some());
  assert!(road_stopped(&TurnEnd::failed("r", "Please run /login", None)).is_some());
  assert!(
    road_stopped(&TurnEnd::failed(
      "r",
      "the engine refused",
      Some(EngineError {
        kind: Some("rate_limit".into())
      })
    ))
    .is_some()
  );
  assert!(road_stopped(&TurnEnd::failed("r", "the tests failed", None)).is_none());
  assert!(
    road_stopped(&TurnEnd::of("r", Outcome::Stopped)).is_none(),
    "a stop from outside is the stop"
  );
  assert!(road_stopped(&TurnEnd::answered("r", "claude usage limit reached")).is_none());
}

// A failed request the program reported is the provider's where the provider could not answer — a server
// error, its connection refused, as the model gateway refuses it while it restarts — and the work's own where
// the request was refused for its shape (docs/provider-order.md, 「A road that fails」).
#[test]
fn a_reported_api_failure_moves_the_road_on_a_server_error_and_never_on_the_request_s_shape() {
  let reported = |said: &str, kind: &str| {
    TurnEnd::failed(
      "r",
      said,
      Some(EngineError {
        kind: Some(kind.into()),
      }),
    )
  };
  let busy = "API Error: 503 litellm.ServiceUnavailableError: DeepseekException - Service is too busy.";
  let refused = "API Error: Connection refused — a firewall or proxy may be blocking it (ECONNREFUSED)";

  assert!(road_stopped(&reported(busy, "server_error")).is_some());
  assert!(road_stopped(&TurnEnd::failed("r", busy, None)).is_some(), "the status alone says it");
  assert!(road_stopped(&reported(refused, "server_error")).is_some());
  assert!(
    road_stopped(&TurnEnd::failed("r", refused, None)).is_some(),
    "the refusal alone says it"
  );
  assert!(road_stopped(&reported("API Error: 529 Overloaded", "overloaded")).is_some());

  assert!(
    road_stopped(&reported(
      "API Error: 400 messages: text content blocks must be non-empty",
      "invalid_request"
    ))
    .is_none()
  );
  assert!(road_stopped(&TurnEnd::failed("r", "API Error: 400 the request is malformed", None)).is_none());
  assert!(road_stopped(&reported("API Error: 413 request too large", "server_error")).is_none());
  assert!(road_stopped(&reported("the answer ran past its ceiling", "max_output_tokens")).is_none());
}

// A job on a registered model whose turn answers the bench-lock 503 holds rather than ending: the provider's
// sentence stands as its `heldFor`, its item's lease is renewed while it holds, and once the answer's block
// opens it goes out again on the same road and the same session, opening on the line that it continues its
// work, and answers (docs/cache-fallback.md, 「Who receives that failure」).
#[tokio::test(flavor = "multi_thread")]
async fn a_job_whose_registered_road_answers_503_holds_and_goes_out_again_on_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, vars) = crate::support::peers::stand().await;
  let ground = standing_naming(&scratch, home.path(), &vars).await;
  let busy = "API Error: 503 litellm.ServiceUnavailableError: DeepseekException - Service is too busy.";

  peers
    .lock()
    .unwrap()
    .items
    .insert(70, crate::support::peers::item(70, "leased", json!({ "scope": "-400" })));
  peers.lock().unwrap().leases.insert(1);
  registered(&scratch, &home.path().join("data"), "a-registration").await;
  item_names(&scratch, "registered", "a-registration").await;

  let mut carrier = Ends {
    ends: VecDeque::from([TurnEnd::failed("job-7", busy, None), TurnEnd::answered("job-7", "Done.")]),
    turns: vec![],
  };

  let carried_job = job();

  let (carried, held) = tokio::join!(carry_job(&ground, &carried_job, &mut carrier), async {
    while job_column(&scratch, "held_for").await.is_none() {
      tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let held = job_column(&scratch, "held_for").await.unwrap_or_default();

    // The item's lease is renewed while the job holds, as the pass does on its clock.
    let renewed = agency_server::job_leases::pass(scratch.db(), &vars).await.unwrap();

    assert_eq!(renewed, vec![(7, agency_server::job_leases::Kept::Renewed)]);
    assert!(
      peers
        .lock()
        .unwrap()
        .calls
        .iter()
        .any(|call| call.method.as_str() == "PATCH" && call.path == "/leases/1"),
      "the lease on the item stands through the wait"
    );

    // The provider answers again: the block its answer wrote is opened, and the road fact wakes the wait.
    for block in kept_blocks(scratch.db()).await.unwrap() {
      clear_block_by_id(scratch.db(), block.id).await.unwrap();
    }

    ground.events.publish(DeploymentResting::of(RestKind::Limit, "login-a", 0));
    held
  });

  let (_, outcome, report, _) = carried.unwrap().expect("an end");

  assert!(held.contains("registered"), "the wait names the provider's road: {held}");
  assert!(held.contains("503"), "the wait names the provider's sentence: {held}");
  assert_eq!(outcome, None, "the job went on and answered: {:?}", carrier.turns);
  assert_eq!(report.as_deref(), Some("Done."));
  assert_eq!(
    carrier
      .turns
      .iter()
      .map(|(road, _, _)| (road.provider.as_str(), road.model.as_str()))
      .collect::<Vec<_>>(),
    vec![("registered", "a-registration"), ("registered", "a-registration")],
    "the same road twice: the job never moved"
  );
  assert!(
    carrier.turns[1].2.contains("already ran on registered"),
    "the turn it goes on with continues the work in the checkout: {}",
    carrier.turns[1].2
  );
  assert!(carrier.turns[1].2.ends_with(carried_job.prompt));
  assert_eq!(job_column(&scratch, "held_for").await, None);
}

// The registration a job's item names, with an address, so its provider offers it.
async fn registered(scratch: &Scratch, data: &Path, name: &str) {
  use agency_server::control::registry::change;

  let save = json!({
    "action": "save", "name": name, "model": "m", "contextTokens": 32_768, "answerTokens": 4_096,
  });
  let address = json!({
    "action": "set-address", "name": name, "node": "alpha",
    "url": "http://[::1]:9/v1/chat/completions", "key": null,
  });

  change(scratch.db(), data, &save).await.unwrap().unwrap();
  change(scratch.db(), data, &address).await.unwrap().unwrap();
}

#[test]
fn a_job_launched_again_is_told_it_continues_the_work_in_the_checkout() {
  let from = Road {
    provider: "deepseek".into(),
    model: "deepseek-v4-pro".into(),
    account: None,
  };
  let prompt = continued(&from, "its balance is spent", "# An item");

  assert!(prompt.contains("deepseek · deepseek-v4-pro"), "{prompt}");
  assert!(prompt.contains("its balance is spent"), "{prompt}");
  assert!(prompt.contains("checkout"), "{prompt}");
  assert!(prompt.ends_with("# An item"), "{prompt}");
}

// A git command in a directory, with the identity a machine without one configured still commits under.
fn git(path: &Path, args: &[&str]) -> String {
  let done = Command::new("git")
    .arg("-C")
    .arg(path)
    .args(args)
    .env("GIT_AUTHOR_NAME", "tester")
    .env("GIT_AUTHOR_EMAIL", "tester@example.com")
    .env("GIT_COMMITTER_NAME", "tester")
    .env("GIT_COMMITTER_EMAIL", "tester@example.com")
    .output()
    .unwrap();

  assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));

  String::from_utf8_lossy(&done.stdout).trim().to_owned()
}

fn at(path: &Path) -> String {
  path.to_string_lossy().into_owned()
}

// A repository whose base checkout stands on main and whose one support worktree is not there yet, and a job
// in that support checkout: the first road that can answer is the pane program's.
async fn support_standing(scratch: &Scratch, home: &Path) -> (Ground, PathBuf, PathBuf, String) {
  let data = home.join("data");
  let env = data.join("env");

  std::fs::create_dir_all(&env).unwrap();
  for (name, key) in [("deepseek.key", "a key\n"), ("llm-gateway.key", "a key\n")] {
    let file = env.join(name);

    std::fs::write(&file, key).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
  }

  let members = data.join("llm-gateway");

  std::fs::create_dir_all(&members).unwrap();
  std::fs::write(
    members.join("members.json"),
    r#"[{"name":"deepseek-flash","road":"deepseek","provider":"deepseek","model":"deepseek-flash","api_base":"https://api.deepseek.com/v1","key_file":"deepseek.key","context_tokens":1048576,"answer_tokens":32768,"thinking":true}]"#,
  )
  .unwrap();

  let order: Vec<OrderEntry> = ["deepseek", "claude-code", "codex", "openrouter", "registered"]
    .iter()
    .map(|name| OrderEntry {
      name: (*name).to_owned(),
      enabled: true,
    })
    .collect();
  let ground = ground_on(scratch.db().clone(), home);

  store::seed::settle(scratch.db(), &deployment(&ground.machine())).await.unwrap();
  signed_in(&machine_config_dir(&ground.machine()));
  crate::support::rows::seated_on_order(scratch.db(), "worker").await;
  set_provider_order(scratch.db(), &order).await.unwrap();

  let base = home.join("web");
  let cwd = home.join("web-w2");
  let mut init = Command::new("git");

  init.args(["init", "-q", "-b", "main"]).arg(&base);

  let done = init.output().unwrap();

  assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));

  std::fs::write(base.join("README.md"), "# The base\n").unwrap();
  std::fs::write(
    base.join("package.json"),
    r#"{"name":"support-test","scripts":{"worktree:setup":"touch setup-ran"}}"#,
  )
  .unwrap();
  git(&base, &["add", "-A"]);
  git(&base, &["commit", "-q", "-m", "the base"]);

  let head = git(&base, &["rev-parse", "HEAD"]);

  repository(scratch.db(), "web", &at(&base), &[("support_count", 1i64.into())]).await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "token_count", "started_at", "prompt")
         VALUES (8, 80, 1, 'harness', 1, 'web-w2', 'running', 0, 0, '# An item')"#,
      vec![],
    )
    .await;

  (ground, base, cwd, head)
}

// A stand-in for the session: it takes the turn only where the worktree already stands at the base's head.
struct Standing {
  at: PathBuf,
  head: String,
  providers: Vec<String>,
  stood: bool,
}

impl Carrier for Standing {
  async fn carry(&mut self, taken: &Road, _entry: &Entry, _prompt: &str) -> Result<TurnEnd, String> {
    self.providers.push(taken.provider.clone());
    self.stood = self.at.join(".git").exists() && git(&self.at, &["rev-parse", "HEAD"]) == self.head;

    Ok(TurnEnd::answered("job-8", "Done."))
  }
}

// A first launch on a fresh support makes its worktree from the base's main, sets it up by the
// repository's own script, and only then stands the pane program the first road goes through
// (docs/worker-slots.md, 「Names and places」); a worktree gone is made again at the next launch.
#[tokio::test(flavor = "multi_thread")]
async fn a_first_launch_on_a_fresh_support_makes_its_worktree_before_the_pane_road_takes_the_turn() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (ground, base, cwd, head) = support_standing(&scratch, home.path()).await;
  let job = || Carried {
    id: 8,
    worker: "web-w2",
    holder: "harness",
    prompt: "# An item\n\nDo the work.",
  };
  let mut carrier = Standing {
    at: cwd.clone(),
    head: head.clone(),
    providers: vec![],
    stood: false,
  };
  let (state, outcome, _, _) = carry_job(&ground, &job(), &mut carrier).await.unwrap().expect("an end");

  assert_eq!((state.as_str(), outcome), ("ended", None));
  assert_eq!(carrier.providers, vec!["deepseek"], "the first road is the pane program's");
  assert!(carrier.stood, "the worktree stood before the pane road's turn was taken");
  assert!(cwd.join(".git").exists());
  assert!(cwd.join("setup-ran").exists(), "the repository's own setup ran in it");
  assert_eq!(git(&cwd, &["rev-parse", "HEAD"]), head, "detached at the base's main");

  // The worktree gone, the next launch makes it again by the same cut.
  git(&base, &["worktree", "remove", "--force", &at(&cwd)]);
  assert!(!cwd.exists());

  let mut again = Standing {
    at: cwd.clone(),
    head: head.clone(),
    providers: vec![],
    stood: false,
  };

  carry_job(&ground, &job(), &mut again).await.unwrap().expect("an end");

  assert!(again.stood, "a missing worktree is made again at the next launch");
  assert!(cwd.join(".git").exists());
  assert_eq!(git(&cwd, &["rev-parse", "HEAD"]), head);
}

// A checkout keeping a session with runs behind it gets its launch's turn: the carrier the launch hands it
// writes `start` rather than the launch sitting silently in a program already carrying another turn
// (docs/agent-control.md, 「A worker's pane lock」).
struct Starts {
  lines: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl Carrier for Starts {
  async fn carry(&mut self, _taken: &Road, _entry: &Entry, _prompt: &str) -> Result<TurnEnd, String> {
    self.lines.lock().unwrap().push("start".to_owned());

    Ok(TurnEnd::answered("job-7", "Done."))
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launch_on_a_worker_with_runs_writes_start() {
  use std::sync::{Arc, Mutex};
  use std::time::{Duration, Instant};

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = standing(&scratch, home.path()).await;

  agency_server::worker_session::records::record(
    scratch.db(),
    "helper",
    "claude-opus-5",
    "",
    "kept",
    None,
    None,
  )
  .await
  .unwrap();

  let started = Arc::new(Mutex::new(Vec::new()));
  let mut carrier = Starts {
    lines: started.clone(),
  };

  carry_job(&ground, &job(), &mut carrier).await.unwrap().expect("an end");

  let began = Instant::now();

  while started.lock().unwrap().is_empty() && began.elapsed() < Duration::from_secs(60) {
    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  assert_eq!(&*started.lock().unwrap(), &["start".to_owned()]);
}
