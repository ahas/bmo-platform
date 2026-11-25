//! The sessions agents keep (docs/sessions.md, 「A role's session」): one row a session an agent's run opened,
//! kept by the agent and the path, with the role, the provider, the model and the command's key beside them, taken by
//! a run as it starts so no two runs share it, and written again with the session that answered once the run
//! has answered. A session past its cache lifetime, or whose last turn filled the model's window, is dropped
//! as it is taken, and the run opens a new one.

use std::path::Path;

use policy::roles::Sessions;
use sea_orm::{ConnectionTrait, QueryResult};
use sha2::{Digest, Sha256};
use store::{Fault, one, run};

use crate::agent_roles;
use crate::lines::claude::Command;
use crate::worker_session::warmth::{lifetime_ms, warm};

// The flag a command names the session it opens with, and the one its system prompt's file is handed by.
const OPENS: &str = "--session-id";
const SYSTEM_PROMPT_FILE: &str = "--system-prompt-file";

/// The agent a role's run at `path` is let in on, whose session it keeps (docs/sessions.md, 「A role's session」):
/// the seat its slot holds there; none where it cannot be told.
pub async fn agent_at<C: ConnectionTrait>(db: &C, role: &str, path: &str) -> Option<String> {
  crate::role_agents::seated_at(db, Some(role), Some(path))
    .await
    .ok()
    .flatten()
    .map(|agent| agent.name)
}

/// The sessions setting a role's run opens its session by: its agent's, found by the folder the run works in
/// (docs/role-agents.md, 「What a run reads off its agent」); a run whose agent cannot be told keeps nothing.
pub async fn setting<C: ConnectionTrait>(db: &C, role: &str, path: &str) -> Sessions {
  match crate::role_agents::sessions_at(db, role, Some(path)).await {
    Ok(sessions) => sessions,
    Err(why) => {
      eprintln!(
        "{role}: its agent's sessions setting could not be read, so its run opens a new one: {why:?}"
      );

      Sessions::Fresh
    }
  }
}

/// The key a command stands under beside its role, path, provider and model: the line, the login's
/// configuration home and every word of the command but its session's and its advisor's, the system prompt's
/// text standing for its file, so another effort, login, tool list or prompt is another command, and another
/// advisor is not (docs/sessions.md, 「A role's session」).
pub fn key(line: &str, command: &Command, home: &Path) -> String {
  let mut hash = Sha256::new();
  let mut part = |text: &[u8]| {
    hash.update((text.len() as u64).to_le_bytes());
    hash.update(text);
  };

  part(line.as_bytes());
  part(command.cwd.as_bytes());
  part(home.as_os_str().as_encoded_bytes());

  let kept = crate::lines::claude::without_advisor(&command.args);
  let mut words = kept.into_iter();

  while let Some(word) = words.next() {
    if word == OPENS {
      words.next();
      continue;
    }

    part(word.as_bytes());

    if word == SYSTEM_PROMPT_FILE
      && let Some(file) = words.next()
    {
      part(&std::fs::read(file).unwrap_or_else(|_| file.as_bytes().to_vec()));
    }
  }

  format!("{:x}", hash.finalize())
}

/// The key a run on a road other than Claude Code's own stands under beside its role, path, provider and model:
/// the line, the folder, the text of its whole system prompt, its MCP configuration and the tools it is allowed,
/// so another prompt, tool list or folder is another command.
pub fn key_of(line: &str, cwd: &str, system: &Path, mcp_config: &Path, tools: &str) -> String {
  let mut hash = Sha256::new();
  let mut part = |text: &[u8]| {
    hash.update((text.len() as u64).to_le_bytes());
    hash.update(text);
  };

  part(line.as_bytes());
  part(cwd.as_bytes());
  part(&std::fs::read(system).unwrap_or_else(|_| system.as_os_str().as_encoded_bytes().to_vec()));
  part(mcp_config.as_os_str().as_encoded_bytes());
  part(tools.as_bytes());

  format!("{:x}", hash.finalize())
}

/// What a session is kept by: the agent whose run answered on it, none where the run's agent cannot be told; the
/// role, the path its run works at, the provider and model it went out on, the effort and login, and the
/// command's key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionKey {
  pub agent: Option<String>,
  pub role: String,
  pub path: String,
  pub provider: String,
  pub model: String,
  pub effort: String,
  pub account: Option<String>,
  pub command: String,
}

