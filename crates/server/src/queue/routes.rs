//! Whether an item has a route to run on now (docs/cache-fallback.md, what a job does when its road
//! fails): the selections its job may go out on, in the order a job walks them, and whether one of
//! them is not blocked. The queue's telling asks it of each item at the head of a worker's queue, so a
//! worker is told open only where its turn can actually be launched.

use std::sync::LazyLock;

use harness_providers::{ARCHIVE_PROVIDERS, Machine};
use policy::fill_marks;
use policy::queue_words::QUEUE_WORDS;
use regex::Regex;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};

use super::settings;
use crate::Refusal;
use crate::control::claude_accounts::Judging;
use crate::control::harness::{missing, named, provider_enabled};
use crate::rest::resting;

/// The work key every job of a worker goes out on.
pub const JOB_WORK_KEY: &str = "job-worker";

// The settings key a blocked road stands under, the road's provider or its one model following it.
const BLOCK_PREFIX: &str = "job_route_block:";

// `<provider>::<model>`, where the provider is any this build carries.
static CHOICE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^([a-z0-9-]+)::([a-zA-Z0-9][a-zA-Z0-9._:/-]{0,200})$").unwrap());

// A Claude model named on its own, as the oldest launches name one.
static CLAUDE_MODEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^claude-[a-zA-Z0-9._-]+$").unwrap());

/// One selection read: the provider and the model.
pub struct Choice<'a> {
  pub provider: &'a str,
  pub model: &'a str,
}

/// A selection as a provider and its model, where it names a provider this build carries.
pub fn choice(value: &str) -> Option<Choice<'_>> {
  let read = CHOICE.captures(value)?;
  let provider = read.get(1)?.as_str();

  ARCHIVE_PROVIDERS.contains(&provider).then(|| Choice {
    provider,
    model: read.get(2).map_or("", |model| model.as_str()),
  })
}

/// Whether a value is a selection a job may ask for: the automatic one, a provider's model, or a bare
/// Claude model.
pub fn is_selection(value: &str) -> bool {
  value == "auto" || choice(value).is_some() || CLAUDE_MODEL.is_match(value)
}

/// Why no selection could be made; the walk then has no route.
#[derive(Debug)]
pub struct Unrouted(pub String);

impl<E: std::fmt::Display> From<E> for Unrouted {
  fn from(error: E) -> Self {
    Self(error.to_string())
  }
}

/// The run line a job goes out on, which a provider that does not carry it never answers.
pub fn job_line() -> &'static str {
  JOB_LINE
}

// The line the job role's runs go out on.
const JOB_LINE: &str = "worker";

// What a checkout's repository pins: a provider, and one model of it (docs/repositories.md, 「A pinned
// model」). A pin to a provider the owner turned off reads as none.
pub async fn worker_pin(
  db: &DatabaseConnection,
  worker: &str,
) -> Result<Option<(String, Option<String>)>, Unrouted> {
  let Some((provider, model)) = worker_pin_held(db, worker).await? else {
    return Ok(None);
  };

  if !provider_enabled(db, &provider).await.map_err(|refused| Unrouted(refused.error))? {
    return Ok(None);
  }

  Ok(Some((provider, model)))
}

/// What a checkout's repository pins, whether or not the owner has its provider on: a provider this build
/// carries and the model.
pub async fn worker_pin_held(
  db: &DatabaseConnection,
  worker: &str,
) -> Result<Option<(String, Option<String>)>, Unrouted> {
  let repository = crate::queue::workers::repository_of(db, worker)
    .await
    .map_err(|why| Unrouted(why.detail))?;
  let Some(pin) = repository.and_then(|repository| repository.pin) else {
    return Ok(None);
  };

  if !ARCHIVE_PROVIDERS.contains(&pin.provider.as_str()) {
    return Ok(None);
  }

  Ok(Some((pin.provider, Some(pin.model))))
}

/// A job's selection on each provider of the job role's order, in that order; or the one its repository pins
/// alone, which its work waits for and never leaves (docs/repositories.md, 「A pinned model」).
pub async fn worker_selections(
  db: &DatabaseConnection,
  worker: Option<&str>,
) -> Result<Vec<String>, Unrouted> {
  let pin = match worker {
    Some(worker) => worker_pin_held(db, worker).await?,
    None => None,
  };

  if let Some((provider, Some(model))) = &pin {
    return Ok(vec![format!("{provider}::{model}")]);
  }

  let role = crate::agent_roles::named(db, crate::agent_roles::job_role())
    .await
    .map_err(|why| Unrouted(why.detail))?
    .ok_or_else(|| Unrouted(crate::agent_roles::unknown(crate::agent_roles::job_role())))?;
  let mut selections = vec![];

  for entry in &role.order.entries {
    if provider_enabled(db, &entry.provider).await.map_err(|refused| Unrouted(refused.error))? {
      selections.push(format!("{}::{}", entry.provider, entry.model));
    }
  }

  Ok(selections)
}

