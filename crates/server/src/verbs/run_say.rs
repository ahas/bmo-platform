//! `bmo-agency run say`: words said to a run or to a repository's worker (docs/agent-control.md, 「Words
//! said to a worker」). A run is named by its session, `session:<id>`; a job by its id, `job:<id>`; a worker by its repository, `repository:<name>`, or by its
//! checkout, `worker:<name>`. The words are offered to the turn going now, kept for a job's next turn, or
//! carried in a turn of the worker's own, and what became of them is printed.

use std::path::PathBuf;

use base64::Engine as _;
use clap::Parser;
use serde_json::{Value, json};

use super::{Answer, CONTROL_TOKEN, VerbError, control_call, control_token};

/// What `bmo-agency run say` is asked.
#[derive(Parser, Debug, Default)]
pub struct SayArgs {
  /// `<kind>:<id>`: `session:<sessionId>`, `job:<jobId>`, `repository:<name>` or `worker:<checkout>`.
  pub target: String,
  /// The words to say.
  #[arg(long, allow_hyphen_values = true)]
  pub words: String,
  /// A file said with the words, repeated for each: its bytes are handed with the words.
  #[arg(long = "file", allow_hyphen_values = true)]
  pub files: Vec<PathBuf>,
  /// Who says them: the owner where none is named, else the supervisor that asked for the run.
  #[arg(long, allow_hyphen_values = true)]
  pub by: Option<String>,
}

/// What a target names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
  Session(String),
  Job(String),
  Repository(String),
  Worker(String),
}

impl Target {
  fn read(text: &str) -> Result<Self, VerbError> {
    let (kind, id) = text.split_once(':').unwrap_or_default();
    let id = id.trim();

    if id.is_empty() {
      return Err(bad_target(text));
    }

    match kind {
      "session" => Ok(Self::Session(id.to_owned())),
      "job" => Ok(Self::Job(id.to_owned())),
      "repository" => Ok(Self::Repository(id.to_owned())),
      "worker" => Ok(Self::Worker(id.to_owned())),
      _ => Err(bad_target(text)),
    }
  }
}

fn bad_target(said: &str) -> VerbError {
  VerbError::Ended {
    code: 2,
    said: format!(
      "{said} is no target: say session:<sessionId>, job:<jobId>, repository:<name> or worker:<checkout>"
    ),
  }
}

/// The supervisor whose program this shell stands in, where one does.
fn speaker() -> String {
  std::env::var("BMO_TURN_AGENT")
    .ok()
    .map(|agent| agent.trim().to_owned())
    .filter(|agent| !agent.is_empty())
    .unwrap_or_else(|| "owner".to_owned())
}

pub async fn run_say(asked: &SayArgs) -> Result<Answer, VerbError> {
  let target = Target::read(&asked.target)?;

  if asked.words.trim().is_empty() && asked.files.is_empty() {
    return Err(VerbError::Refused("words or files (one at least) are required".to_owned()));
  }

  let token = control_token()
    .ok_or_else(|| VerbError::Refused(format!("{CONTROL_TOKEN} is not set in this process")))?;
  let by = asked.by.clone().unwrap_or_else(speaker);
  let mut body = json!({ "words": asked.words, "by": by });
  let mut files = vec![];
  let engine = base64::engine::general_purpose::STANDARD;

  for path in &asked.files {
    let bytes = std::fs::read(path)
      .map_err(|why| VerbError::Refused(format!("{} could not be read: {why}", path.display())))?;
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("attachment").to_owned();

    files.push(json!({ "name": name, "data": engine.encode(bytes) }));
  }

  if !files.is_empty() {
    body["files"] = Value::Array(files);
  }

  let path = match &target {
    Target::Session(id) => format!("/sessions/{id}/words"),
    Target::Job(id) => format!("/jobs/{id}/words"),
    Target::Repository(name) => format!("/repositories/{name}/words"),
    Target::Worker(name) => {
      let repository = repository_of(&token, name).await?;
      body["worker"] = json!(name);
      format!("/repositories/{repository}/words")
    }
  };
  let (status, said) = control_call(&token, reqwest::Method::POST, &path, Some(&body))
    .await
    .map_err(|why| VerbError::Refused(format!("no server answered ({why})")))?;
  let said = said.unwrap_or(Value::Null);

  if !(200..300).contains(&status) {
    let why = said.get("error").and_then(Value::as_str).unwrap_or("no reason given");

    return Err(VerbError::Ended {
      code: 1,
      said: format!("the words did not go in: {why}"),
    });
  }

  Ok(spoken(&said))
}

// What the route answered, as the lines a person reads.
fn spoken(said: &Value) -> Answer {
  let field = |name: &str| said.get(name).and_then(Value::as_str).map(str::to_owned);
  let run = field("run").map(|run| format!(" ({run})")).unwrap_or_default();
  let reason = field("reason");

  match said.get("said").and_then(Value::as_str) {
    Some("delivered") => Answer {
      lines: vec![format!("The turn took the words{run}.")],
      code: 0,
    },
    Some("kept") => Answer {
      lines: vec![format!(
        "No turn of the job took the words now, so they are kept for its next turn{run}."
      )],
      code: 0,
    },
    Some("started") => Answer {
      lines: vec![format!("A turn of the worker's own was started{run}.")],
      code: 0,
    },
    Some("waiting") => Answer {
      lines: vec![format!(
        "The run waits on a person's answer, so the words did not go in{run}. {}",
        reason.unwrap_or_default()
      )],
      code: 1,
    },
    _ => Answer {
      lines: vec![format!("The words did not go in{run}. {}", reason.unwrap_or_default())],
      code: 1,
    },
  }
}

// The repository a checkout is of, as the repositories route answers it.
async fn repository_of(token: &str, worker: &str) -> Result<String, VerbError> {
  let (status, said) = control_call(token, reqwest::Method::GET, "/repositories", None)
    .await
    .map_err(|why| VerbError::Refused(format!("no server answered ({why})")))?;

  if !(200..300).contains(&status) {
    return Err(VerbError::Refused(format!("the repositories answered {status}")));
  }

  let items = said
    .as_ref()
    .and_then(|said| said.get("items"))
    .and_then(Value::as_array)
    .cloned()
    .unwrap_or_default();

  for repository in items {
    let name = repository.get("name").and_then(Value::as_str).unwrap_or_default();
    let checkouts = repository.get("checkouts").and_then(Value::as_array).cloned().unwrap_or_default();

    if name == worker
      || checkouts
        .iter()
        .any(|checkout| checkout.get("name").and_then(Value::as_str) == Some(worker))
    {
      return Ok(name.to_owned());
    }
  }

  Err(VerbError::Ended {
    code: 2,
    said: format!("{worker} is no checkout of any repository"),
  })
}
