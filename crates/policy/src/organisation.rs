//! What agency reads off the account service's delegation chain for the person an agent serves, beside its
//! prompt layers: where a person's organisation supply goes out, the agents an account is given as it is made
//! ready, who may add and change repositories, and who may make personal apps and what one may ask for
//! (docs/supply.md, docs/role-agents.md, docs/repositories.md, docs/person-nodes.md).

use serde::Deserialize;

use crate::prompt_layers::Kind;

/// Organisation supply's words, and the account service's setting agency reads it by.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
  /// The source word a run on its organisation's own credential carries.
  pub organisation_word: String,
  /// With `{provider}`.
  pub no_organisation: String,
  /// The key of the account service's kind saying whether organisation supply stands open.
  pub organisation_kind: String,
}

/// The sign-up composition's setting.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Composition {
  pub key: String,
  pub description: String,
  pub shape: String,
  pub mode: String,
  pub rules: bool,
  pub levels: Vec<String>,
  pub unit_permission: String,
  pub person_writer: String,
  /// The most agents of one role a composition makes for one person.
  pub most_agents: usize,
}

impl Composition {
  /// The composition as a kind the account service takes, with no default: none set gives the platform's own.
  pub fn kind(&self) -> Kind {
    Kind {
      key: self.key.clone(),
      description: self.description.clone(),
      shape: self.shape.clone(),
      choices: Vec::new(),
      mode: self.mode.clone(),
      rules: self.rules,
      levels: self.levels.clone(),
      unit_permission: self.unit_permission.clone(),
      person_writer: self.person_writer.clone(),
      default: None,
    }
  }
}

/// The personal apps' settings: who may put one, and the ceiling of what one may ask for.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalApps {
  pub make: Kind,
  pub network: Kind,
  pub secrets: Kind,
  pub share_scope: Kind,
  pub share_review: Kind,
  pub install: Kind,
}

/// A person's own changes over the agents their organisation supplies: whether they may change the set, whether they
/// may choose their agents' roads, and the most one of their instructions holds.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnAgents {
  pub most_characters: usize,
  pub add: Kind,
  pub road: Kind,
}

/// The settings that stood in agency's own settings table, held on the chain (docs/api.md, 「settings」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
  pub research_supervisor: Kind,
  pub deploy_flush_minutes: Kind,
  pub deepseek_card_secret: Kind,
  pub deepseek_login_secret: Kind,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Organisation {
  pub sources: Sources,
  pub composition: Composition,
  pub repositories: Kind,
  pub personal_apps: PersonalApps,
  /// Whether a person may add a local AI of their own (docs/supply.md, 「A person's local AI」).
  pub personal_local_ai: Kind,
  pub own_agents: OwnAgents,
  /// The head check for the resources filed under a unit (docs/api.md, 「Who a caller acts for」).
  pub resources: Kind,
  pub settings: Settings,
}

crate::part!(pub ORGANISATION: Organisation = "organisation.toml");
