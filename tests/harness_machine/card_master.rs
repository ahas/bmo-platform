//! The arbiter's master as agency reads it (docs/harness-machine.md, 「The arbiter's master」): its token out
//! of the arbiter deployment's settings file, and a service's standing instances and whether a node of its
//! placement is open.

use harness_machine::card_master::{Demand, MasterAddress, standing_in, token_of};
use serde_json::json;

#[test]
fn the_token_is_the_settings_file_s_card_master_token() {
  let dir = tempfile::tempdir().unwrap();
  let file = dir.path().join("env");

  std::fs::write(&file, "CARD_MASTER_PORT=7440\nCARD_MASTER_TOKEN=master-token\n").unwrap();
  assert_eq!(token_of(&file).as_deref(), Some("master-token"));

  std::fs::write(&file, "CARD_MASTER_PORT=7440\n").unwrap();
  assert_eq!(token_of(&file), None, "a file with no such entry is no token");

  let named = file.to_string_lossy().into_owned();

  std::fs::write(&file, "CARD_MASTER_TOKEN=master-token\n").unwrap();

  let read = MasterAddress::read(|name| match name {
    "AGENCY_ARBITER_URL" => Some("http://127.0.0.1:7440".to_owned()),
    "AGENCY_ARBITER_TOKEN_FILE" => Some(named.clone()),
    _ => None,
  })
  .unwrap()
  .unwrap();

  assert_eq!(read.token.as_deref(), Some("master-token"));
  assert_eq!(MasterAddress::read(|_| None).unwrap(), None, "no address names no master");
  assert!(MasterAddress::read(|_| Some("ftp://host".to_owned())).is_err());
}

fn picture(cards: serde_json::Value) -> serde_json::Value {
  json!({
    "tiers": [{ "name": "bench", "tier": 1 }, { "name": "arena", "tier": 2 }],
    "services": [{
      "serviceName": "arena", "tier": 2, "nodeNames": ["n3", "n4"],
      "instances": [
        { "nodeName": "n3", "address": "http://[::1]:9/v1/chat/completions", "keyFile": "/keys/n3.key", "demands": 1 },
        { "nodeName": "n4", "demands": 0 },
      ],
    }],
    "nodes": [
      { "name": "n3", "link": { "state": "connected" }, "cards": [cards] },
      { "name": "n4", "link": { "state": "lost" }, "cards": [{ "queue": [] }] },
    ],
  })
}

#[test]
fn a_service_is_open_while_a_node_of_its_placement_is_held_by_no_claim_and_no_lower_tier() {
  let open = standing_in(&picture(json!({ "queue": [{ "serviceName": "arena" }] })), "arena").unwrap();

  assert!(open.open);
  assert_eq!(open.instances.len(), 1, "an instance naming no address is no instance callers reach");
  assert_eq!(open.instances[0].node_name, "n3");
  assert_eq!(open.instances[0].key_file.as_deref(), Some("/keys/n3.key"));

  for held in [
    json!({ "claim": { "claimantName": "trainer" }, "queue": [] }),
    json!({ "turn": { "serviceName": "bench" }, "queue": [] }),
    json!({ "queue": [{ "serviceName": "bench" }] }),
  ] {
    assert!(!standing_in(&picture(held.clone()), "arena").unwrap().open, "{held}");
  }

  assert_eq!(standing_in(&picture(json!({})), "arena-coder"), None);
}

#[test]
fn a_demand_holds_its_card_only_while_granted_and_untaken() {
  let read = |value: serde_json::Value| serde_json::from_value::<Demand>(value).unwrap();
  let card = json!([{ "nodeName": "n3", "address": "http://[::1]:9/v1/chat/completions" }]);

  assert_eq!(
    read(json!({ "id": 1, "serviceName": "arena", "cards": card, "state": "granted" }))
      .card()
      .map(|c| c.node_name.as_str()),
    Some("n3")
  );

  let taken =
    read(json!({ "id": 1, "serviceName": "arena", "cards": card, "state": "granted", "taken": "bench" }));

  assert!(taken.card().is_none());
  assert_eq!(taken.waiting_on().as_deref(), Some("The card was taken by bench."));
  assert!(read(json!({ "id": 1, "serviceName": "arena", "state": "lent" })).lent());
}
