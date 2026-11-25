//! What agency's control routes answer a caller with where they refuse a change (docs/api.md): one
//! section a route family, each `{name}` filled in by the route.

use serde::Deserialize;

/// A request no route can read.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestWords {
  pub not_json: String,
  pub action: String,
}

/// The fallback roads, the run modes and the provider order.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallbackWords {
  pub no_provider: String,
  pub models_unread: String,
  pub unknown_key: String,
  pub models_retired: String,
  pub order: String,
  pub all_off: String,
  pub not_carried: String,
  pub provider_switch: String,
  pub run_mode: String,
  pub unknown_action: String,
  pub not_offered: String,
  pub cell_off: String,
  pub cell_off_detail: String,
  pub cell_not_carried_detail: String,
  pub cell_unsettled: String,
  pub cell_unsettled_detail: String,
  pub cell_unresolved: String,
}

/// A Claude account signed in from the console.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginsWords {
  pub which: String,
  pub not_directory: String,
  pub directory_taken: String,
  pub no_link: String,
  pub none: String,
  pub code_needed: String,
  pub refused: String,
  pub no_login: String,
  pub silent: String,
  pub no_node: String,
  pub node_not_attached: String,
  pub node_unready: String,
  pub login_elsewhere: String,
  pub no_cli: String,
  pub unread: String,
}

/// A person's own keys for the gateway's roads.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalSupplyWords {
  pub no_person: String,
  pub unknown_road: String,
  pub road_needed: String,
  pub unknown: String,
  pub place: String,
  pub enabled: String,
  pub nothing: String,
  pub off_boarding: String,
  pub taken: String,
}

/// An organisation's own supply (docs/supply.md, 「Organisation supply」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganisationSupplyWords {
  pub not_head: String,
  pub unchecked: String,
  pub closed: String,
  pub unknown: String,
  pub enabled: String,
  pub taken: String,
  pub no_person: String,
}

/// The Claude accounts.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountsWords {
  pub switch: String,
  pub run_on: String,
  pub credential_kind: String,
  pub credential_path: String,
  pub endpoint_needed: String,
  pub endpoint_url: String,
  pub key_file_needed: String,
  pub key_not_file: String,
  pub models_shape: String,
  pub base_prompt_shape: String,
  pub name_needed: String,
  pub which_to_change: String,
  pub which_to_remove: String,
  pub choice: String,
  pub unknown: String,
  pub off: String,
  pub no_id: String,
  pub id_shape: String,
  pub id_taken: String,
  pub name_empty: String,
  pub name_long: String,
  pub name_taken: String,
  pub path_empty: String,
  pub path_relative: String,
  pub path_missing: String,
  pub path_not_directory: String,
  pub path_not_file: String,
  pub order: String,
  pub last_off: String,
  pub last_gone: String,
  pub chosen_off: String,
  pub base_prompt: String,
  pub secret_needed: String,
  pub secret_name: String,
  pub lend_owner: String,
  pub not_unit_owned: String,
  pub unit_owner: String,
  pub unit_named: String,
  pub platform_only: String,
}

/// A worker's own sampling.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkersSamplingWords {
  pub no_agent: String,
  pub not_worker: String,
  pub follows_base: String,
  pub shape: String,
  pub unknown: String,
}

/// A worker changed, and its own sampling.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkersWords {
  pub back_here_path: String,
  pub not_found: String,
  pub nothing: String,
  pub not_removed: String,
  pub sampling: WorkersSamplingWords,
}

/// An agent's own switches and its registration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentsWords {
  pub not_found: String,
  pub no_gates: String,
  pub gates_of: String,
  pub gate_switch: String,
  pub follows_tier: String,
  pub follows_base: String,
  pub thinking: String,
  pub effort: String,
  pub role: String,
  pub kind: String,
  pub owner: String,
  pub seeded: String,
  pub running: String,
  pub model_method: String,
  pub model_registration: String,
  pub model_registration_missing: String,
  pub model_route_only: String,
  pub model_provider: String,
  pub model_line: String,
  pub model_none: String,
}

/// The roles an agent may hold, as the owner adds, changes and removes them.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRolesWords {
  pub name: String,
  pub taken: String,
  pub unknown: String,
  pub label: String,
  pub bundles: String,
  pub line: String,
  pub bound: String,
  pub prompt: String,
  pub description: String,
  pub fold_ratio: String,
  pub sessions: String,
  pub held: String,
  pub default_role: String,
}

