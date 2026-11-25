//! `bmo-agency repository add <name> --path <checkout>` and `repository remove <name>`: a repository's
//! registration written or taken away through the server's own routes (docs/repositories.md, 「What a
//! repository row holds」). The add route refuses a name already taken, and the remove route refuses a removal
//! while a job holds one of the repository's checkouts; this verb prints what either answered, or the route's
//! refusal; nothing on disk — the base checkout, a worktree, a deploy tree — is touched.

use reqwest::Method;
use serde_json::{Map, Value};

use super::{Answer, CONTROL_TOKEN, VerbError, control_call, control_token};

/// What the route answered an addition: the row it wrote, printed as `bmo agents` prints a repository, or
/// the route's refusal.
pub fn added(status: u16, answer: Option<Value>, name: &str, path: &str) -> Result<Answer, VerbError> {
  if (200..300).contains(&status) {
    let written = answer
      .as_ref()
      .and_then(|answer| answer.get("name"))
      .and_then(Value::as_str)
      .unwrap_or(name);
    let at = answer
      .as_ref()
      .and_then(|answer| answer.get("path"))
      .and_then(Value::as_str)
      .unwrap_or(path);

    return Ok(Answer::from(vec![format!("repository {written}: {at}")]));
  }

  let why = answer
    .as_ref()
    .and_then(|answer| answer.get("error"))
    .and_then(Value::as_str)
    .map_or_else(|| format!("agency's server answered {status}"), str::to_owned);

  Err(VerbError::Refused(format!("repository add: {why}")))
}

/// The repository `name` added at `path`, with the label it is shown by where one is named and marked an umbrella
/// where `umbrella` says so, through the server's `POST /repositories` route. Every other field is left at the
/// server's default.
pub async fn add(name: &str, path: &str, label: Option<&str>, umbrella: bool) -> Result<Answer, VerbError> {
  let name = name.trim();
  let path = path.trim();

  let Some(token) = control_token() else {
    return Err(VerbError::Refused(format!(
      "repository add: {CONTROL_TOKEN} is not set in this process."
    )));
  };

  let mut body = Map::new();
  body.insert("name".to_owned(), name.into());

  // A repository names its path: an empty one is left out, so the route refuses it in its own words.
  if !path.is_empty() {
    body.insert("path".to_owned(), path.into());
  }

  if let Some(label) = label {
    body.insert("label".to_owned(), label.into());
  }

  if umbrella {
    body.insert("umbrella".to_owned(), true.into());
  }

  let body = Value::Object(body);
  let (status, answer) = control_call(&token, Method::POST, "/repositories", Some(&body))
    .await
    .map_err(|why| VerbError::Refused(format!("repository add: agency's server did not answer: {why}")))?;

  added(status, answer, name, path)
}

/// What the route answered a removal: the line naming what went, or the route's refusal.
pub fn removed(name: &str, status: u16, answer: Option<serde_json::Value>) -> Result<Answer, VerbError> {
  if (200..300).contains(&status) {
    return Ok(Answer::from(vec![format!("repository remove: removed {name}.")]));
  }

  let why = answer
    .as_ref()
    .and_then(|answer| answer.get("error"))
    .and_then(serde_json::Value::as_str)
    .map_or_else(|| format!("agency's server answered {status}"), str::to_owned);

  Err(VerbError::Refused(format!("repository remove: {why}")))
}

/// The repository `name` removed by the server's `DELETE /repositories/{repositoryName}` route.
pub async fn run(name: &str) -> Result<Answer, VerbError> {
  let name = name.trim();

  if name.is_empty() {
    return Err(VerbError::Refused("repository remove: name the repository to remove.".to_owned()));
  }

  let Some(token) = control_token() else {
    return Err(VerbError::Refused(format!(
      "repository remove: {CONTROL_TOKEN} is not set in this process."
    )));
  };
  let path = format!("/repositories/{name}");
  let (status, answer) = control_call(&token, Method::DELETE, &path, None).await.map_err(|why| {
    VerbError::Refused(format!("repository remove: agency's server did not answer: {why}"))
  })?;

  removed(name, status, answer)
}
