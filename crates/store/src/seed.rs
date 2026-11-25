//! What a store is given beside its tables (docs/store.md, 「Its seeds」): the account, the model catalogue,
//! the base prompts and the roles. The values are the deployment's, handed in by whoever settles the store;
//! this module knows none of them and supplies only when and how each is written.

use std::time::{SystemTime, UNIX_EPOCH};

use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait, Value};

use crate::statements::{Fault, run};

/// The agents a role is seeded with, as many as its declaration's count, each on the road it names
/// (docs/role-agents.md, 「The roles the policy seeds」), for a role that holds none yet.
pub const AGENTS_OF_ROLES: &str = include_str!("seed/agents_of_roles.sql");

/// Every agent given what its role's row holds of the sessions setting and the compaction threshold, as a
/// seeded role's agents are given theirs (docs/role-agents.md, 「What an agent row holds」).
pub const AGENT_SETTINGS: &str = r#"UPDATE "role_agents" AS a
  SET "sessions" = r."sessions", "fold_ratio" = COALESCE(a."fold_ratio", r."fold_ratio")
  FROM "roles" AS r
  WHERE r."name" = a."role""#;

/// Everything a store is seeded with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seeds {
  /// The account a store holding none is given.
  pub account: Option<AccountSeed>,
  /// The entries the catalogue is given once, as its table is made.
  pub catalogue: Vec<CatalogueSeed>,
  /// The base prompts each given once by its name.
  pub base_prompts: Vec<BasePromptSeed>,
  /// The roles, each given once by its name, in the order a screen lists them.
  pub roles: Vec<RoleSeed>,
}

/// One role a store is given once by its name (docs/agent-roles.md, 「The roles the policy seeds」): its
/// label, the bundles it gives, its line, its provider order as the JSON list a row keeps with its quota
/// share, its cap, the text of its prompt and its description, whether its runs keep their sessions,
/// `keep` or `fresh`, its compaction threshold, and the text a run of it is given beside its prompt by the
/// provider its program stands on, each given once by the role and the provider.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleSeed {
  pub name: String,
  pub label: String,
  pub bundles: Vec<String>,
  pub line: String,
  pub order: String,
  pub max_concurrent: i64,
  pub prompt: Option<String>,
  pub description: Option<String>,
  pub sessions: String,
  pub fold_ratio: f64,
  pub provider_prompts: Vec<(String, String)>,
  /// Whether its runs follow the supervisor that asks them, its agents' own roads standing where none does.
  pub follows_asker: bool,
}

/// One base prompt a store is given once by its name (docs/base-prompts.md, 「The seeds」), with the
/// agents given the choice of it where it is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasePromptSeed {
  pub name: String,
  pub label: String,
  pub text: String,
  pub agents: Vec<String>,
}

/// The one account a store holding none is given: the login the machine already has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountSeed {
  pub id: String,
  pub name: String,
  pub credential_kind: String,
  pub credential_path: String,
}

/// One model file of the catalogue (docs/model-catalogue.md); it names no node, since where a file lies
/// is a fact of the machines.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogueSeed {
  pub name: String,
  pub label: String,
  pub file: String,
  pub bytes: i64,
  pub sha256: String,
  pub source: Option<String>,
  pub engine: Option<String>,
  pub registration: Option<String>,
  pub thinking: Option<bool>,
  pub temperature: Option<f64>,
  pub top_p: Option<f64>,
  pub presence_penalty: Option<f64>,
  pub max_tokens: Option<i64>,
  pub note: Option<String>,
}

/// The store brought to this build's shape and its seeds given, as one settling: the store brought to its
/// shape (`crate::migration::up`), then every seed in one transaction. A seed given once is not given again,
/// so a row the owner removed stays removed.
pub async fn settle(db: &DatabaseConnection, seeds: &Seeds) -> Result<(), Fault> {
  let made = crate::migration::up(db).await?;

  let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs() as i64);
  let writing = db.begin().await?;

  if made {
    for entry in &seeds.catalogue {
      catalogue_entry(&writing, entry, now).await?;
    }
  }
  if let Some(account) = &seeds.account {
    account_seed(&writing, account, now).await?;
  }
  for prompt in &seeds.base_prompts {
    base_prompt_seed(&writing, prompt, now).await?;
  }
  for (place, role) in seeds.roles.iter().enumerate() {
    role_seed(&writing, role, place as i64, now * 1000).await?;

    for (provider, text) in &role.provider_prompts {
      provider_prompt_seed(&writing, &role.name, provider, text, now * 1000).await?;
    }
  }

  writing.commit().await?;

  Ok(())
}

fn text(value: &Option<String>) -> Value {
  Value::from(value.clone())
}

async fn catalogue_entry<C: ConnectionTrait>(db: &C, entry: &CatalogueSeed, now: i64) -> Result<(), Fault> {
  run(
    db,
    r#"INSERT INTO "model_catalogue" ("name", "label", "file", "bytes", "sha256", "source", "engine",
         "registration", "thinking", "temperature", "top_p", "presence_penalty", "max_tokens", "note", "added_at")
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)"#,
    vec![
      entry.name.clone().into(),
      entry.label.clone().into(),
      entry.file.clone().into(),
      entry.bytes.into(),
      entry.sha256.clone().into(),
      text(&entry.source),
      text(&entry.engine),
      text(&entry.registration),
      entry.thinking.map(i64::from).into(),
      entry.temperature.into(),
      entry.top_p.into(),
      entry.presence_penalty.into(),
      entry.max_tokens.into(),
      text(&entry.note),
      now.into(),
    ],
  )
  .await?;

  Ok(())
}