/// One kept session as its row holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
  pub id: i64,
  pub role: String,
  pub path: String,
  pub provider: String,
  pub model: String,
  pub effort: String,
  pub account: Option<String>,
  pub session: String,
  pub opened_at: i64,
  pub answered_at: Option<i64>,
  pub runs: i64,
  pub context_tokens: Option<i64>,
  pub context_share: Option<f64>,
  pub held: bool,
  pub dropped: bool,
}

impl Kept {
  pub fn of(row: &QueryResult) -> Result<Self, Fault> {
    Ok(Self {
      id: row.try_get("", "id")?,
      role: row.try_get("", "role")?,
      path: row.try_get("", "path")?,
      provider: row.try_get("", "provider")?,
      model: row.try_get("", "model")?,
      effort: row.try_get("", "effort")?,
      account: row.try_get("", "account")?,
      session: row.try_get("", "session")?,
      opened_at: row.try_get("", "opened_at")?,
      answered_at: row.try_get("", "answered_at")?,
      runs: row.try_get("", "runs")?,
      context_tokens: row.try_get("", "context_tokens")?,
      context_share: row.try_get("", "context_share")?,
      held: row.try_get::<i64>("", "held")? != 0,
      dropped: row.try_get::<i64>("", "dropped")? != 0,
    })
  }
}

/// Whether the next run of a kept session's key would resume it at `now`: not dropped, held by no run, its
/// last turn under the model's window, and its cache still warm — or its provider declaring no cache lifetime,
/// which leaves the session nothing to outlive, so a local road's session is resumed however long the wait
/// before it (docs/sessions.md, 「The warm/cold criterion」).
pub fn valid(kept: &Kept, now: i64) -> bool {
  let lifetime = lifetime_ms(&kept.provider, &kept.model);

  !kept.dropped
    && !kept.held
    && kept.context_share.is_none_or(|share| share < 1.0)
    && kept
      .answered_at
      .is_some_and(|answered| lifetime.is_none_or(|lifetime| warm(answered, now, lifetime)))
}

/// The session a key's last answered run left, taken so no other run resumes it meanwhile, where it is still
/// valid at `now`; one no longer valid is dropped all the same, and the run opens a new one.
pub async fn take<C: ConnectionTrait>(db: &C, key: &SessionKey, now: i64) -> Result<Option<String>, Fault> {
  let Some(row) = one(
    db,
    r#"SELECT * FROM "role_sessions"
       WHERE "role" = $1 AND "path" = $2 AND "provider" = $3 AND "model" = $4 AND "command_key" = $5
         AND "agent" IS NOT DISTINCT FROM $6 AND "dropped" = 0 AND "held" = 0 AND "answered_at" IS NOT NULL
       ORDER BY "answered_at" DESC LIMIT 1"#,
    vec![
      key.role.clone().into(),
      key.path.clone().into(),
      key.provider.clone().into(),
      key.model.clone().into(),
      key.command.clone().into(),
      key.agent.clone().into(),
    ],
  )
  .await?
  else {
    return Ok(None);
  };
  let kept = Kept::of(&row)?;

  if !valid(&kept, now) {
    drop_session(db, kept.id).await?;

    return Ok(None);
  }

  let taken = run(
    db,
    r#"UPDATE "role_sessions" SET "held" = 1 WHERE "id" = $1 AND "held" = 0"#,
    vec![kept.id.into()],
  )
  .await?;

  Ok((taken == 1).then_some(kept.session))
}

/// A kept session dropped with its record: no later run resumes it.
pub async fn drop_session<C: ConnectionTrait>(db: &C, id: i64) -> Result<(), Fault> {
  run(
    db,
    r#"UPDATE "role_sessions" SET "dropped" = 1, "held" = 0 WHERE "id" = $1"#,
    vec![id.into()],
  )
  .await?;

  Ok(())
}

/// Whether a run's session is due a fold as the run ends: its last turn left it at or past its role's
/// compaction threshold (docs/sessions.md, 「Session compaction」). A turn that reports no share is not due.
pub fn fold_due(context_share: Option<f64>, fold_ratio: f64) -> bool {
  context_share.is_some_and(|share| share >= fold_ratio)
}

