//! The rest that closed the last login, published in its boundary shape with the policy's words.

use agency_server::events::Event;
use agency_server::rest::{DeploymentResting, RestKind};
use policy::rest::REST;
use serde_json::json;

#[test]
fn a_limit_names_its_login_and_an_outage_none() {
  assert_eq!(DeploymentResting::NAME, "rest.changed");
  assert_eq!(
    serde_json::to_value(DeploymentResting::of(RestKind::Limit, "spare", 1_790_000_000)).unwrap(),
    json!({ "account": "spare", "kind": "limit", "until": 1_790_000_000, "notice": REST.resting })
  );
  assert_eq!(DeploymentResting::of(RestKind::Outage, "spare", 1).account, None);
}