/// The selection a job asked for as it goes out: a provider's model as it is named, a bare Claude
/// model on Claude Code, and the first of its role's order otherwise.
pub async fn resolve_job_model(
  db: &DatabaseConnection,
  selection: Option<&str>,
  worker: Option<&str>,
) -> Result<String, Unrouted> {
  if let Some(selection) = selection.filter(|selection| *selection != "auto" && !selection.is_empty()) {
    if !is_selection(selection) {
      return Err(Unrouted(fill_marks(&QUEUE_WORDS.routes.model_wrong, &[("selection", selection)])));
    }

    return Ok(if choice(selection).is_some() {
      selection.to_owned()
    } else {
      format!("claude-code::{selection}")
    });
  }

  worker_selections(db, worker)
    .await?
    .into_iter()
    .next()
    .ok_or_else(|| Unrouted(QUEUE_WORDS.routes.no_model.clone()))
}

/// Every selection a job of a worker may go out on now, in the order the job walks them: the one it
/// asked for where it named one, then the worker's walk on its tier, each kept only where its provider
/// is on, carries the job's line and can answer on this machine.
pub async fn job_route_candidates(
  db: &DatabaseConnection,
  machine: &Machine,
  requested: Option<&str>,
  worker: Option<&str>,
) -> Result<Vec<String>, Unrouted> {
  let mut choices: Vec<String> = vec![];
  let mut add = |selection: String| {
    if choice(&selection).is_some() && !choices.contains(&selection) {
      choices.push(selection);
    }
  };

  if let Some(requested) = requested.filter(|requested| *requested != "auto" && !requested.is_empty())
    && let Some(asked) = choice(requested)
    && provider_enabled(db, asked.provider).await.map_err(|refused| Unrouted(refused.error))?
  {
    add(resolve_job_model(db, Some(requested), worker).await?);
  }

  for selection in worker_selections(db, worker).await? {
    add(selection);
  }

  let mut open = vec![];

  for selection in choices {
    let Some(asked) = choice(&selection) else { continue };
    let provider = named(asked.provider).map_err(|refused| Unrouted(refused.error))?;

    if !provider_enabled(db, asked.provider).await.map_err(|refused| Unrouted(refused.error))?
      || !provider.declared().carries(job_line())
    {
      continue;
    }

    if missing(db, machine, provider)
      .await
      .map_err(|refused| Unrouted(refused.error))?
      .is_none()
    {
      open.push(selection);
    }
  }

  Ok(open)
}

/// Whether a provider's models are services of their own, the registry's registrations.
pub fn own_services(provider: &str) -> bool {
  named(provider).is_ok_and(|provider| provider.unregistered().is_some())
}

// The road a failure closes: the provider, or on a provider whose models are services of their own the
// one model that did not answer.
pub fn health_key(selection: &Choice<'_>) -> String {
  if own_services(selection.provider) {
    format!("job_route_block:{}::{}", selection.provider, selection.model)
  } else {
    format!("job_route_block:{}", selection.provider)
  }
}

/// A selection's road closed until a moment, in epoch milliseconds, with the reason the failure that
/// closed it gives and, where a walk wrote it, the failure's kind (docs/api.md, 「road-blocks and
/// launch-room」). A value naming no provider this build carries closes nothing.
pub async fn block_route(
  db: &DatabaseConnection,
  selection: &str,
  until_ms: i64,
  reason: &str,
  kind: Option<&str>,
) -> Result<bool, Refusal> {
  let Some(asked) = choice(selection) else { return Ok(false) };
  let kept = json!({ "until": until_ms, "reason": reason, "selection": selection, "kind": kind });

  settings::set(db, &health_key(&asked), &kept.to_string()).await?;

  Ok(true)
}

/// One road block as the boundary lists it: the settings row's id, the selection it closes and the
/// moment it opens again, in epoch milliseconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
  pub id: i64,
  pub selection: String,
  pub until: i64,
  pub reason: String,
  /// The settings key the block stands under, which names the road it closes.
  pub key: String,
}

/// Every block the store keeps, in the order they were written.
pub async fn kept_blocks(db: &DatabaseConnection) -> Result<Vec<Block>, Refusal> {
  let rows = store::all(
    db,
    r#"SELECT "rowid" AS "id", "key", "value" FROM "settings" WHERE "key" LIKE $1 ORDER BY "rowid""#,
    vec![format!("{BLOCK_PREFIX}%").into()],
  )
  .await?;
  let mut blocks = vec![];

  for row in rows {
    let key: String = row.try_get("", "key")?;
    let value: Value = row
      .try_get::<String>("", "value")
      .ok()
      .and_then(|text| serde_json::from_str(&text).ok())
      .unwrap_or(Value::Null);
    let named = value.get("selection").and_then(Value::as_str).map(str::to_owned);
    let selection = named.or_else(|| key.strip_prefix(BLOCK_PREFIX).map(str::to_owned));

    blocks.push(Block {
      id: row.try_get("", "id")?,
      until: value
        .get("until")
        .and_then(|at| at.as_i64().or_else(|| at.as_f64().map(|at| at as i64)))
        .unwrap_or(0),
      reason: value.get("reason").and_then(Value::as_str).unwrap_or_default().to_owned(),
      selection: selection.unwrap_or_default(),
      key,
    });
  }

  Ok(blocks)
}

