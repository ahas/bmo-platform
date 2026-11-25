//! What a killed agency process left in flight, read by the next process started on the same store and data
//! root (docs/sessions.md, 「A restart hands the session over」; 「Session replacement」). A server is started and
//! killed, and what such a process leaves is put in place as it would have left it: each turn kept under its run,
//! the execution its run opened in a pane of its own, the turn's liveness row naming a program that still
//! stands, the session's transcript, the job a turn carried, and a room's handover owed. The next server reads
//! them as it starts, before its routes stand:
//! - a turn whose program never took its words is lost: a research run's is ended failed and its execution
//!   closed, and a job's is let go and its job launched again from the words it keeps, never reaped;
//! - a turn no job holds that its program took and is still working on is attached to, and cut at the bound of
//!   an adopted turn, since its caller went away with the killed process;
//! - a room's handover owed on a session the room no longer holds is read back and let go.

use std::time::Duration;

use harness_providers::ARCHIVE_PROVIDERS;
use serde_json::json;

use agency_server::control::work::{REAPED, RESUMED};
use agency_server::now_ms;
use agency_server::supervisor_turn::replacement::{RoomOwed, owe_room, room_owed};
use agency_server::turn_pickup::{InFlight, keep, kept};

use crate::support::grounds::{OrderEntry, set_provider_order};
use crate::support::scratch::Scratch;
use crate::support::standing;

const TOKEN: &str = "secret";

// A program standing for the one a pane of its own carried a turn in, taken down however the test ends.
struct Program(std::process::Child);

impl Program {
  fn stand() -> Self {
    Self(
      std::process::Command::new("sleep")
        .arg("600")
        .stdin(std::process::Stdio::null())
        .spawn()
        .unwrap(),
    )
  }

  fn alive(&mut self) -> bool {
    self.0.try_wait().unwrap().is_none()
  }
}

