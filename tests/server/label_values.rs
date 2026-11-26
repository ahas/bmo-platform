//! What an item's `role`, `agent` and `model` labels may name, published to the job service
//! (docs/agent-control.md, 「A label agency does not hold is refused as it is set」): every role, every agent
//! switched on or off, every model an agent's road names by its name and as `<provider>/<model>`, and `auto`
//! under each; each key put where it differs from what was last published, and nothing put where none does.

use agency_server::label_values::{publish, values};
use agency_server::queue::orderer::JobService;
use agency_server::role_agents::RoleAgent;
use serde_json::{Value, json};

use crate::support::peers::stand;
use crate::support::scratch::Scratch;

fn agent(name: &str, role: &str, road: Option<(&str, &str)>, enabled: bool) -> RoleAgent {
  RoleAgent {
    name: name.to_owned(),
    role: role.to_owned(),
    owner_account: None,
    owner_unit: None,
    home_node: None,
    provider: road.map(|(provider, _)| provider.to_owned()),
    model: road.map(|(_, model)| model.to_owned()),
    effort: None,
    thinking: None,
    answer_tokens: None,
    advisor: None,
    enabled,
    keeps_cache: false,
    sessions: Default::default(),
    fold_ratio: None,
    prompt: None,
    bundles: None,
    skills: None,
    added_at: 0,
    follows_asker: None,
  }
}

#[test]
fn a_label_may_name_every_role_every_agent_on_or_off_every_model_a_road_names_and_auto() {
  let roles = ["worker".to_owned(), "coder".to_owned()];
  let agents = [
    agent("coder-a", "coder", Some(("claude-code", "opus")), true),
    agent("coder-b", "coder", Some(("codex", "gpt")), false),
    agent("worker-01", "worker", None, true),
  ];
  let held = values(&roles, &agents);

  assert_eq!(held["role"], ["auto", "coder", "worker"]);
  assert_eq!(held["agent"], ["auto", "coder-a", "coder-b", "worker-01"], "switched off as much as on");
  assert_eq!(
    held["model"],
    ["auto", "claude-code/opus", "codex/gpt", "gpt", "opus"],
    "by name and by provider, a road that names none adding nothing"
  );
  assert_eq!(held.len(), 3);
}

#[tokio::test]
async fn each_key_is_put_to_the_job_service_once_and_again_only_where_it_changed() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let (peers, vars) = stand().await;
  let service = JobService::of(&vars).unwrap();

  publish(db, &service).await.unwrap();

  let held = values(
    &agency_server::agent_roles::names(db).await.unwrap(),
    &agency_server::role_agents::every(db).await.unwrap(),
  );
  let put = peers.lock().unwrap().label_values();
  let expected: Vec<(String, Value)> =
    held.iter().map(|(key, values)| ((*key).to_owned(), json!(values))).collect();

  assert_eq!(put, expected, "each key put with the store's values");

  publish(db, &service).await.unwrap();
  assert_eq!(peers.lock().unwrap().label_values().len(), 3, "nothing changed, nothing put");

  // An agent added: its key alone is put again.
  scratch
    .run(r#"INSERT INTO "role_agents" ("name", "role") VALUES ('extra-01', 'worker')"#, vec![])
    .await;
  publish(db, &service).await.unwrap();

  let put = peers.lock().unwrap().label_values();

  assert_eq!(put.len(), 4, "{put:?}");
  assert_eq!(put[3].0, "agent");
  assert!(put[3].1.as_array().unwrap().contains(&json!("extra-01")), "{put:?}");
}
