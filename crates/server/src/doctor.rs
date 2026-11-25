//! The doctor (docs/run-records.md, 「Why a run died」): why a run died, and what is published when one
//! answers again. What the diagnosis says is bounded by what was read where the run ended — how it
//! finished, what it was last seen doing and what the program said back — and a cause nobody could read is
//! said to be unknown. The same place remembers the failure, so the next run that answers is known to be a
//! recovery.

use policy::chat_health::CHAT_HEALTH;
use policy::deployment::DEPLOYMENT;
use policy::fill_marks;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use store::one;

use crate::Refusal;
use crate::events::Event;
use crate::queue::settings;

// How far past a line anything read off a run is cut.
const SAID_LIMIT: usize = 120;
const DETAIL_LIMIT: usize = 60;

// The failure standing now, kept in the store so a server that went down over its own failure still knows.
const STANDING_KEY: &str = "doctor_standing_failure";

/// How a run ended, as whoever watched it could read it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RunEnding {
  /// The process ended by itself and said it had not worked.
  Exit { code: i64 },
  /// The process ran to its end and handed back an error in place of an answer.
  ErrorResult,
  /// The run held together and produced no answer where one was owed.
  Silent,
  /// The run was never had: the agent's pane or host could not be stood, or went away under it.
  Transport,
  /// Nothing about the ending was readable.
  Unknown,
}

/// What the run was last seen doing, as its own log kept it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastAction {
  pub tool: String,
  pub detail: Option<String>,
}

/// What a dead run left behind: the room whose turn it was (none for the owner's own chat), how it ended,
/// what the program said back, what it was last doing and where its whole account is.
#[derive(Clone, Debug)]
pub struct Evidence {
  pub room: Option<String>,
  pub ending: RunEnding,
  pub said: Option<String>,
  pub last_action: Option<LastAction>,
  pub record: String,
}

/// Why the run died, whether something actually broke, and what tells this cause from another.
#[derive(Clone, Debug)]
pub struct Diagnosis {
  pub broke: bool,
  pub cause: String,
  pub signature: String,
}

/// One line of what a run said back, with nothing of its shape left to spread over a message.
pub fn one_line(text: &str, limit: usize) -> Option<String> {
  let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");

  if flat.is_empty() {
    return None;
  }

  if flat.chars().count() <= limit {
    Some(flat)
  } else {
    Some(format!("{}…", flat.chars().take(limit).collect::<String>()))
  }
}

// What a shell reports when something outside the process ended it: 128 plus the signal's number.
fn signal_of(code: i64) -> Option<i64> {
  (code > 128 && code < 160).then_some(code - 128)
}

/// Why the run died, in the plain words the owner reads, out of what was actually read.
pub fn diagnose(evidence: &Evidence) -> Diagnosis {
  let causes = &CHAT_HEALTH.causes;
  let (broke, cause, signature) = match evidence.ending {
    RunEnding::Exit { code } => match signal_of(code) {
      None => (true, fill_marks(&causes.exit, &[("code", &code.to_string())]), format!("exit:{code}")),
      Some(signal) => (
        true,
        fill_marks(&causes.signal, &[("signal", &signal.to_string())]),
        format!("signal:{code}"),
      ),
    },
    RunEnding::ErrorResult => (true, causes.error_result.clone(), "error-result".to_owned()),
    RunEnding::Silent => (false, causes.silent.clone(), "silent".to_owned()),
    RunEnding::Transport => (true, causes.transport.clone(), "transport".to_owned()),
    RunEnding::Unknown => (true, causes.unknown.clone(), "unknown".to_owned()),
  };

  Diagnosis {
    broke,
    cause,
    signature,
  }
}

/// What a run was last doing, as the owner reads it.
pub fn action_line(action: &LastAction) -> String {
  let words = &CHAT_HEALTH.account;
  let detail = action
    .detail
    .as_ref()
    .map_or_else(String::new, |detail| fill_marks(&words.detail, &[("detail", detail)]));

  fill_marks(&words.action, &[("tool", &action.tool), ("detail", &detail)])
}

/// The account of the dead run, in two lines: what broke and why, then what it was doing and where the rest
/// is.
pub fn doctor_line(evidence: &Evidence, diagnosis: &Diagnosis) -> String {
  let words = &CHAT_HEALTH.account;
  let opening = if diagnosis.broke {
    fill_marks(&words.broke, &[("agent", &DEPLOYMENT.agent_name)])
  } else {
    fill_marks(&words.silent, &[("agent", &DEPLOYMENT.agent_name)])
  };
  let room = evidence
    .room
    .as_ref()
    .map_or_else(String::new, |room| fill_marks(&words.room, &[("room", room)]));
  let said = evidence
    .said
    .as_deref()
    .and_then(|said| one_line(said, SAID_LIMIT))
    .map_or_else(String::new, |said| fill_marks(&words.said, &[("said", &said)]));
  let rest = evidence
    .last_action
    .as_ref()
    .map(action_line)
    .into_iter()
    .chain(std::iter::once(fill_marks(&words.record, &[("record", &evidence.record)])))
    .collect::<Vec<_>>()
    .join(" ");

  fill_marks(
    &words.line,
    &[
      ("opening", &opening),
      ("room", &room),
      ("cause", &diagnosis.cause),
      ("said", &said),
      ("rest", &rest),
    ],
  )
}