/// The session a run just kept, taken again for its fold where its last turn left it at or past its role's
/// threshold, read afresh from the role's row: the session, held so no other run resumes it while it folds;
/// none where no fold is due, the role is gone or another run took it first.
pub async fn take_for_fold<C: ConnectionTrait>(
  db: &C,
  role: &str,
  session: &str,
  context_share: Option<f64>,
) -> Result<Option<String>, Fault> {
  let Some(role) = agent_roles::named(db, role).await? else {
    return Ok(None);
  };
  // The threshold is the one the agent the run is let in on sets, where it sets one: the run still holds its
  // slot at the session's path while it folds.
  let path = one(
    db,
    r#"SELECT "path" FROM "role_sessions" WHERE "session" = $1 AND "role" = $2 ORDER BY "id" DESC LIMIT 1"#,
    vec![session.into(), role.name.as_str().into()],
  )
  .await?
  .map(|row| row.try_get::<String>("", "path"))
  .transpose()?;
  let own = crate::role_agents::seated_at(db, Some(&role.name), path.as_deref())
    .await?
    .and_then(|agent| agent.fold_ratio);

  // An agent that sets none folds at the ratio the session's provider declares.
  let declared = one(
    db,
    r#"SELECT "provider" FROM "role_sessions" WHERE "session" = $1 AND "role" = $2 ORDER BY "id" DESC LIMIT 1"#,
    vec![session.into(), role.name.as_str().into()],
  )
  .await?
  .map(|row| row.try_get::<String>("", "provider"))
  .transpose()?
  .and_then(|provider| harness_providers::provider(&provider).and_then(|held| held.declared().fold_ratio))
  .unwrap_or(crate::order::DEFAULT_FOLD_RATIO);

  if !fold_due(context_share, own.unwrap_or(declared)) {
    return Ok(None);
  }

  let taken = run(
    db,
    r#"UPDATE "role_sessions" SET "held" = 1
       WHERE "session" = $1 AND "role" = $2 AND "dropped" = 0 AND "held" = 0"#,
    vec![session.into(), role.name.into()],
  )
  .await?;

  Ok((taken == 1).then(|| session.to_owned()))
}

/// A session whose fold went through given back: answered at `at`, its context unread until its next run
/// answers on it, and valid for that run.
pub async fn folded<C: ConnectionTrait>(db: &C, session: &str, at: i64) -> Result<(), Fault> {
  run(
    db,
    r#"UPDATE "role_sessions" SET "held" = 0, "answered_at" = $2, "context_tokens" = NULL,
         "context_share" = NULL
       WHERE "session" = $1 AND "dropped" = 0"#,
    vec![session.into(), at.into()],
  )
  .await?;

  Ok(())
}

/// The session a key's run went out on handed over: closed with its record, so the next run of its key opens a
/// new one (docs/sessions.md, 「A role's session」).
pub async fn hand_over<C: ConnectionTrait>(db: &C, session: &str) -> Result<u64, Fault> {
  run(
    db,
    r#"UPDATE "role_sessions" SET "dropped" = 1, "held" = 0 WHERE "session" = $1 AND "dropped" = 0"#,
    vec![session.into()],
  )
  .await
}

/// What a run that answered leaves of its session: the context its last turn carried, where it is read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Answered {
  pub at: i64,
  pub opened_at: i64,
  pub context_tokens: Option<i64>,
  pub context_share: Option<f64>,
}

/// The session a run answered on, kept for the next run of its key: the row it resumed given back with one
/// more run, or a row of its own where it opened a new session.
pub async fn keep<C: ConnectionTrait>(
  db: &C,
  key: &SessionKey,
  session: &str,
  answered: Answered,
) -> Result<(), Fault> {
  let resumed = run(
    db,
    r#"UPDATE "role_sessions" SET "held" = 0, "answered_at" = $2, "runs" = "runs" + 1, "context_tokens" = $3,
         "context_share" = $4
       WHERE "session" = $1 AND "role" = $5 AND "dropped" = 0"#,
    vec![
      session.into(),
      answered.at.into(),
      answered.context_tokens.into(),
      answered.context_share.into(),
      key.role.clone().into(),
    ],
  )
  .await?;

  if resumed > 0 {
    return Ok(());
  }

  run(
    db,
    r#"INSERT INTO "role_sessions" ("role", "path", "provider", "model", "effort", "account", "command_key",
         "session", "opened_at", "answered_at", "runs", "context_tokens", "context_share", "agent")
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 1, $11, $12, $13)"#,
    vec![
      key.role.clone().into(),
      key.path.clone().into(),
      key.provider.clone().into(),
      key.model.clone().into(),
      key.effort.clone().into(),
      key.account.clone().into(),
      key.command.clone().into(),
      session.into(),
      answered.opened_at.into(),
      answered.at.into(),
      answered.context_tokens.into(),
      answered.context_share.into(),
      key.agent.clone().into(),
    ],
  )
  .await?;

  Ok(())
}

