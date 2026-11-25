//! The run log's writer: a line cut at a character and saying so, a job's lines numbered in its own order
//! and kept whole, whichever of the job's writers files them, a credential masked on the way in, and the log
//! held to its bounds.

use agency_server::run_log::lines::Line;
use agency_server::run_log::prune::{
  LOG_BYTES_PER_AGENT, LOG_LINES_PER_AGENT, LOG_MOST_AGE_MS, agent_run_log, run_log,
};
use agency_server::run_log::secrets::MASKED_VALUE;
use agency_server::run_log::{MAX_TOOL_CHARS, RunLog, Target, preview};

use crate::support::scratch::Scratch;

fn target(agent: &str, run: &str, job_id: Option<i64>) -> Target {
  Target {
    agent: agent.into(),
    run: run.into(),
    chat_id: Some(-7),
    job_id,
  }
}

#[test]
fn a_preview_cuts_at_a_character_and_says_so() {
  assert_eq!(preview("éèabc", Some(2)), ("éè".to_owned(), true));
  assert_eq!(preview("abc", Some(3)), ("abc".to_owned(), false));
  assert_eq!(preview("abc", None), ("abc".to_owned(), false));
}

#[test]
fn the_bounds_are_the_sweeps() {
  assert_eq!(
    (LOG_LINES_PER_AGENT, LOG_BYTES_PER_AGENT, LOG_MOST_AGE_MS),
    (4000, 64 * 1024 * 1024, 604_800_000)
  );
}

#[tokio::test]
async fn a_jobs_lines_are_numbered_kept_whole_and_masked() {
  let scratch = Scratch::new().await;
  let mut log = RunLog::new(target("worker", "job-run-1", Some(41)));
  let long = "x".repeat(MAX_TOOL_CHARS + 10);

  log
    .write(
      scratch.db(),
      Line {
        kind: "tool",
        text: format!("Authorization: Bearer abcdefgh1234 {long}"),
        ..Line::default()
      },
    )
    .await
    .unwrap();
  log
    .write(
      scratch.db(),
      Line {
        kind: "text",
        text: "done".into(),
        ..Line::default()
      },
    )
    .await
    .unwrap();

  let rows = store::all(
    scratch.db(),
    r#"SELECT "sequence", "text", "detail", "truncated", "source" FROM "agent_log" WHERE "job_id" = 41 ORDER BY "id""#,
    vec![],
  )
  .await
  .unwrap();
  let sequence = |at: usize| rows[at].try_get::<i64>("", "sequence").unwrap();
  let detail: String = rows[0].try_get("", "detail").unwrap();

  assert_eq!((sequence(0), sequence(1)), (1, 2));
  assert_eq!(rows[0].try_get::<i64>("", "truncated").unwrap(), 1);
  assert!(detail.starts_with(&format!("Authorization: Bearer {MASKED_VALUE}")), "{detail}");
  assert!(detail.ends_with(&long), "a job's line keeps its whole text as its detail");
  assert!(!detail.contains("abcdefgh1234"));
}

#[tokio::test]
async fn the_log_is_held_to_its_age_but_an_agents_newest_system_prompt_and_a_jobs_lines() {
  let scratch = Scratch::new().await;
  let now = 2_000_000_000_000;
  let old = now - LOG_MOST_AGE_MS - 60_000;
  let insert = |agent: &'static str, kind: &'static str, name: Option<&'static str>, job: Option<i64>| {
    let db = scratch.db().clone();

    async move {
      store::run(
        &db,
        r#"INSERT INTO "agent_log" ("agent", "run", "at", "kind", "name", "text", "job_id", "truncated", "upstream_truncated",
             "detail_complete", "detail_length", "metadata_length")
           VALUES ($1, 'r', $2, $3, $4, 'line', $5, 0, 0, 1, 4, 0)"#,
        vec![agent.into(), old.into(), kind.into(), name.into(), job.into()],
      )
      .await
      .unwrap();
    }
  };

  insert("pal", "prompt", Some("system"), None).await;
  insert("pal", "text", None, None).await;
  insert("pal", "end", None, None).await;
  insert("pal", "text", None, Some(9)).await;
  insert("other", "text", None, None).await;

  // A writer's prune takes its own agent's lines past the age, but its newest system prompt and a job's
  // lines; another agent's old lines are the clock's sweep.
  assert_eq!(agent_run_log(scratch.db(), "pal", now).await.unwrap(), 2);

  let left = |agent: &'static str| {
    scratch.count(r#"SELECT COUNT(*) FROM "agent_log" WHERE "agent" = $1"#, vec![agent.into()])
  };

  assert_eq!(left("pal").await, 2, "its newest system prompt and the job's line stay");
  assert_eq!(left("other").await, 1, "another agent's old line waits for the sweep");

  // The sweep over the whole table keeps every agent's newest system prompt.
  insert("other", "prompt", Some("system"), None).await;
  insert("other", "text", None, None).await;

  assert_eq!(run_log(scratch.db(), now).await.unwrap(), 2);
  assert_eq!((left("pal").await, left("other").await), (2, 1));
}

#[tokio::test]
async fn a_pass_takes_the_old_lines_in_batches_and_finishes() {
  let scratch = Scratch::new().await;
  let now = 2_000_000_000_000;
  let old = now - LOG_MOST_AGE_MS - 60_000;

  scratch
    .run(
      r#"INSERT INTO "agent_log" ("agent", "run", "at", "kind", "text", "job_id")
         SELECT 'pal', 'r', $1, 'text', 'line', NULL FROM generate_series(1, 1200)"#,
      vec![old.into()],
    )
    .await;

  assert_eq!(run_log(scratch.db(), now).await.unwrap(), 1200);
  assert_eq!(
    scratch.count(r#"SELECT COUNT(*) FROM "agent_log" WHERE "agent" = 'pal'"#, vec![]).await,
    0
  );
}

#[tokio::test]
async fn a_job_s_line_numbered_past_another_writer_s_is_filed_once_more_and_never_lost() {
  let scratch = Scratch::new().await;
  let line = |text: &str| Line {
    kind: "text",
    text: text.into(),
    ..Line::default()
  };

  // The turn's own log has written once, so it holds where it thinks the job's lines left off.
  let mut turn = RunLog::new(target("worker", "job-42", Some(42)));

  turn.write(scratch.db(), line("the turn's first line")).await.unwrap();

  // A word said to the job meanwhile files its lines through a writer of its own.
  let mut word = RunLog::new(target("worker", "job-42", Some(42)));

  word.write(scratch.db(), line("a word said to the job")).await.unwrap();

  // The turn's next line is filed all the same, after the word's.
  assert!(turn.write(scratch.db(), line("the turn's second line")).await.is_some());

  let rows = store::all(
    scratch.db(),
    r#"SELECT "text" FROM "agent_log" WHERE "job_id" = 42 ORDER BY "sequence""#,
    vec![],
  )
  .await
  .unwrap();
  let texts: Vec<String> = rows.iter().map(|row| row.try_get("", "text").unwrap()).collect();

  assert_eq!(texts, ["the turn's first line", "a word said to the job", "the turn's second line"]);
}