// What a tool call was given, short enough to read in the same breath as the tool's name: the command it
// ran or the file it touched.
fn action_detail(text: &str) -> Option<String> {
  let input = serde_json::from_str::<Value>(text).ok()?;

  ["command", "file_path", "path"]
    .iter()
    .find_map(|field| input.get(*field).and_then(Value::as_str).filter(|value| !value.is_empty()))
    .and_then(|value| one_line(value, DETAIL_LIMIT))
}

/// What the run was last seen doing, read off its own log: the last tool call the agent's run made.
pub async fn last_action_of(
  db: &DatabaseConnection,
  run: &str,
  agent: &str,
) -> Result<Option<LastAction>, Refusal> {
  let row = one(
    db,
    r#"SELECT "name", "text" FROM "agent_log" WHERE "agent" = $1 AND "run" = $2 AND "kind" = 'tool' ORDER BY "id" DESC LIMIT 1"#,
    vec![agent.into(), run.into()],
  )
  .await?;
  let Some(row) = row else {
    return Ok(None);
  };
  let text: String = row.try_get("", "text")?;

  Ok(Some(LastAction {
    tool: row
      .try_get::<Option<String>>("", "name")?
      .unwrap_or_else(|| CHAT_HEALTH.account.unnamed_tool.clone()),
    detail: action_detail(&text),
  }))
}

/// Where the run's own account is, in the words the owner would use to go and open it: the run's log, or
/// for a failure no run was had for, the journal of the unit the caller names.
pub fn record_of(run: Option<&str>, unit: &str) -> String {
  match run {
    None => format!("journalctl --user -u {unit}"),
    Some(run) => {
      fill_marks(&CHAT_HEALTH.account.run_log, &[("agent", &DEPLOYMENT.agent_name), ("run", run)])
    }
  }
}

/// A failure that has been told and not yet answered by a run that came back.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StandingFailure {
  pub signature: String,
  pub cause: String,
  pub room: Option<String>,
  pub count: i64,
  pub at: i64,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub told: Option<bool>,
}

/// The failure standing now, or none.
pub async fn standing_failure(db: &DatabaseConnection) -> Result<Option<StandingFailure>, Refusal> {
  Ok(
    settings::get(db, STANDING_KEY)
      .await?
      .and_then(|stored| serde_json::from_str(&stored).ok()),
  )
}

/// Writes this failure down, counting how many times in a row the same cause has been it.
pub async fn note_failure(
  db: &DatabaseConnection,
  diagnosis: &Diagnosis,
  room: Option<&str>,
  now_ms: i64,
) -> Result<StandingFailure, Refusal> {
  let standing = standing_failure(db).await?;
  let failure = StandingFailure {
    signature: diagnosis.signature.clone(),
    cause: diagnosis.cause.clone(),
    room: room.map(str::to_owned),
    count: match &standing {
      Some(standing) if standing.signature == diagnosis.signature => standing.count + 1,
      _ => 1,
    },
    at: now_ms,
    // Heard of once is heard of until the agent answers again, whichever cause the latest failure had.
    told: Some(standing.and_then(|standing| standing.told).unwrap_or(false)),
  };

  settings::set(db, STANDING_KEY, &serde_json::to_string(&failure).unwrap_or_default()).await?;

  Ok(failure)
}

/// The owner has heard of the standing failure from the consumer's own line, so its end is the owner's to
/// hear too.
pub async fn mark_failure_told(db: &DatabaseConnection) -> Result<(), Refusal> {
  if let Some(standing) = standing_failure(db).await? {
    let told = StandingFailure {
      told: Some(true),
      ..standing
    };

    settings::set(db, STANDING_KEY, &serde_json::to_string(&told).unwrap_or_default()).await?;
  }

  Ok(())
}

pub async fn clear_failure(db: &DatabaseConnection) -> Result<(), Refusal> {
  settings::forget(db, STANDING_KEY).await
}

/// Where a turn was, as a line names it: the owner's own chat, or another room by its title.
pub fn place_of(room: Option<&str>) -> String {
  let places = &CHAT_HEALTH.places;

  room.map_or_else(|| places.owner_chat.clone(), |room| fill_marks(&places.room, &[("room", room)]))
}

/// A run that has come back after a failure: the facts of it, what saying it is for, the kind of notice
/// it is and how long it stays worth saying. Agency publishes it and voices nothing; the consumer that
/// owns the room says it in its own manner.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecovered {
  pub fact: String,
  pub intent: String,
  pub kind: String,
  pub ttl_ms: i64,
}

impl Event for RunRecovered {
  const NAME: &'static str = "run-recovered";
}

/// The recovery published for a run that has come back from the standing failure.
pub fn recovery_notice(standing: &StandingFailure) -> RunRecovered {
  let said = &CHAT_HEALTH.recovered;
  let sender = &DEPLOYMENT.sender_name;
  let times = if standing.count > 1 {
    fill_marks(&said.repeated, &[("count", &standing.count.to_string())])
  } else {
    String::new()
  };

  RunRecovered {
    fact: [
      fill_marks(
        &said.failed,
        &[("sender", sender), ("where", &place_of(standing.room.as_deref())), ("times", &times)],
      ),
      fill_marks(&said.cause, &[("cause", &standing.cause)]),
      fill_marks(&said.answering, &[("sender", sender)]),
    ]
    .join("\n"),
    intent: said.intent.clone(),
    kind: said.kind.clone(),
    ttl_ms: said.ttl_minutes * 60_000,
  }
}