/// The base prompts.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BasePromptsWords {
  pub unknown: String,
  pub label: String,
  pub text: String,
  pub name_shape: String,
  pub taken: String,
  pub in_use: String,
  pub in_use_accounts: String,
  pub in_use_both: String,
  pub not_chooser: String,
  pub choice: String,
  pub reset_reason: String,
}

/// A session's reset.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionsWords {
  pub none: String,
  pub none_in_room: String,
  pub asked_shape: String,
  pub asked: String,
}

/// A supervisor's settings.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorsWords {
  pub not_supervisor: String,
  pub generation_pairs: String,
  pub not_generation: String,
  pub reference: String,
  pub brain: String,
  pub tools: String,
  pub own_pairs: String,
  pub own_name: String,
  pub own_size: String,
  pub pieces: String,
  pub pieces_twice: String,
  pub pieces_none: String,
  pub persona: String,
  pub registration: String,
  pub not_field: String,
  pub thinking: String,
  pub reset_reason: String,
}

/// A generation setting's value out of what it takes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationWords {
  pub switch: String,
  pub number: String,
  pub whole: String,
  pub range: String,
  pub above: String,
  pub least: String,
  pub most: String,
}

/// The tier settings.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TiersWords {
  pub provider: String,
  pub tier: String,
  pub key: String,
  pub conflicts: String,
  pub mapping: String,
  pub answer_tokens: String,
}

/// The registry of models.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryWords {
  pub tier: String,
  pub worker_pin: String,
  pub supervisor_pin: String,
  pub key_pin: String,
  pub model_file: String,
  pub name: String,
  pub node: String,
  pub url_unread: String,
  pub url_shape: String,
  pub key_file: String,
  pub whole: String,
  pub label: String,
  pub model: String,
  pub context_label: String,
  pub answer_label: String,
  pub answer_under_window: String,
  pub switches: String,
  pub keeps_research: String,
  pub unknown: String,
  pub key_text: String,
  pub role_files: String,
  pub in_use: String,
  pub last_address: String,
  pub no_address: String,
  pub the_registration: String,
  pub card_service: String,
  pub card_role_files: String,
  pub card_service_address: String,
  pub languages: String,
  pub account: String,
  pub unit: String,
  pub owner_only: String,
  /// With `{name}`.
  pub unknown_member: String,
  /// With `{why}`.
  pub members_unread: String,
  /// A key file an address names that is not one plain file name.
  pub key_file_name: String,
  /// An address that both gives a key and names a key file.
  pub key_both: String,
}

/// The model catalogue.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelsWords {
  pub source_unread: String,
  pub source_address: String,
  pub source_shape: String,
  pub word_unread: String,
  pub above: String,
  pub whole: String,
  pub number: String,
  pub label: String,
  pub file: String,
  pub bytes: String,
  pub sha256: String,
  pub note: String,
  pub engine_word: String,
  pub registration_word: String,
  pub thinking: String,
  pub name: String,
  pub unknown: String,
  pub switching_to: String,
  pub serving: String,
  pub aside: String,
  pub in_hand: String,
  pub no_engine: String,
  pub nowhere: String,
  pub engine_switching: String,
  pub unit_failed: String,
  pub node_unread: String,
}

/// A supervisor registered.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorRosterWords {
  pub name: String,
  pub name_taken: String,
  pub label: String,
  pub call_names: String,
}

/// A supervisor's reset.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorResetWords {
  pub no_agent: String,
  pub no_range: String,
  pub not_supervisor: String,
  pub brain_root: String,
  pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlWords {
  pub request: RequestWords,
  pub fallback: FallbackWords,
  pub accounts: AccountsWords,
  pub logins: LoginsWords,
  pub personal_supply: PersonalSupplyWords,
  pub organisation_supply: OrganisationSupplyWords,
  pub workers: WorkersWords,
  pub agents: AgentsWords,
  pub agent_roles: AgentRolesWords,
  pub base_prompts: BasePromptsWords,
  pub sessions: SessionsWords,
  pub supervisors: SupervisorsWords,
  pub generation: GenerationWords,
  pub tiers: TiersWords,
  pub registry: RegistryWords,
  pub models: ModelsWords,
  pub supervisor_roster: SupervisorRosterWords,
  pub supervisor_reset: SupervisorResetWords,
  pub cards: CardWords,
}

/// The cards the arbiter's master grants a run (docs/own-llm.md, 「An engine the arbiter stands」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardWords {
  pub no_master: String,
  pub waiting: String,
  pub unreachable: String,
  pub gone: String,
  pub stood_down: String,
  pub stopped: String,
  /// With `{entry}` and `{seconds}`.
  pub unserved: String,
}

crate::part!(pub CONTROL_WORDS: ControlWords = "control-words.toml");