/// The kept session of this id as its row holds it, dropped or not; none where no row holds it.
pub async fn kept<C: ConnectionTrait>(db: &C, session: &str) -> Result<Option<Kept>, Fault> {
  one(
    db,
    r#"SELECT * FROM "role_sessions" WHERE "session" = $1 ORDER BY "id" DESC LIMIT 1"#,
    vec![session.into()],
  )
  .await?
  .as_ref()
  .map(Kept::of)
  .transpose()
}

/// Whether a kept session is still the one its key's next run would look to: the last answered, not dropped,
/// of its role at its path on its provider and model. A later run that kept another there leaves this one
/// behind.
pub async fn newest<C: ConnectionTrait>(db: &C, kept: &Kept) -> Result<bool, Fault> {
  let last = one(
    db,
    r#"SELECT "session" FROM "role_sessions"
       WHERE "role" = $1 AND "path" = $2 AND "provider" = $3 AND "model" = $4 AND "dropped" = 0
         AND "answered_at" IS NOT NULL
       ORDER BY "answered_at" DESC, "id" DESC LIMIT 1"#,
    vec![
      kept.role.clone().into(),
      kept.path.clone().into(),
      kept.provider.clone().into(),
      kept.model.clone().into(),
    ],
  )
  .await?
  .map(|row| row.try_get::<String>("", "session"))
  .transpose()?;

  Ok(last.as_deref() == Some(kept.session.as_str()))
}

/// A kept session taken for its warm ping (docs/keepalive-schedule.md, 「An agent's kept session」), held so
/// no run resumes it while the ping goes; whether it was taken — not where a run or a fold holds it, or it
/// was dropped.
pub async fn take_for_ping<C: ConnectionTrait>(db: &C, session: &str) -> Result<bool, Fault> {
  let taken = run(
    db,
    r#"UPDATE "role_sessions" SET "held" = 1 WHERE "session" = $1 AND "dropped" = 0 AND "held" = 0"#,
    vec![session.into()],
  )
  .await?;

  Ok(taken == 1)
}

/// A kept session given back after a ping that answered: its last answer is the ping's, so it stands valid
/// from that moment, and its count of runs is as it was.
pub async fn warmed<C: ConnectionTrait>(db: &C, session: &str, at: i64) -> Result<(), Fault> {
  run(
    db,
    r#"UPDATE "role_sessions" SET "held" = 0, "answered_at" = $2 WHERE "session" = $1"#,
    vec![session.into(), at.into()],
  )
  .await?;

  Ok(())
}

/// A session a run took and did not answer on given back unchanged, so the next run of its key may resume
/// it while it is still valid.
pub async fn give_back<C: ConnectionTrait>(db: &C, session: &str) -> Result<(), Fault> {
  run(
    db,
    r#"UPDATE "role_sessions" SET "held" = 0 WHERE "session" = $1"#,
    vec![session.into()],
  )
  .await?;

  Ok(())
}

/// The session kept for a role at a path on a provider and model handed over: closed with its record, so the next
/// run of that role there opens a new one.
pub async fn hand_over_key<C: ConnectionTrait>(
  db: &C,
  role: &str,
  path: &str,
  provider: &str,
  model: &str,
) -> Result<u64, Fault> {
  run(
    db,
    r#"UPDATE "role_sessions" SET "dropped" = 1, "held" = 0
       WHERE "role" = $1 AND "path" = $2 AND "provider" = $3 AND "model" = $4 AND "dropped" = 0"#,
    vec![role.into(), path.into(), provider.into(), model.into()],
  )
  .await
}

/// Every session kept at a path handed over, whatever its role, provider and model: a reset of the checkout's
/// sessions leaves the next run there a new one.
pub async fn hand_over_path<C: ConnectionTrait>(db: &C, path: &str) -> Result<u64, Fault> {
  run(
    db,
    r#"UPDATE "role_sessions" SET "dropped" = 1, "held" = 0 WHERE "path" = $1 AND "dropped" = 0"#,
    vec![path.into()],
  )
  .await
}
