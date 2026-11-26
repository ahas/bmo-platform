//! The roles as the store keeps them (docs/agent-roles.md, 「What a role row holds」): every field read off a
//! role's row at every use, and a role the store does not hold giving nothing.

use agency_server::agent_roles::{bundles_of, named, prompt_of, provider_prompt, provider_prompts};

use crate::support::scratch::Scratch;

#[tokio::test]
async fn a_roles_row_is_read_whole_and_a_role_the_store_does_not_hold_gives_nothing() {
  let scratch = Scratch::new().await;

  scratch
    .run(
      r#"INSERT INTO "roles" ("name", "label", "bundles", "max_concurrent", "prompt", "position", "created_at",
           "updated_at", "description", "line", "provider_order", "fold_ratio")
         VALUES ('making', 'Making', '["web.read"]', 1, 'Make it.', 0, 1, 1, 'What making is for.', 'worker',
           '[{"provider":"deepseek","model":"deepseek-flash","effort":"low"}]', 0.5)"#,
      vec![],
    )
    .await;
  crate::support::rows::reseated(scratch.db()).await;

  let making = named(scratch.db(), "making").await.unwrap().unwrap();

  assert_eq!(making.max_concurrent, 1);
  assert_eq!(making.line, "worker");
  assert_eq!(making.order.entry("deepseek").and_then(|entry| entry.effort.as_deref()), Some("low"));
  assert_eq!(making.fold_ratio, 0.5);
  assert_eq!(bundles_of(scratch.db(), "making").await.unwrap(), ["web.read"]);
  assert_eq!(prompt_of(scratch.db(), "making").await.unwrap().as_deref(), Some("Make it."));
  scratch
    .run(
      r#"INSERT INTO "role_provider_prompts" ("role", "provider", "prompt", "updated_at")
         VALUES ('making', 'claude-code', 'Use your subagents.', 1)"#,
      vec![],
    )
    .await;
  assert_eq!(
    provider_prompt(scratch.db(), "making", "claude-code").await.unwrap().as_deref(),
    Some("Use your subagents.")
  );
  assert!(provider_prompt(scratch.db(), "making", "deepseek").await.unwrap().is_none());
  assert_eq!(provider_prompts(scratch.db(), "making").await.unwrap().len(), 1);

  assert!(named(scratch.db(), "nobody").await.unwrap().is_none());
  assert!(bundles_of(scratch.db(), "nobody").await.unwrap().is_empty());
}

// An agent of a role on Claude Code, on a model, with an advisor.
fn advised(name: &str, model: &str, advisor: &str) -> agency_server::role_agents::RoleAgent {
  agency_server::role_agents::RoleAgent {
    name: name.to_owned(),
    role: "translating".to_owned(),
    owner_account: None,
    owner_unit: None,
    home_node: None,
    provider: Some("claude-code".to_owned()),
    model: Some(model.to_owned()),
    effort: None,
    thinking: None,
    answer_tokens: None,
    advisor: Some(advisor.to_owned()),
    enabled: true,
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

/// a run that holds no seat — a translation — goes out as the agent its road was taken from, with that agent's
/// advisor: the first free agent on the road, a busy one passed over, and each model's own
#[tokio::test]
async fn a_run_that_holds_no_seat_goes_out_as_the_agent_its_road_was_taken_from() {
  let scratch = Scratch::new().await;

  scratch
    .run(
      r#"INSERT INTO "roles" ("name", "label", "bundles", "max_concurrent", "position", "created_at",
           "updated_at", "line", "provider_order", "fold_ratio")
         VALUES ('translating', 'Translating', '[]', 1, 0, 1, 1, 'translate', '[]', 0.5)"#,
      vec![],
    )
    .await;

  let role = named(scratch.db(), "translating").await.unwrap().unwrap();
  let agents = vec![
    advised("tr-a", "claude-sonnet-5", "opus"),
    advised("tr-b", "claude-sonnet-5", "fable"),
    advised("tr-c", "claude-opus-5-5", "fable"),
  ];
  let taken = |role: &agency_server::agent_roles::AgentRole, model: &str| {
    role
      .agent_of_road("claude-code", model)
      .map(|agent| (agent.name.clone(), agent.advisor.clone().unwrap_or_default()))
  };

  let free = role.clone().seated(agents.clone(), &[]);

  assert_eq!(taken(&free, "claude-sonnet-5"), Some(("tr-a".to_owned(), "opus".to_owned())));
  assert_eq!(taken(&free, "claude-opus-5-5"), Some(("tr-c".to_owned(), "fable".to_owned())));
  assert_eq!(taken(&free, "claude-haiku-5-5"), None);

  let busy = role.seated(agents, &["tr-a".to_owned()]);

  assert_eq!(taken(&busy, "claude-sonnet-5"), Some(("tr-b".to_owned(), "fable".to_owned())));
}
