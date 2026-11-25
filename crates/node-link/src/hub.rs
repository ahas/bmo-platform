//! A jake hub's settings file as agency reads it, the control credential made from it that reaches one agent and
//! no other (docs/person-nodes.md, 「Who reaches a node」), and the hub let go of the listens it keeps for an agent
//! (「The link over jake」).

use std::path::Path;

use hmac::{Hmac, Mac};

/// The variable of the hub's settings its control key is kept under, the key a credential scoped to named agents
/// is made with.
pub const CONTROL_KEY: &str = "JAKE_CONTROL_KEY";

/// The variable the hub's connect program reads a scoped control credential from, presenting it in the control
/// token's place.
pub const CREDENTIAL_VARIABLE: &str = "JAKE_CONTROL_CREDENTIAL";

/// One setting of the hub's settings file, where the file sets it.
pub fn setting(file: &Path, wanted: &str) -> Result<Option<String>, String> {
  let shown = file.display();

  for item in dotenvy::from_path_iter(file).map_err(|why| format!("{shown}: {why}"))? {
    let (key, value) = item.map_err(|why| format!("{shown}: {why}"))?;

    if key == wanted {
      return Ok(Some(value));
    }
  }

  Ok(None)
}

/// The control credential that reaches the agent named alone: its name, a dot, and the lowercase hex of
/// HMAC-SHA256 over the name keyed by the hub's control key, as jake's hub checks it. Refused where the settings
/// hold no control key, so the control token is never what stands in for it.
pub fn agent_credential(file: &Path, agent: &str) -> Result<String, String> {
  if agent.is_empty() || agent.contains(',') {
    return Err(format!("{agent:?} is no agent name a control credential can be scoped to"));
  }

  let key = setting(file, CONTROL_KEY)?
    .map(|key| key.trim().to_owned())
    .filter(|key| !key.is_empty())
    .ok_or_else(|| {
      format!(
        "{} holds no {CONTROL_KEY}, which the credential reaching the agent {agent} alone is made with",
        file.display()
      )
    })?;
  let mut mac =
    Hmac::<sha2::Sha256>::new_from_slice(key.as_bytes()).expect("HMAC takes a key of any length");

  mac.update(agent.as_bytes());

  Ok(format!("{agent}.{}", hex::encode(mac.finalize().into_bytes())))
}

/// Every listen the hub keeps for the agent let go of, at the control port the hub's settings name and with the
/// credential reaching that agent alone; the ports it let go of, or why it could not be asked.
pub async fn drop_listens(file: &Path, agent: &str) -> Result<Vec<u16>, String> {
  let port = match setting(file, "JAKE_CONTROL_PORT")? {
    Some(port) => port.parse::<u16>().map_err(|_| format!("JAKE_CONTROL_PORT {port} is no port"))?,
    None => 8791,
  };
  let credential = agent_credential(file, agent)?;
  let client = reqwest::Client::new();
  let held: serde_json::Value = client
    .get(format!("http://127.0.0.1:{port}/listens"))
    .query(&[("agent", agent)])
    .bearer_auth(&credential)
    .send()
    .await
    .and_then(reqwest::Response::error_for_status)
    .map_err(|why| format!("the hub's listens: {why}"))?
    .json()
    .await
    .map_err(|why| format!("the hub's listens: {why}"))?;
  let ports: Vec<u16> = held
    .get("listens")
    .and_then(serde_json::Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|listen| u16::try_from(listen.get("port")?.as_u64()?).ok())
    .collect();
  let mut dropped = Vec::new();

  for held in ports {
    let asked = client
      .delete(format!("http://127.0.0.1:{port}/listen"))
      .query(&[("agent", agent), ("port", &held.to_string())])
      .bearer_auth(&credential)
      .send()
      .await
      .and_then(reqwest::Response::error_for_status);

    match asked {
      Ok(_) => dropped.push(held),
      Err(why) => return Err(format!("port {held}: {why}")),
    }
  }

  Ok(dropped)
}