impl Drop for Program {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

// A moment as a Claude Code transcript stamps its lines.
fn stamp(at: i64) -> String {
  chrono::DateTime::from_timestamp_millis(at).unwrap().to_rfc3339()
}

// A turn the killed process kept in flight in a pane of its own: the execution its run opened at `began`, the
// liveness row naming `program` and the transcript, and the turn kept under its run since `kept_since`.
async fn left_in_flight(
  scratch: &Scratch,
  program: &Program,
  transcript: &std::path::Path,
  (run, agent, job): (&str, &str, Option<i64>),
  began: i64,
  kept_since: i64,
) {
  let identity = agent_host::identity::ProcessIdentity::of(program.0.id() as i32).unwrap();
  let execution = format!("own-{run}");

  scratch
    .run(
      r#"INSERT INTO "agent_executions" ("id", "body") VALUES ($1, $2)"#,
      vec![
        execution.clone().into(),
        json!({ "id": execution, "agent": null, "line": "worker", "provider": "claude-code",
                "parent": run, "state": "running", "startedAt": began, "endedAt": null })
        .to_string()
        .into(),
      ],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "turn_liveness" ("execution", "run", "line", "session", "phase", "program_pid",
           "program_birth", "records", "opened_at", "written_at")
         VALUES ($1, $2, 'worker', 's', 'working', $3, $4, $5, $6, $6)"#,
      vec![
        execution.into(),
        run.into(),
        i64::from(identity.pid).into(),
        identity.birth.clone().into(),
        json!([transcript]).to_string().into(),
        began.into(),
      ],
    )
    .await;
  keep(
    scratch.db(),
    &InFlight {
      session: format!("{agent}@worker"),
      turn: 1,
      run: run.to_owned(),
      key: None,
      agent: agent.to_owned(),
      chat_id: Some(7),
      ledger: None,
      job,
      started_at: kept_since,
    },
  )
  .await;
}

// The text of the end line a run's log closes with, once it has one.
async fn end_logged(scratch: &Scratch, run: &str) -> Option<String> {
  store::one(
    scratch.db(),
    r#"SELECT "text" FROM "agent_log" WHERE "run" = $1 AND "kind" = 'end' ORDER BY "id" DESC LIMIT 1"#,
    vec![run.into()],
  )
  .await
  .unwrap()
  .and_then(|row| row.try_get::<String>("", "text").ok())
}

// Whether every execution a run opened is closed.
async fn executions_closed(scratch: &Scratch, run: &str) -> bool {
  scratch
    .count(
      r#"SELECT COUNT(*) FROM "agent_executions"
         WHERE ("body"::jsonb)->>'parent' = $1 AND ("body"::jsonb)->>'endedAt' IS NULL"#,
      vec![run.into()],
    )
    .await
    == 0
}

async fn job_report(scratch: &Scratch, id: i64) -> (String, Option<String>) {
  let row =
    store::one(scratch.db(), r#"SELECT "state", "report" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
      .await
      .unwrap()
      .unwrap();

  (row.try_get("", "state").unwrap(), row.try_get("", "report").unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_killed_process_left_in_flight_is_ended_cut_launched_again_or_let_go_by_the_next_one() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");

  std::fs::create_dir_all(&data).unwrap();
  // Claude Code alone is on the order and has no login, so a job launched again stands nothing on the machine.
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  set_provider_order(
    scratch.db(),
    &ARCHIVE_PROVIDERS
      .iter()
      .map(|name| OrderEntry {
        name: (*name).to_owned(),
        enabled: *name == "claude-code",
      })
      .collect::<Vec<_>>(),
  )
  .await
  .unwrap();
  crate::support::rows::repository(scratch.db(), "worker-a", "/checkouts/worker-a", &[]).await;

  let first = standing::stand_server(home.path(), &data, &scratch.address(), TOKEN);

  drop(first);

  // What the killed process left. Its programs stand on: the restart of agency is no event of theirs.
  let mut program = Program::stand();
  let now = now_ms();
  let began = now - 20 * 60 * 1000;
  let bound = policy::resident_turn::RESIDENT_TURN.adopted_bound_minutes as i64 * 60_000;
  let long_ago = now - bound - 60_000;

  // A research run's turn and a job's whose program never took their words: nothing in their transcripts.
  left_in_flight(
    &scratch,
    &program,
    &home.path().join("research-1.jsonl"),
    ("research-1", "voyager", None),
    began,
    began,
  )
  .await;
  left_in_flight(
    &scratch,
    &program,
    &home.path().join("job-7192.jsonl"),
    ("job-7192", "worker-a", Some(7192)),
    began,
    began,
  )
  .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
           "run_name", "room_id", "state", "started_at", "prompt", "turn_words")
         VALUES (7192, 1, 0, 'harness', 0, 'worker-a', 'job-7192', 7, 'running', $1, 'apart', 'the work')"#,
      vec![began.into()],
    )
    .await;

  // A research run's turn its program took and still works on, begun past the bound of an adopted turn.
  let working = home.path().join("research-2.jsonl");

  std::fs::write(
    &working,
    format!(
      "{}\n",
      json!({ "type": "user", "timestamp": stamp(long_ago + 10), "message": { "content": "the search" } })
    ),
  )
  .unwrap();
  left_in_flight(&scratch, &program, &working, ("research-2", "voyager", None), long_ago, long_ago).await;

  // A room's handover owed on a session the room no longer holds.
  owe_room(
    scratch.db(),
    "chat-agent",
    5,
    &RoomOwed {
      supervisor: "chat-agent".into(),
      label: Some("the chat agent".into()),
      pinned_model: None,
      provider: "claude-code".into(),
      model: "claude-sonnet-5".into(),
      system: "the prompt last sent".into(),
      generation: indexmap::IndexMap::new(),
      session: "s-gone".into(),
      placement: None,
      why: agency_server::supervisor_turn::handoff::Why::Documents,
    },
  )
  .await
  .unwrap();

  let second = standing::stand_server(home.path(), &data, &scratch.address(), TOKEN);

  // Before its routes stood, the next process ended the research run whose program never took its words, and
  // closed what its run opened.
  assert!(!kept(scratch.db(), "research-1").await);
  assert!(
    end_logged(&scratch, "research-1")
      .await
      .unwrap_or_default()
      .contains("no record its program keeps names it"),
    "{:?}",
    end_logged(&scratch, "research-1").await
  );
  assert!(executions_closed(&scratch, "research-1").await);

  // The job's turn no program took was let go and the job launched again from the words it keeps, so the pass
  // that ends a job left running, which ran after, did not reap it.
  assert!(!kept(scratch.db(), "job-7192").await);
  assert!(executions_closed(&scratch, "job-7192").await);
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "agent_log" WHERE "job_id" = 7192 AND "text" = $1"#,
        vec![RESUMED.into()],
      )
      .await,
    1
  );

  let (state, report) = job_report(&scratch, 7192).await;

  assert!(!report.unwrap_or_default().contains(REAPED), "the job was not reaped: {state}");

  // The turn still working is attached to and cut at the bound of an adopted turn: nobody else would end it.
  let mut cut = None;

  for _ in 0..200 {
    cut = end_logged(&scratch, "research-2").await;

    if cut.is_some() && !kept(scratch.db(), "research-2").await {
      break;
    }

    tokio::time::sleep(Duration::from_millis(25)).await;
  }

  let cut = cut.expect("the adopted turn was ended");

  assert!(cut.contains(&format!("{}-minute bound", bound / 60_000)), "{cut}");
  assert!(!kept(scratch.db(), "research-2").await);
  assert!(program.alive(), "the turn's program was never this process's to end");

  // The room's handover owed is read back by the next process's settle pass, which a reload of documents runs at
  // once, and let go, since the room no longer holds the session it was owed on.
  let answer = reqwest::Client::new()
    .post(format!("http://127.0.0.1:{}/document-moves", second.port))
    .bearer_auth(TOKEN)
    .json(&json!({ "paths": ["notes.md"] }))
    .send()
    .await
    .unwrap();

  assert_eq!(answer.status().as_u16(), 201);

  for _ in 0..200 {
    if room_owed(scratch.db(), "chat-agent", 5).await.unwrap().is_none() {
      break;
    }

    tokio::time::sleep(Duration::from_millis(25)).await;
  }

  assert_eq!(room_owed(scratch.db(), "chat-agent", 5).await.unwrap(), None);
}
