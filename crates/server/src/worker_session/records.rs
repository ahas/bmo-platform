//! What a job's turn at a checkout leaves (docs/sessions.md, 「A role's session」): the session the next job of
//! the role at that checkout on that model resumes, kept as a role session; the heal of a session the
//! account's safeguard answered on a lesser model; and a supervisor's last answer, in the store; the windows the turn read, kept (docs/usage-windows.md); and
//! what it spent, published for the reader that keeps usage (docs/api.md, 「Events」).

use cache_siren::Keeping;
use harness_providers::claude_code::ClaudeCode;
use sea_orm::ConnectionTrait;
use store::{all, one, run};

use super::told::WorkerDowngraded;
use crate::control::claude_accounts::SEED_ACCOUNT;
use crate::door::record::{ModelSpend, Reply, UsageSpent};
use crate::events::Bus;
use crate::places::Ground;
use crate::{Refusal, now_ms};

// The role a checkout's kept sessions are kept under: the role a job runs as where its item names none.
fn job_role() -> &'static str {
  crate::agent_roles::job_role()
}

// The provider a checkout's kept sessions stand on: its program is Claude Code's.
const PROVIDER: &str = super::PROVIDER;

/// The folder a checkout works in, read off its repository's row: the base's path, a support worktree's beside
/// it, or the path a node checkout stands at; none for a name no repository holds.
pub async fn checkout_path(db: &impl ConnectionTrait, checkout: &str) -> Result<Option<String>, Refusal> {
  if let Some(row) =
    one(db, r#"SELECT "path" FROM "repositories" WHERE "name" = $1"#, vec![checkout.into()]).await?
  {
    return Ok(Some(row.try_get("", "path")?));
  }

  if let Some((base, slot)) = checkout
    .rsplit_once("-w")
    .and_then(|(base, slot)| Some((base, slot.parse::<i64>().ok()?)))
    && let Some(row) =
      one(db, r#"SELECT "path" FROM "repositories" WHERE "name" = $1"#, vec![base.into()]).await?
  {
    let path: String = row.try_get("", "path")?;

    return Ok(Some(format!("{}-w{slot}", crate::queue::paths::resolve(&path).display())));
  }

  if let Some((base, node)) = checkout.split_once('@')
    && let Some(row) = one(
      db,
      r#"SELECT "path" FROM "repository_node_checkouts" WHERE "repository" = $1 AND "node" = $2"#,
      vec![base.into(), node.into()],
    )
    .await?
  {
    return Ok(Some(row.try_get("", "path")?));
  }

  Ok(None)
}

// The path a checkout's sessions are kept at, its name where no repository holds it.
async fn path_of(db: &impl ConnectionTrait, checkout: &str) -> Result<String, Refusal> {
  Ok(checkout_path(db, checkout).await?.unwrap_or_else(|| checkout.to_owned()))
}

/// One session a checkout keeps for the job role, on one model and login (docs/sessions.md, 「A role's
/// session」).
#[derive(Clone, Debug, PartialEq)]
pub struct WorkerSession {
  pub model: String,
  pub account: String,
  pub session_id: String,
  /// When it last answered, in epoch seconds.
  pub last_reply_at: i64,
  /// The model it answers on where that is not its own.
  pub answered_model: Option<String>,
  /// The tokens its last answered turn carried, and the share of the window they filled.
  pub context_tokens: Option<i64>,
  pub context_ratio: Option<f64>,
}

/// Every session a checkout keeps for the job role, one per model and login, oldest answer first.
pub async fn sessions(db: &impl ConnectionTrait, worker: &str) -> Result<Vec<WorkerSession>, Refusal> {
  let rows = all(
    db,
    r#"SELECT "model", "account", "session", "answered_at", "context_tokens", "context_share" FROM "role_sessions"
       WHERE "role" = $1 AND "path" = $2 AND "provider" = $3 AND "dropped" = 0 AND "answered_at" IS NOT NULL
       ORDER BY "answered_at""#,
    vec![job_role().into(), path_of(db, worker).await?.into(), PROVIDER.into()],
  )
  .await?;

  Ok(
    rows
      .iter()
      .map(|row| -> Result<WorkerSession, sea_orm::DbErr> {
        Ok(WorkerSession {
          model: row.try_get("", "model")?,
          account: row.try_get::<Option<String>>("", "account")?.unwrap_or_default(),
          session_id: row.try_get("", "session")?,
          last_reply_at: row.try_get::<i64>("", "answered_at")? / 1000,
          answered_model: None,
          context_tokens: row.try_get("", "context_tokens")?,
          context_ratio: row.try_get("", "context_share")?,
        })
      })
      .collect::<Result<_, _>>()?,
  )
}

/// The context a kept session's answered turn carried, kept on its row.
pub async fn note_carried(
  db: &impl ConnectionTrait,
  worker: &str,
  model: &str,
  account: &str,
  _fresh: bool,
  reply: &Reply,
) -> Result<(), Refusal> {
  run(
    db,
    r#"UPDATE "role_sessions" SET "context_tokens" = $5, "context_share" = $6
       WHERE "role" = $1 AND "path" = $2 AND "model" = $3 AND "account" IS NOT DISTINCT FROM $4 AND "dropped" = 0"#,
    vec![
      job_role().into(),
      path_of(db, worker).await?.into(),
      model.into(),
      (!account.is_empty()).then(|| account.to_owned()).into(),
      reply.context_tokens.into(),
      reply.context_ratio.into(),
    ],
  )
  .await?;

  Ok(())
}

/// The generation a checkout's session on a model stands in: how many sessions were opened on it, the first
/// counted as one.
pub async fn generation(db: &impl ConnectionTrait, worker: &str, model: &str) -> Result<u64, Refusal> {
  let count = one(
    db,
    r#"SELECT COUNT(*) AS "count" FROM "role_sessions" WHERE "role" = $1 AND "path" = $2 AND "model" = $3"#,
    vec![job_role().into(), path_of(db, worker).await?.into(), model.into()],
  )
  .await?
  .map(|row| row.try_get::<i64>("", "count"))
  .transpose()?
  .unwrap_or(0);

  Ok(u64::try_from(count).unwrap_or(0).max(1))
}

/// The checkout's session on a model and login a turn may resume: none where there is none, or where one
/// idle past the life the configuration gives a session is retired.
pub async fn resumable(
  db: &impl ConnectionTrait,
  worker: &str,
  model: &str,
  account: &str,
  ttl_ms: Option<i64>,
) -> Result<Option<String>, Refusal> {
  let row = one(
    db,
    r#"SELECT "id", "session", "answered_at" FROM "role_sessions"
       WHERE "role" = $1 AND "path" = $2 AND "model" = $3 AND "account" IS NOT DISTINCT FROM $4 AND "dropped" = 0
         AND "answered_at" IS NOT NULL
       ORDER BY "answered_at" DESC LIMIT 1"#,
    vec![
      job_role().into(),
      path_of(db, worker).await?.into(),
      model.into(),
      (!account.is_empty()).then(|| account.to_owned()).into(),
    ],
  )
  .await?;
  let Some(row) = row else { return Ok(None) };
  let last: i64 = row.try_get("", "answered_at")?;

  if let Some(ttl) = ttl_ms
    && now_ms() - last > ttl
  {
    forget(db, worker, model, Some(account)).await?;

    return Ok(None);
  }

  Ok(Some(row.try_get("", "session")?))
}

/// The session's record gone: one login's, or every login's on the model.
pub async fn forget(
  db: &impl ConnectionTrait,
  worker: &str,
  model: &str,
  account: Option<&str>,
) -> Result<(), Refusal> {
  let path = path_of(db, worker).await?;

  match account {
    None => {
      run(
        db,
        r#"UPDATE "role_sessions" SET "dropped" = 1, "held" = 0 WHERE "role" = $1 AND "path" = $2 AND "model" = $3"#,
        vec![job_role().into(), path.into(), model.into()],
      )
      .await?
    }
    Some(account) => {
      run(
        db,
        r#"UPDATE "role_sessions" SET "dropped" = 1, "held" = 0
           WHERE "role" = $1 AND "path" = $2 AND "model" = $3 AND "account" IS NOT DISTINCT FROM $4"#,
        vec![
          job_role().into(),
          path.into(),
          model.into(),
          (!account.is_empty()).then(|| account.to_owned()).into(),
        ],
      )
      .await?
    }
  };

  Ok(())
}

/// The session that answered, or the one a fold left, kept as the one the next job at the checkout on the
/// model resumes, its answer moved to now.
pub async fn record(
  db: &impl ConnectionTrait,
  worker: &str,
  model: &str,
  account: &str,
  session_id: &str,
  _prefix_digest: Option<&str>,
  _answered_model: Option<&str>,
) -> Result<(), Refusal> {
  let path = path_of(db, worker).await?;
  let account = (!account.is_empty()).then(|| account.to_owned());
  let now = now_ms();
  let resumed = run(
    db,
    r#"UPDATE "role_sessions" SET "answered_at" = $3, "runs" = "runs" + 1, "held" = 0
       WHERE "session" = $1 AND "role" = $2 AND "dropped" = 0"#,
    vec![session_id.into(), job_role().into(), now.into()],
  )
  .await?;

  if resumed > 0 {
    return Ok(());
  }

  // A checkout keeps one session a model and login: the one before it on them is dropped.
  run(
    db,
    r#"UPDATE "role_sessions" SET "dropped" = 1
       WHERE "role" = $1 AND "path" = $2 AND "model" = $3 AND "account" IS NOT DISTINCT FROM $4"#,
    vec![job_role().into(), path.clone().into(), model.into(), account.clone().into()],
  )
  .await?;
  run(
    db,
    r#"INSERT INTO "role_sessions" ("role", "path", "provider", "model", "account", "command_key", "session",
         "opened_at", "answered_at", "runs")
       VALUES ($1, $2, $3, $4, $5, '', $6, $7, $7, 1)"#,
    vec![
      job_role().into(),
      path.into(),
      PROVIDER.into(),
      model.into(),
      account.into(),
      session_id.into(),
      now.into(),
    ],
  )
  .await?;

  Ok(())
}

/// The model a turn on a kept session goes out on: the one it fell to, where the repository's heal is off.
pub async fn launch_model(
  db: &impl ConnectionTrait,
  worker: &str,
  fallen: Option<&str>,
  model: String,
) -> Result<String, Refusal> {
  let Some(fallen) = fallen else { return Ok(model) };

  Ok(if heals(db, worker).await? { model } else { fallen.to_owned() })
}

// Whether a session the safeguard refused is dropped, which the checkout's repository says: on unless the
// owner turned it off.
async fn heals(db: &impl ConnectionTrait, worker: &str) -> Result<bool, Refusal> {
  let base = worker.split_once('@').map_or(worker, |(base, _)| base);
  let base = base
    .rsplit_once("-w")
    .filter(|(_, slot)| slot.parse::<i64>().is_ok())
    .map_or(base, |(base, _)| base);

  Ok(
    one(
      db,
      r#"SELECT "downgrade_heal" FROM "repositories" WHERE "name" = $1"#,
      vec![base.into()],
    )
    .await?
    .map(|row| row.try_get::<i64>("", "downgrade_heal"))
    .transpose()?
    .is_none_or(|heal| heal != 0),
  )
}

fn family(model: &str) -> &str {
  ClaudeCode::family(model)
}

/// The models a run answered on other than the one it was launched on, within its family: the account's
/// safeguard refusing the launched model on the session's carried context. A launch that named its model
/// in short is read as no downgrade, since which full name the CLI made of it is not known here.
pub fn downgrades(spend: &[ModelSpend], launched: &str) -> Vec<String> {
  if !launched.starts_with("claude-") {
    return vec![];
  }

  spend
    .iter()
    .filter(|one| one.tokens > 0 && one.model != launched && family(&one.model) == family(launched))
    .map(|one| one.model.clone())
    .collect()
}

/// What a downgrade came to: the lesser models, and whether the session's record was dropped.
pub struct Downgrade {
  pub models: Vec<String>,
  pub dropped: bool,
}

/// The heal of a run that came back on a lesser model: the refused session's record dropped, unless the
/// owner turned the heal off for this worker, and `worker-downgraded` published with which worker fell to
/// which model, and on which job where a job's run fell. Telling the owner is the consumer's.
#[allow(clippy::too_many_arguments)]
pub async fn heal(
  ground: &Ground,
  worker: &str,
  model: &str,
  launched: &str,
  account: &str,
  spend: &[ModelSpend],
  job: Option<i64>,
) -> Result<Downgrade, Refusal> {
  let models = downgrades(spend, launched);

  if models.is_empty() {
    return Ok(Downgrade {
      models,
      dropped: false,
    });
  }

  let dropped = heals(&ground.db, worker).await?;

  if dropped {
    forget(&ground.db, worker, model, Some(account)).await?;
  }

  println!(
    "downgrade {worker} {launched}: answered on {}, session {}",
    models.join(", "),
    if dropped { "dropped" } else { "kept, the heal is off for this worker" }
  );

  let repository = crate::queue::workers::repository_of(&ground.db, worker)
    .await
    .ok()
    .flatten()
    .map(|held| held.name);

  ground.events.publish(WorkerDowngraded {
    worker: worker.to_owned(),
    repository,
    role: Some(job_role().to_owned()),
    launched: launched.to_owned(),
    answered_on: models.clone(),
    dropped,
    job,
  });

  Ok(Downgrade { models, dropped })
}

/// The heal of a job's run that came back on a lesser model than it went out on (docs/cache-fallback.md,
/// 「Healing differs for a job」): the session it kept for its role at the checkout dropped, unless the owner
/// turned the heal off for the checkout's repository, and `worker.changed` published with the checkout, the
/// role, the model it went out on, the models it answered on and the job. Telling the owner is the consumer's.
pub async fn heal_role_run(
  ground: &Ground,
  checkout: &str,
  role: &str,
  launched: &str,
  reply: &Reply,
  job: Option<i64>,
) -> Result<(), Refusal> {
  let models = downgrades(&reply.spend, launched);

  if models.is_empty() {
    return Ok(());
  }

  let dropped = heals(&ground.db, checkout).await?;

  if dropped && let Some(session) = &reply.session_id {
    crate::apart_sessions::hand_over(&ground.db, session).await?;
  }

  println!(
    "downgrade {checkout} {role} {launched}: answered on {}, session {}",
    models.join(", "),
    if dropped { "dropped" } else { "kept, the heal is off for this repository" }
  );

  let repository = crate::queue::workers::repository_of(&ground.db, checkout)
    .await
    .ok()
    .flatten()
    .map(|held| held.name);

  ground.events.publish(WorkerDowngraded {
    worker: checkout.to_owned(),
    repository,
    role: Some(role.to_owned()),
    launched: launched.to_owned(),
    answered_on: models,
    dropped,
    job,
  });

  Ok(())
}

/// What a kept session is marked with once a turn on it answered: the model it fell to now, or the
/// fallen model it was launched on, which it still answers on.
pub fn fallen_model(model: &str, launched: &str, fell_to: &[String]) -> Option<String> {
  fell_to.first().cloned().or_else(|| (launched != model).then(|| launched.to_owned()))
}

/// A supervisor's last answer: the provider, model and effort it answered on. A run of a role keeps none on
/// any row; its session's row says what it went out on.
pub async fn note_answer(
  db: &impl ConnectionTrait,
  agent: &str,
  mode: &str,
  model: Option<&str>,
  effort: Option<&str>,
) -> Result<(), Refusal> {
  run(
    db,
    r#"UPDATE "supervisors" SET "last_mode" = $1, "last_model" = $2, "last_effort" = $3 WHERE "name" = $4"#,
    vec![mode.into(), model.into(), effort.into(), agent.into()],
  )
  .await?;

  Ok(())
}

/// A ping into a supervisor's program counted as its keepalive, failure included.
pub async fn note_keepalive(db: &impl ConnectionTrait, agent: &str) -> Result<(), Refusal> {
  let now = now_ms() / 1000;

  run(
    db,
    r#"UPDATE "supervisors" SET "last_run_at" = $1, "last_keepalive_at" = $1, "runs" = "runs" + 1 WHERE "name" = $2"#,
    vec![now.into(), agent.into()],
  )
  .await?;

  Ok(())
}

/// What one run spent published under a name, on the login it went out on: every model it spent on with
/// tokens, and nothing for a run that reported nothing.
pub fn note_spend(events: &Bus, agent: &str, spend: &[ModelSpend], account: &str) {
  if let Some(spent) = UsageSpent::of(agent, None, now_ms() / 1000, spend, |_| account.to_owned()) {
    events.publish(spent);
  }
}

/// What a turn's answer says about usage: the agent's last answer written, the windows the answer read
/// kept under the login that answered, and what it spent published.
pub async fn note_usage(
  db: &impl ConnectionTrait,
  windows: &Keeping,
  events: &Bus,
  agent: &str,
  reply: &Reply,
) -> Result<(), Refusal> {
  let at = now_ms() / 1000;
  let account = reply.account.clone().unwrap_or_else(|| SEED_ACCOUNT.to_owned());

  note_answer(db, agent, &reply.mode, reply.model.as_deref(), reply.effort.as_deref()).await?;
  crate::usage_windows::keep_reply(windows, reply, at).await;

  note_spend(events, agent, &reply.spend, &account);

  Ok(())
}

/// A turn of a supervisor counted.
pub async fn note_run(db: &impl ConnectionTrait, agent: &str) -> Result<(), Refusal> {
  run(
    db,
    r#"UPDATE "supervisors" SET "last_run_at" = $1, "runs" = "runs" + 1 WHERE "name" = $2"#,
    vec![(now_ms() / 1000).into(), agent.into()],
  )
  .await?;

  Ok(())
}

/// The login a provider's reading is filed under: the one that answered, where the provider holds logins
/// of agency's, and the empty account on any other road.
pub fn slot_account(mode: &str, account: Option<&str>) -> String {
  if harness_providers::provider(mode).is_some_and(|provider| provider.holds_logins()) {
    account.filter(|account| !account.is_empty()).unwrap_or(SEED_ACCOUNT).to_owned()
  } else {
    String::new()
  }
}

/// What one run apart reported it spent, published under the agent's own name and the run log it kept,
/// each row on its provider's slot: the login that carried it where the provider holds logins of agency's.
pub fn note_run_spend(
  events: &Bus,
  agent: &str,
  spend: &[ModelSpend],
  run_log: Option<&str>,
  account: Option<&str>,
) {
  if let Some(spent) =
    UsageSpent::of(agent, run_log, now_ms() / 1000, spend, |one| slot_account(&one.mode, account))
  {
    events.publish(spent);
  }
}

/// What a job's turn says about usage under the job's run: the agent's last answer written, the windows
/// the answer read kept under the login that answered, and what it spent published, each row on its
/// provider's slot.
pub async fn note_job_usage(
  db: &impl ConnectionTrait,
  windows: &Keeping,
  events: &Bus,
  agent: &str,
  reply: &Reply,
  logged_as: Option<&str>,
) -> Result<(), Refusal> {
  let at = now_ms() / 1000;
  let account = reply.account.as_deref();

  note_answer(db, agent, &reply.mode, reply.model.as_deref(), reply.effort.as_deref()).await?;
  crate::usage_windows::keep_reply(windows, reply, at).await;

  // A row its program named no provider for is the turn's own provider's.
  let spend: Vec<ModelSpend> = reply
    .spend
    .iter()
    .map(|one| ModelSpend {
      mode: if one.mode.is_empty() { reply.mode.clone() } else { one.mode.clone() },
      ..one.clone()
    })
    .collect();

  if let Some(spent) = UsageSpent::of(agent, logged_as, at, &spend, |one| slot_account(&one.mode, account))
  {
    events.publish(spent);
  }

  Ok(())
}
