//! An own service's door as a run on it is held to it (docs/own-llm.md, 「How requests go」): the
//! gateway's key, readable only as a file of 0600, and the gateway's list of models, which must name the
//! service's own registration once. A failure says the service and a class of failure and never a body,
//! a request or the key.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::registry::OwnLlmService;

// How many times the list is asked where its failure may pass.
const ATTEMPTS: u32 = 2;

// A list read whole, and the complaint a failure is drained of, each held to a ceiling.
const MAX_SUCCESS_BYTES: usize = 512 * 1_024;

/// Why a door call failed: its class, the status the service answered with where it answered one, and
/// whether asking again may help.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnLlmError {
  pub kind: &'static str,
  pub status: Option<u16>,
  transient: bool,
}

impl OwnLlmError {
  /// A failure of a class that asking again does not help.
  pub fn of(kind: &'static str) -> Self {
    Self {
      kind,
      status: None,
      transient: false,
    }
  }

  fn passing(kind: &'static str) -> Self {
    Self {
      transient: true,
      ..Self::of(kind)
    }
  }

  /// A failure the service answered with a status.
  pub fn status(status: u16) -> Self {
    let (kind, transient) = match status {
      401 | 403 => ("unauthorized", false),
      408 => ("timeout", true),
      429 => ("rate-limited", true),
      _ => ("service", status >= 500),
    };

    Self {
      kind,
      status: Some(status),
      transient,
    }
  }

  /// The failure as the service it came from is named in it.
  pub fn said(&self, service: &OwnLlmService) -> String {
    match self.status {
      None => format!("{} {}", service.short_label, self.kind),
      Some(status) => format!("{} {} ({status})", service.short_label, self.kind),
    }
  }
}

/// The gateway's key, from a file of something at 0600: a file with looser permissions is counted as
/// absent.
pub fn credential(path: &Path) -> Result<String, OwnLlmError> {
  use std::os::unix::fs::PermissionsExt;

  let usable = std::fs::metadata(path)
    .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o777 == 0o600 && meta.len() > 0);
  let key = usable.then(|| std::fs::read_to_string(path).ok()).flatten().unwrap_or_default();
  let key = key.trim();

  if key.is_empty() {
    return Err(OwnLlmError::of("key-unavailable"));
  }

  Ok(key.to_owned())
}

/// The models the gateway lists, each by its id, read under the list's own wait.
pub async fn models(service: &OwnLlmService, key: &str) -> Result<Vec<String>, OwnLlmError> {
  let url = format!("{}/v1/models", service.base_url);
  let body = request(&url, key).await?;
  let parsed: Value = if body.trim().is_empty() {
    return Err(OwnLlmError::of("empty"));
  } else {
    serde_json::from_str(&body).map_err(|_| OwnLlmError::of("malformed"))?
  };
  let data = parsed
    .get("data")
    .and_then(Value::as_array)
    .ok_or_else(|| OwnLlmError::of("malformed"))?;
  let ids: Vec<String> = data
    .iter()
    .filter_map(|entry| entry.get("id").and_then(Value::as_str))
    .map(str::trim)
    .filter(|id| !id.is_empty())
    .map(str::to_owned)
    .collect();

  if ids.is_empty() {
    return Err(OwnLlmError::of("empty"));
  }

  let owners = data.iter().filter_map(|entry| entry.get("owned_by").and_then(Value::as_str));

  if ids.iter().map(String::as_str).chain(owners).any(|said| said.contains(key)) {
    return Err(OwnLlmError::of("credential-reflected"));
  }

  Ok(ids)
}

/// The service's own registration out of the gateway's list, which must name it exactly once.
pub fn selectable(service: &OwnLlmService, models: &[String]) -> Result<String, OwnLlmError> {
  let mut offered = models.iter().filter(|id| **id == service.registration);

  match (offered.next(), offered.next()) {
    (Some(one), None) => Ok(one.clone()),
    _ => Err(OwnLlmError::of("catalogue-changed")),
  }
}

/// Whether the gateway's list offers the model a run goes out on as the service's own registration.
pub fn offers(service: &OwnLlmService, models: &[String], model: &str) -> Result<(), OwnLlmError> {
  if selectable(service, models)? == model {
    Ok(())
  } else {
    Err(OwnLlmError::of("catalogue-changed"))
  }
}

// One GET waited on until it answers or its connection is refused or broken, asked again once where its
// failure may pass.
async fn request(url: &str, key: &str) -> Result<String, OwnLlmError> {
  let client = reqwest::Client::new();
  let mut last = OwnLlmError::passing("network");

  for attempt in 0..ATTEMPTS {
    let asked = client
      .get(url)
      .bearer_auth(key)
      .header("accept", "application/json")
      .header("content-type", "application/json")
      .send()
      .await;

    last = match asked {
      Ok(answer) if answer.status().is_success() => return bounded(answer).await,
      Ok(answer) => OwnLlmError::status(answer.status().as_u16()),
      Err(_) => OwnLlmError::passing("network"),
    };

    if !last.transient || attempt + 1 >= ATTEMPTS {
      return Err(last);
    }

    let pause = Duration::from_millis(100 * u64::from(attempt + 1));

    tokio::time::sleep(pause).await;
  }

  Err(last)
}

// A body read whole up to its ceiling.
async fn bounded(mut answer: reqwest::Response) -> Result<String, OwnLlmError> {
  let mut body = Vec::new();

  loop {
    match answer.chunk().await {
      Ok(Some(part)) => {
        body.extend_from_slice(&part);

        if body.len() > MAX_SUCCESS_BYTES {
          return Err(OwnLlmError::of("response-too-large"));
        }
      }
      Ok(None) => return Ok(String::from_utf8_lossy(&body).into_owned()),
      Err(_) => return Err(OwnLlmError::passing("network")),
    }
  }
}
