//! The gateway's status as its launcher last wrote it (docs/harness-machine.md, 「The gateway's
//! status」): the table it serves, the one starting, how many old children are still finishing, and
//! when; and where the gateway answers.

use std::path::Path;

use policy::providers::registered::REGISTERED;
use serde::Serialize;
use serde_json::Value;

/// Where agency's gateway answers: the address the process names in `BMO_LLM_GATEWAY_URL`, without a
/// trailing slash, and the policy's otherwise.
pub fn url(named: Option<&str>) -> String {
  match named.map(str::trim).filter(|named| !named.is_empty()) {
    Some(named) => named.trim_end_matches('/').to_owned(),
    None => REGISTERED.gateway.url.clone(),
  }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GatewayStatus {
  pub serving: Value,
  pub starting: Value,
  pub draining: Value,
  pub at: Value,
}

/// The status under the data root, or none where the launcher has written none or it does not read.
/// A field it left out reads as none, nothing starting, no child finishing and time zero.
pub fn status(data_root: &Path) -> Option<GatewayStatus> {
  let text = std::fs::read_to_string(data_root.join("llm-gateway").join("status.json")).ok()?;
  let read: Value = serde_json::from_str(&text).ok()?;
  let field = |name: &str, missing: Value| match read.get(name) {
    None | Some(Value::Null) => missing,
    Some(value) => value.clone(),
  };

  // Any value but null reads, a field it does not hold falling to its default.
  (!read.is_null()).then(|| GatewayStatus {
    serving: field("serving", Value::Null),
    starting: field("starting", Value::Null),
    draining: field("draining", 0.into()),
    at: field("at", 0.into()),
  })
}
