//! `app_put`: a supervisor puts the personal app a worker ask wrote for the person its session acts for, sends it
//! back to an earlier version, or removes it, through agency's server, which asks the person's PC and the tool host
//! (docs/tools.md, 「The app put」).

use serde::Deserialize;
use serde_json::{Value, json};
use tool_call::{Answer, Asked, Control, Handed, Session};

/// The entry the manifest declares.
pub const NAMESPACE: &str = "bmo";
pub const NAME: &str = "app_put";
pub const TITLE: &str = "Put a personal app";
pub const DESCRIPTION: &str = "Build the personal app a worker run wrote for the person you serve into a new version on their PC and put it, so it stands in their sessions' toolbook as my/<name> from the next turn; or send it back to an earlier version, or remove it.";
pub const CAPABILITIES: &[&str] = &["room.tools"];

/// What a call may ask.
pub const ACTIONS: &[&str] = &["put", "back", "remove"];

pub fn entry() -> String {
  format!("{NAMESPACE}/{NAME}")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PutInput {
  pub name: String,
  #[serde(default)]
  pub action: Option<String>,
  #[serde(default)]
  pub version: Option<String>,
}

/// One call against the server the record's data root describes.
pub async fn put(session: &Session, input: PutInput) -> Answer {
  let control = Control::read(&session.data_root(), |name| std::env::var(name).ok());

  put_with(&control, session, input).await
}

/// The same call against the server the caller names, for a test of the refusals and the round trip.
pub async fn put_with(control: &Control, session: &Session, input: PutInput) -> Answer {
  let Some(account) = session.owner_account.as_deref().map(str::trim).filter(|account| !account.is_empty())
  else {
    return Answer::failure("this session acts for no person, and holds no personal app");
  };
  let name = input.name.trim();
  let action = input.action.as_deref().unwrap_or("put");
  let base = format!("{}/personal-apps/{account}/{name}", control.base());
  let request = match action {
    "put" => reqwest::Client::new().post(format!("{base}/put")).json(&json!({})),
    "back" => reqwest::Client::new()
      .post(format!("{base}/back"))
      .json(&json!({ "version": input.version })),
    "remove" => reqwest::Client::new().delete(base),
    other => return Answer::failure(format!("{other} is no action: one of {}", ACTIONS.join(", "))),
  };
  let request = match &control.token {
    Some(token) => request.bearer_auth(token),
    None => request,
  };
  let answered = match request.send().await {
    Ok(answered) => answered,
    Err(why) => {
      return Answer::failure(format!("agency could not be asked to {action} the app {name}: {why}"));
    }
  };
  let status = answered.status();
  let body: Value = answered.json().await.unwrap_or(Value::Null);

  if !status.is_success() {
    let why = body.get("error").and_then(Value::as_str).unwrap_or("no reason given");

    return Answer::failure(format!("the app {name} was not {}: {why}", done_word(action)));
  }

  Answer::text(said(action, name, &body))
}

fn done_word(action: &str) -> &'static str {
  match action {
    "back" => "sent back",
    "remove" => "removed",
    _ => "put",
  }
}

// What a call that went through answers: the version in place, the entries reached from the next turn, and what the
// host and the build said.
fn said(action: &str, name: &str, body: &Value) -> String {
  let mut text = match (action, body.get("version").and_then(Value::as_str)) {
    ("remove", _) | (_, None) => {
      format!("The app {name} is removed: no session reaches it from its next call.")
    }
    ("back", Some(version)) => format!("The app {name} is back on version {version}."),
    (_, Some(version)) => format!("The app {name} is put as version {version}."),
  };
  let entries: Vec<&str> = body
    .get("entries")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|entry| entry.get("name").and_then(Value::as_str))
    .collect();

  if !entries.is_empty() {
    text.push_str(&format!(" From the next turn its sessions reach {}.", entries.join(", ")));
  }

  let lines: Vec<&str> = body
    .get("said")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .collect();

  if !lines.is_empty() {
    text.push_str("\n\n");
    text.push_str(&lines.join("\n"));
  }

  text
}

pub async fn call(session: &Session, arguments: Value) -> Result<Answer, String> {
  let input: PutInput =
    serde_json::from_value(arguments).map_err(|why| format!("invalid arguments: {NAME}: {why}"))?;

  Ok(put(session, input).await)
}

pub async fn answer(handed: Handed) -> Result<Value, String> {
  let answered = match handed.asked {
    Asked::Call {
      entry: asked,
      arguments,
    } if asked == entry() => call(&handed.session, arguments).await.unwrap_or_else(Answer::failure),
    Asked::Call { entry: asked, .. } => Answer::failure(format!("this executable holds no entry {asked}")),
    Asked::ResourcesList => return Ok(json!({ "resources": [] })),
    Asked::ResourcesRead { .. } => return Ok(json!({ "error": "this executable serves no resources" })),
    Asked::Other => return Err("this executable answers calls alone".to_owned()),
  };

  serde_json::to_value(answered).map_err(|why| format!("the answer could not be written: {why}"))
}
