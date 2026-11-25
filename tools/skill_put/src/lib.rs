//! `skill_put`: a supervisor saves a skill of the person its session acts for, kept at home and reaching only the
//! agents it is attached to, or removes one, through agency's server (docs/tools.md, 「The skill put」).

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};
use tool_call::{Answer, Asked, Control, Handed, Session};

/// The entry the manifest declares.
pub const NAMESPACE: &str = "bmo";
pub const NAME: &str = "skill_put";
pub const TITLE: &str = "Save a personal skill";
pub const DESCRIPTION: &str = "Save a skill of the person you serve — its SKILL.md and reference files, whole — kept at home, where it reaches only the agents the person attaches it to; or remove one of their own.";
pub const CAPABILITIES: &[&str] = &["room.tools"];

/// What a call may ask.
pub const ACTIONS: &[&str] = &["put", "remove"];

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
  pub files: BTreeMap<String, String>,
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
    return Answer::failure("this session acts for no person, and holds no personal skill");
  };
  let name = input.name.trim();
  let action = input.action.as_deref().unwrap_or("put");
  let address = format!("{}/personal-skills/{account}/{name}", control.base());
  let request = match action {
    "put" if input.files.is_empty() => {
      return Answer::failure("a put names the skill's files, SKILL.md among them");
    }
    "put" => reqwest::Client::new().put(address).json(&json!({ "files": input.files })),
    "remove" => reqwest::Client::new().delete(address),
    other => return Answer::failure(format!("{other} is no action: one of {}", ACTIONS.join(", "))),
  };
  let request = match &control.token {
    Some(token) => request.bearer_auth(token),
    None => request,
  };
  let answered = match request.send().await {
    Ok(answered) => answered,
    Err(why) => {
      return Answer::failure(format!("agency could not be asked to {action} the skill {name}: {why}"));
    }
  };
  let status = answered.status();
  let body: Value = answered.json().await.unwrap_or(Value::Null);

  if !status.is_success() {
    let why = body.get("error").and_then(Value::as_str).unwrap_or("no reason given");
    let done = if action == "remove" { "removed" } else { "saved" };

    return Answer::failure(format!("the skill {name} was not {done}: {why}"));
  }

  Answer::text(match (action, body.get("version").and_then(Value::as_str)) {
    ("remove", _) | (_, None) => format!("The skill {name} is removed, with every attachment of it."),
    (_, Some(version)) => format!(
      "The skill {name} is saved as version {version}. It reaches only the agents the person attaches it to."
    ),
  })
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
