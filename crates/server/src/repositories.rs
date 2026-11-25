//! The repositories as the store keeps them (docs/repositories.md, 「What a repository row holds」): a place
//! runs work in, never an agent; every field is read off its row at every use.

use sea_orm::{ConnectionTrait, QueryResult};
use store::{Fault, all, one};

/// A provider and one model of it, as a repository's pin (docs/repositories.md, 「A pinned model」), an item's
/// model and a row's forced provider (docs/provider-order.md, 「What a road is」) name one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
  pub provider: String,
  pub model: String,
}

impl Pin {
  /// The pin two columns hold, none where either is missing or empty.
  pub fn of_parts(provider: Option<String>, model: Option<String>) -> Option<Self> {
    provider
      .filter(|provider| !provider.trim().is_empty())
      .zip(model.filter(|model| !model.trim().is_empty()))
      .map(|(provider, model)| Self { provider, model })
  }

  /// The pin as JSON answers it.
  pub fn view(&self) -> serde_json::Value {
    serde_json::json!({ "provider": self.provider, "model": self.model })
  }
}

/// The label an item names the model its job runs on with, beside its role (docs/repositories.md, 「An agent an
/// item names」).
pub const MODEL_LABEL: &str = "model";

/// The label an item names the role its job runs as with (docs/repositories.md, 「A role an item names」).
pub const ROLE_LABEL: &str = "role";

/// The label an item names the agent its job runs on with (docs/repositories.md, 「An agent an item names」).
pub const AGENT_LABEL: &str = "agent";

/// The label an item names the other repositories its job is lent a checkout of with, comma-separated
/// (docs/worker-slots.md, 「A checkout lent to a job」).
pub const REPOSITORIES_LABEL: &str = "repositories";

/// The repositories an item's labels name to be lent, each once, in the order named; none where the label is
/// absent or names nothing.
pub fn named_repositories(labels: &serde_json::Value) -> Vec<String> {
  let mut named: Vec<String> = vec![];

  for name in labels[REPOSITORIES_LABEL].as_str().unwrap_or_default().split(',').map(str::trim) {
    if !name.is_empty() && !named.iter().any(|kept| kept == name) {
      named.push(name.to_owned());
    }
  }

  named
}

/// The value of a label that names nothing, as an absent or empty one.
pub const AUTO: &str = "auto";

/// What an item's labels name under `key`, trimmed; none where the label is absent, empty or `auto`.
pub fn named_by<'a>(labels: &'a serde_json::Value, key: &str) -> Option<&'a str> {
  labels[key].as_str().map(str::trim).filter(|named| !named.is_empty() && *named != AUTO)
}

/// One repository as its row holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repository {
  pub name: String,
  pub label: Option<String>,
  pub owner_account: Option<i64>,
  /// The organisation unit it belongs to in place of an account, whose people's agents alone work on it
  /// (docs/repositories.md, 「An organisation's repository」).
  pub owner_unit: Option<i64>,
  /// The base checkout, where its jobs work.
  pub path: String,
  /// The work node the base checkout stands on, where it stands on one.
  pub node: Option<String>,
  /// How many support worktrees stand beside the base, or `auto` for as many as its launches grow.
  pub support_count: bmo_agency_client::SupportCount,
  /// The paths of the documents a run's prefix carries here, in order.
  pub prefix_documents: Vec<String>,
  pub preamble: Option<String>,
  /// The prompt folder under `prompts/agents/` its rules are read from.
  pub rules: Option<String>,
  pub dev_command: Option<String>,
  pub pin: Option<Pin>,
  /// Whether it is an umbrella: a folder holding other repositories' checkouts, whose own tree its jobs never
  /// write (docs/worker-slots.md, 「An umbrella repository」).
  pub umbrella: bool,
  pub downgrade_heal: bool,
}

impl Repository {
  /// Whether a launch that finds every checkout held grows the next support checkout: set to `auto`, its base
  /// on this machine (docs/worker-slots.md, 「Growing a checkout at a launch」).
  pub fn grows(&self) -> bool {
    !self.umbrella
      && self.support_count.stored().is_none()
      && self.node.as_deref().is_none_or(str::is_empty)
  }

  pub fn of(row: &QueryResult) -> Result<Self, Fault> {
    let documents: Option<String> = row.try_get("", "prefix_documents")?;
    let provider: Option<String> = row.try_get("", "pinned_provider")?;
    let model: Option<String> = row.try_get("", "pinned_model")?;

    Ok(Self {
      name: row.try_get("", "name")?,
      label: row.try_get("", "label")?,
      owner_account: row.try_get("", "owner_account")?,
      owner_unit: row.try_get("", "owner_unit")?,
      path: row.try_get("", "path")?,
      node: row.try_get("", "node")?,
      support_count: bmo_agency_client::SupportCount::of_stored(row.try_get("", "support_count")?),
      prefix_documents: documents.and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default(),
      preamble: row.try_get("", "preamble")?,
      rules: row.try_get("", "prompts")?,
      dev_command: row.try_get("", "dev_command")?,
      pin: Pin::of_parts(provider, model),
      umbrella: row.try_get::<i64>("", "umbrella")? != 0,
      downgrade_heal: row.try_get::<i64>("", "downgrade_heal")? != 0,
    })
  }
}

/// Every repository the store holds, by name.
pub async fn every<C: ConnectionTrait>(db: &C) -> Result<Vec<Repository>, Fault> {
  all(db, r#"SELECT * FROM "repositories" ORDER BY "name""#, vec![])
    .await?
    .iter()
    .map(Repository::of)
    .collect()
}

/// The repository of this name, where the store holds one.
pub async fn named<C: ConnectionTrait>(db: &C, name: &str) -> Result<Option<Repository>, Fault> {
  one(db, r#"SELECT * FROM "repositories" WHERE "name" = $1"#, vec![name.into()])
    .await?
    .as_ref()
    .map(Repository::of)
    .transpose()
}

/// The repository whose base checkout is this path, where one is.
pub async fn at_path<C: ConnectionTrait>(db: &C, path: &str) -> Result<Option<Repository>, Fault> {
  one(db, r#"SELECT * FROM "repositories" WHERE "path" = $1"#, vec![path.into()])
    .await?
    .as_ref()
    .map(Repository::of)
    .transpose()
}