/// The id the store gave a selection's block, where it keeps one.
pub async fn block_id(db: &DatabaseConnection, selection: &str) -> Result<Option<i64>, Refusal> {
  let Some(asked) = choice(selection) else { return Ok(None) };
  let row = store::one(
    db,
    r#"SELECT "rowid" AS "id" FROM "settings" WHERE "key" = $1"#,
    vec![health_key(&asked).into()],
  )
  .await?;

  row.map(|row| row.try_get("", "id")).transpose().map_err(Into::into)
}

/// A block opened again by the id the boundary named: the selection it closed, or none where the store
/// keeps no such block.
pub async fn clear_block_by_id(db: &DatabaseConnection, id: i64) -> Result<Option<String>, Refusal> {
  let row = store::one(
    db,
    r#"DELETE FROM "settings" WHERE "rowid" = $1 AND "key" LIKE $2 RETURNING "key""#,
    vec![id.into(), format!("{BLOCK_PREFIX}%").into()],
  )
  .await?;

  Ok(row.map(|row| row.try_get::<String>("", "key")).transpose()?)
}

/// The roads a model or a worker would walk now, as the keys a block stands under.
pub async fn walked_keys(
  db: &DatabaseConnection,
  machine: &Machine,
  model: Option<&str>,
  worker: Option<&str>,
) -> Result<std::collections::HashSet<String>, Refusal> {
  let candidates = job_route_candidates(db, machine, model, worker)
    .await
    .map_err(|unrouted| Refusal::bad("no-route", unrouted.0))?;

  Ok(
    candidates
      .iter()
      .filter_map(|selection| choice(selection))
      .map(|asked| health_key(&asked))
      .collect(),
  )
}

/// A selection's road opened again before its block ran out.
pub async fn clear_route_block(db: &DatabaseConnection, selection: &str) -> Result<(), Refusal> {
  match choice(selection) {
    Some(asked) => settings::forget(db, &health_key(&asked)).await,
    None => Ok(()),
  }
}

/// Whether a selection's road is shut now: its provider off, Claude Code while the deployment rests, or a
/// failure that closed it still standing.
pub async fn route_blocked(judging: Judging<'_>, selection: &str, now_ms: i64) -> Result<bool, Unrouted> {
  let db = judging.db;
  let Some(asked) = choice(selection) else {
    return Ok(false);
  };

  if !provider_enabled(db, asked.provider).await.map_err(|refused| Unrouted(refused.error))? {
    return Ok(true);
  }

  if asked.provider == "claude-code"
    && resting(judging, now_ms / 1000).await.map_err(|refusal| Unrouted(refusal.detail))?
  {
    return Ok(true);
  }

  Ok(standing_block(db, selection, now_ms).await?.is_some())
}

/// A block standing on a road: the moment it opens again, in epoch milliseconds, the reason the failure that
/// closed it gave, and the failure's kind where a walk wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
  pub until: i64,
  pub reason: String,
  pub kind: Option<String>,
}

/// The block standing on a selection's road at `now_ms`, where one does.
pub async fn standing_block(
  db: &DatabaseConnection,
  selection: &str,
  now_ms: i64,
) -> Result<Option<Standing>, Unrouted> {
  let Some(asked) = choice(selection) else {
    return Ok(None);
  };
  let block = settings::get(db, &health_key(&asked))
    .await
    .map_err(|refusal| Unrouted(refusal.detail))?
    .and_then(|text| serde_json::from_str::<Value>(&text).ok());
  let Some(block) = block else { return Ok(None) };
  let until = block.get("until").and_then(Value::as_f64).unwrap_or(0.0);

  if until <= now_ms as f64 {
    return Ok(None);
  }

  let reason = block.get("reason").and_then(Value::as_str).unwrap_or_default().to_owned();
  let kind = block.get("kind").and_then(Value::as_str).map(str::to_owned);

  Ok(Some(Standing {
    until: until as i64,
    reason,
    kind,
  }))
}

/// Whether an item has a route to run on now: some selection its job may go out on that is not
/// blocked. A walk that could not be read is no route.
pub async fn item_route_open(
  judging: Judging<'_>,
  machine: &Machine,
  model: Option<&str>,
  worker: Option<&str>,
  now_ms: i64,
) -> bool {
  let Ok(candidates) =
    job_route_candidates(judging.db, machine, Some(model.unwrap_or("auto")), worker).await
  else {
    return false;
  };

  for selection in candidates {
    match route_blocked(judging, &selection, now_ms).await {
      Ok(false) => return true,
      Ok(true) => {}
      Err(_) => return false,
    }
  }

  false
}
