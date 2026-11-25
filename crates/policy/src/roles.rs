//! The roles and what is no role's own (docs/agent-roles.md): the roles themselves are the store's rows, which
//! these declarations seed once; the bundles, the supervisor bundles, the role a job and an upkeep run hold
//! where nothing names one, the sending bundle, the marks and the
//! shape of a role's name are the policy's.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One function bundle a role may give: a capability of the tool host, with the short name a screen
/// writes it by.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
  pub name: String,
  pub label: String,
}

/// Whether a role's runs go on in the session an earlier run left while its cache is warm, or each opens its
/// own (docs/agent-roles.md, 「A role's sessions」).
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Sessions {
  #[default]
  Keep,
  Fresh,
}

impl Sessions {
  /// The word a row and the routes write it by.
  pub fn word(self) -> &'static str {
    match self {
      Self::Keep => "keep",
      Self::Fresh => "fresh",
    }
  }

  /// The setting a word names, where it names one.
  pub fn of(word: &str) -> Option<Self> {
    match word {
      "keep" => Some(Self::Keep),
      "fresh" => Some(Self::Fresh),
      _ => None,
    }
  }

  /// Whether a run under it goes on in a kept session.
  pub fn keeps(self) -> bool {
    self == Self::Keep
  }
}

/// One entry of a seeded role's provider order: the provider, once, and the model, the effort and the thinking
/// the role runs on there (docs/provider-order.md, 「What a road is」).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OrderEntry {
  pub provider: String,
  pub model: String,
  #[serde(default)]
  pub effort: Option<String>,
  #[serde(default)]
  pub thinking: Option<bool>,
}

/// One role a store is seeded with: its name, its label as a screen writes it, its line, the file under
/// `prompts/roles/` whose text is its prompt, the files under `prompts/roles/` whose text a run of it is given
/// beside that prompt where its program is a named provider's, the one sentence that is its description, the
/// bundles it gives, the tool entries its runs are shown by name, its cap, whether its runs keep their
/// sessions and its provider order.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
  pub name: String,
  pub label: String,
  pub line: String,
  #[serde(default)]
  pub prompt: Option<String>,
  /// The file under `prompts/roles/` whose text a run of it is given beside its prompt, by the provider whose
  /// program the run stands on (docs/agent-roles.md, 「A role's provider prompts」).
  #[serde(default)]
  pub provider_prompts: std::collections::BTreeMap<String, String>,
  /// What the role is for, in one sentence.
  #[serde(default)]
  pub description: Option<String>,
  pub bundles: Vec<String>,
  #[serde(default)]
  pub shown: Vec<String>,
  /// The most of its runs that go at once.
  pub max_concurrent: i64,
  /// Whether its runs keep their sessions; `keep` where the declaration names none.
  #[serde(default)]
  pub sessions: Sessions,
  pub order: Vec<OrderEntry>,
  /// Whether a run of it follows the supervisor that asks it, its agents' own roads standing where none does
  /// (docs/role-agents.md, 「An agent that follows its asker」); off where the declaration names none.
  #[serde(default)]
  pub follows_asker: bool,
}

/// Every bundle and every seeded role, each in the order a screen lists them, and the roles nothing names.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Roles {
  /// The role a job runs as where its item names none.
  pub job: String,
  /// The role agency's own runs hold that stand for no one's work.
  pub renewal: String,
  /// The role a translation agency is asked for runs as.
  pub translate: String,
  /// The role a shared app's review runs as.
  pub app_review: String,
  /// The bundle that opens sending into a room.
  pub send: String,
  /// The bundles every supervisor's session record holds.
  pub supervisor: Vec<String>,
  /// The capabilities through which a session reaches a brain, which a supervisor whose brain is off holds
  /// none of.
  pub brain: Vec<String>,
  /// The capabilities through which a session reaches the gate's reading of a brain, which a supervisor whose
  /// reference switch is off holds none of.
  pub reference: Vec<String>,
  /// The shape of a role's name.
  pub name_pattern: String,
  /// The most characters a role's name holds.
  pub name_most: usize,
  /// The most characters a role's label holds.
  pub label_most: usize,
  /// The mark a screen shows beside every supervisor, role and repository.
  pub icons: BTreeMap<String, String>,
  pub bundles: Vec<Bundle>,
  pub roles: Vec<Role>,
}

impl Roles {
  /// The mark a screen shows beside every member of this axis, where the policy names one.
  pub fn icon(&self, axis: &str) -> Option<&str> {
    self.icons.get(axis).map(String::as_str)
  }

  /// The seed of a role of this name.
  pub fn role(&self, name: &str) -> Option<&Role> {
    self.roles.iter().find(|role| role.name == name)
  }

  /// Whether the policy seeds a role of this name.
  pub fn holds(&self, name: &str) -> bool {
    self.role(name).is_some()
  }

  /// Whether the policy declares a bundle of this name, which a role may then give.
  pub fn declares_bundle(&self, name: &str) -> bool {
    self.bundles.iter().any(|bundle| bundle.name == name)
  }

  /// Whether a role of this name is one the policy names as a default, which is never removed.
  pub fn default_role(&self, name: &str) -> bool {
    [&self.job, &self.renewal].iter().any(|role| *role == name)
  }

  /// The short name a screen writes a bundle by, or the bundle's own name where none is declared.
  pub fn bundle_label<'a>(&'a self, name: &'a str) -> &'a str {
    self
      .bundles
      .iter()
      .find(|bundle| bundle.name == name)
      .map_or(name, |bundle| &bundle.label)
  }
}

crate::part!(pub ROLES: Roles = "roles.toml");
