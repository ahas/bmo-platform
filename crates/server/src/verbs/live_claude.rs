//! A job opening the live-Claude guard for itself (docs/live-claude-guard.md): `--purpose` opens it for
//! this job, `--list` says what this job opened and what each opening let through.
//!
//! Nobody is asked. The job is the one this process's run name belongs to, so the verb is run from the
//! job's own shell; outside a job there is nothing to open. Every run the guard lets through afterwards is
//! kept against the opening, and closing the item puts the count into its report.

use sea_orm::DatabaseConnection;

use super::{VerbError, turn_variable};
use store::{all, one};

/// The verb's two forms.
pub const USAGE: &str = "usage: bmo-agency live-claude --purpose '<what it measures and why>' | --list";

// The job runner's own marker, set by the runner once it holds its checkout's lock.
const JOB_RUNNER_MARKER: &str = "BMO_TASK_LOCKED";

/// Whether this process is a job's: a process the job's own CLI started carries the Claude Code session
/// marker and the job runner's marker; a room's session carries the first alone, and the bot's services
/// neither.
pub fn inside_job() -> bool {
  std::env::var("CLAUDECODE").is_ok_and(|mark| !mark.is_empty())
    && turn_variable(JOB_RUNNER_MARKER).is_some()
}

/// The run this process works for, where it is a job's.
pub fn job_run() -> Option<String> {
  inside_job().then(|| turn_variable("BMO_RUN_NAME")).flatten()
}

fn ended(said: String) -> VerbError {
  VerbError::Ended { code: 1, said }
}

/// What the job running as `run` opened, oldest first, each with the runs it let through.
pub async fn list(db: &DatabaseConnection, run: &str) -> Result<Vec<String>, VerbError> {
  let openings = all(
    db,
    r#"SELECT "id", "purpose" FROM "live_claude_openings" WHERE "run" = $1 ORDER BY "id""#,
    vec![run.into()],
  )
  .await?;

  if openings.is_empty() {
    return Ok(vec!["live-claude: this job has opened nothing".into()]);
  }

  let mut lines = vec![];

  for opening in openings {
    let id: i64 = opening.try_get("", "id")?;
    let purpose: String = opening.try_get("", "purpose")?;
    let runs = all(
      db,
      r#"SELECT "what", "ran_at" FROM "live_claude_runs" WHERE "opening_id" = $1 ORDER BY "id""#,
      vec![id.into()],
    )
    .await?;

    lines.push(format!(
      "#{id} {purpose} — {} run{}",
      runs.len(),
      if runs.len() == 1 { "" } else { "s" }
    ));

    for ran in runs {
      let at = chrono::DateTime::from_timestamp_millis(ran.try_get::<i64>("", "ran_at")?)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

      lines.push(format!("  {at} {}", ran.try_get::<String>("", "what")?));
    }
  }

  Ok(lines)
}

/// The guard opened for the job running as `run`, for the purpose it says, kept under the run's name.
pub async fn open(
  db: &DatabaseConnection,
  run: &str,
  purpose: &str,
  now_ms: i64,
) -> Result<Vec<String>, VerbError> {
  let purpose = purpose.trim();

  if purpose.is_empty() {
    return Err(ended(format!("live-claude: say what the runs are for with --purpose.\n{USAGE}")));
  }

  let opened = one(
    db,
    r#"INSERT INTO "live_claude_openings" ("run", "purpose", "opened_at") VALUES ($1, $2, $3) RETURNING "id""#,
    vec![run.into(), purpose.into(), now_ms.into()],
  )
  .await?
  .map(|row| row.try_get::<i64>("", "id"))
  .transpose()?
  .unwrap_or_default();

  Ok(vec![format!(
    "live-claude: opened #{opened} for this job — {purpose}. Every run it lets through is recorded; name them in the report."
  )])
}

/// The refusal outside a job.
pub fn not_a_job() -> VerbError {
  ended("live-claude: this process is not a job's, so there is no job to open the guard for.".into())
}
