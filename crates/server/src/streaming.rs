//! The block a session's answer is writing right now, read from the model gateway that counts it as it passes
//! (docs/own-llm.md, 「The gateway」; docs/api.md, 「sessions」).

use harness_providers::Machine;
use harness_providers::registered::Registered;
use serde_json::Value;

/// The open block the gateway holds for a program session, by the session's own id; none where the gateway holds
/// none or does not answer.
pub async fn open_block(machine: &Machine, program_session: &str) -> Option<Value> {
  let url = harness_machine::gateway::url(machine.var("BMO_LLM_GATEWAY_URL"));
  let key = std::fs::read_to_string(Registered::key_file(machine)).ok()?;
  let answer = reqwest::Client::new()
    .get(format!("{url}/gateway/streams/{program_session}"))
    .bearer_auth(key.trim())
    .send()
    .await
    .ok()?;

  if !answer.status().is_success() {
    return None;
  }

  answer.json::<Value>().await.ok()
}