// The account is given to a store holding none; a list the owner has written to is left alone.
async fn account_seed<C: ConnectionTrait>(db: &C, account: &AccountSeed, now: i64) -> Result<(), Fault> {
  run(
    db,
    r#"INSERT INTO "claude_accounts" ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at")
       SELECT $1, $2, 1, 1, $3, $4, $5
       WHERE NOT EXISTS (SELECT 1 FROM "claude_accounts")"#,
    vec![
      account.id.clone().into(),
      account.name.clone().into(),
      account.credential_kind.clone().into(),
      account.credential_path.clone().into(),
      now.into(),
    ],
  )
  .await?;

  Ok(())
}

// A base prompt is given once by its name, recorded as given, so one the owner removes stays removed. A
// prompt of that name already kept — the owner's own, or one an earlier store held — is left as it stands,
// and only a prompt written now gives its agents the choice of it; a choice an agent already holds stands.
async fn base_prompt_seed<C: ConnectionTrait>(
  db: &C,
  prompt: &BasePromptSeed,
  now: i64,
) -> Result<(), Fault> {
  let given = run(
    db,
    r#"INSERT INTO "base_prompt_seeded" ("name") VALUES ($1) ON CONFLICT DO NOTHING"#,
    vec![prompt.name.clone().into()],
  )
  .await?;

  if given == 0 {
    return Ok(());
  }

  let written = run(
    db,
    r#"INSERT INTO "base_prompts" ("name", "label", "text", "updated_at") VALUES ($1, $2, $3, $4)
       ON CONFLICT DO NOTHING"#,
    vec![
      prompt.name.clone().into(),
      prompt.label.clone().into(),
      prompt.text.clone().into(),
      now.into(),
    ],
  )
  .await?;

  if written == 0 {
    return Ok(());
  }

  for agent in &prompt.agents {
    run(
      db,
      r#"INSERT INTO "agent_base_prompts" ("agent", "prompt") VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
      vec![agent.clone().into(), prompt.name.clone().into()],
    )
    .await?;
  }

  Ok(())
}

// A role is given once by its name, recorded as given, so one the owner removes stays removed; a role of
// that name already held stands as it is. Its place is the one the policy declares it at, which a role the
// owner adds later comes after.
async fn role_seed<C: ConnectionTrait>(
  db: &C,
  role: &RoleSeed,
  place: i64,
  at_ms: i64,
) -> Result<(), Fault> {
  let given = run(
    db,
    r#"INSERT INTO "role_seeded" ("name") VALUES ($1) ON CONFLICT DO NOTHING"#,
    vec![role.name.clone().into()],
  )
  .await?;

  if given == 0 {
    return Ok(());
  }

  let bundles = serde_json::to_string(&role.bundles).map_err(|error| Fault::new(error.to_string()))?;

  run(
    db,
    r#"INSERT INTO "roles" ("name", "label", "bundles", "max_concurrent", "prompt", "position", "created_at",
         "updated_at", "sessions", "description", "line", "provider_order", "fold_ratio", "follows_asker")
       VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8, $9, $10, $11, $12, $13) ON CONFLICT DO NOTHING"#,
    vec![
      role.name.clone().into(),
      role.label.clone().into(),
      bundles.into(),
      role.max_concurrent.into(),
      text(&role.prompt),
      place.into(),
      at_ms.into(),
      role.sessions.clone().into(),
      text(&role.description),
      role.line.clone().into(),
      role.order.clone().into(),
      role.fold_ratio.into(),
      i64::from(role.follows_asker).into(),
    ],
  )
  .await?;

  // A role is seeded with its agents (docs/role-agents.md, 「The roles the policy seeds」): as many as its
  // declaration's count, each on the road it names, given once with the role.
  run(
    db,
    &format!(r#"{} AND r."name" = $1"#, AGENTS_OF_ROLES.trim_end()),
    vec![role.name.clone().into()],
  )
  .await?;
  // Its agents take the sessions setting and the compaction threshold its declaration states
  // (docs/role-agents.md, 「What an agent row holds」).
  run(db, &format!(r#"{AGENT_SETTINGS} AND a."role" = $1"#), vec![role.name.clone().into()]).await?;

  Ok(())
}

// A role's provider prompt is given once by the role and the provider, recorded as given, so one the owner
// removes stays removed and one the owner changed stands as it is; it is given to a role already held as
// much as to one seeded now (docs/agent-roles.md, 「A role's provider prompts」).
async fn provider_prompt_seed<C: ConnectionTrait>(
  db: &C,
  role: &str,
  provider: &str,
  text: &str,
  at_ms: i64,
) -> Result<(), Fault> {
  let given = run(
    db,
    r#"INSERT INTO "role_provider_prompt_seeded" ("role", "provider") VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
    vec![role.into(), provider.into()],
  )
  .await?;

  if given == 0 {
    return Ok(());
  }

  run(
    db,
    r#"INSERT INTO "role_provider_prompts" ("role", "provider", "prompt", "updated_at")
       SELECT $1, $2, $3, $4 WHERE EXISTS (SELECT 1 FROM "roles" WHERE "name" = $1)
       ON CONFLICT DO NOTHING"#,
    vec![role.into(), provider.into(), text.into(), at_ms.into()],
  )
  .await?;

  Ok(())
}
