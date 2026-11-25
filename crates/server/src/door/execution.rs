//! A resident turn's execution row (docs/agent-control.md, 「Completion, cancellation and loss」): the
//! row it is in the archive's `agent_executions`, the envelope its collector keeps under the executions
//! root, the cancellation that ends it, and the rows of the child runs its program opens. The row's body
//! is one shape, field for field, since every reader of the row reads it: the reconciler, the web console
//! and the TypeScript execution store (`archive/archive/execution-store.ts`) among them.

use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_host::identity::process_birth;
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::record::ChildEvidence;
use crate::queue::running::EXECUTION_CANCEL_PREFIX;
use crate::queue::settings;
use store::{all, one, run};

use crate::{Refusal, now_ms};

/// The envelope's file, beside the supervised path's under the same executions root.
pub const ENVELOPE: &str = "collector.json";

/// Who the turn's row says it is: the agent it is for, the work it belongs to, the work key it runs for and
/// what it runs on, set by the caller over what the run's name and the command name.
#[derive(Clone, Debug, Default)]
pub struct Identity {
  /// The agent whose program the turn goes into (docs/agent-control.md, 「Identity and launch
  /// handshake」): a standing seat's own where the caller names none, and none for a pane of the turn's
  /// own.
  pub agent: Option<String>,
  /// The root its executions are counted under and the run it is a child of, where the caller names
  /// them rather than the run's own name.
  pub root: Option<String>,
  pub parent: Option<String>,
  /// The role the run holds, where it is a role's.
  pub role: Option<String>,
  pub model: Option<String>,
  pub effort: Option<String>,
}

/// What this process is called, as a run it opens is counted under: the name its environment hands it, or
/// the unit its cgroup names, or nothing.
pub fn own_unit(env: &std::collections::BTreeMap<String, String>) -> Option<String> {
  if let Some(named) = env.get("BMO_RUN_NAME").filter(|named| !named.is_empty()) {
    return Some(named.clone());
  }

  std::fs::read_to_string("/proc/self/cgroup").ok()?.lines().find_map(|line| {
    let (_, unit) = line.trim().strip_suffix(".service")?.rsplit_once('/')?;

    (!unit.is_empty()).then(|| unit.to_owned())
  })
}

/// The root a run's executions are counted under: a task unit's own name, and a root of its own for anything
/// else.
pub fn execution_root(owner: &str) -> String {
  if owner.starts_with("bmo-task-") {
    return owner.to_owned();
  }

  format!("bmo-root-{}", uuid::Uuid::new_v4())
}

/// Whether a standing cancellation covers any of these names.
pub async fn cancelled(db: &DatabaseConnection, names: &[&str]) -> Result<bool, Refusal> {
  for name in names.iter().filter(|name| !name.is_empty()) {
    if settings::get(db, &format!("{EXECUTION_CANCEL_PREFIX}{name}")).await?.is_some() {
      return Ok(true);
    }
  }

  Ok(false)
}

// How often a removal looks again whether the rows it waits on have closed.
const OPEN_POLL: Duration = Duration::from_millis(500);

/// Every row naming this agent as the one it is for taken, once the agent itself is gone
/// (docs/supervisors.md, 「Adding and removing a supervisor」), with each row's envelope directory
/// and the attempt count of a root no row holds any more. A row still open is waited on until it closes,
/// however long that takes, since its collector writes it as it ends and would write back a row taken
/// before then. Answers what of the disk could not be removed.
pub async fn forget_agent(
  db: &DatabaseConnection,
  data_root: &Path,
  agent: &str,
) -> Result<Vec<String>, Refusal> {
  loop {
    let open = one(
      db,
      r#"SELECT count(*) AS "open" FROM "agent_executions"
         WHERE ("body"::jsonb)->>'agent' = $1 AND ("body"::jsonb)->>'endedAt' IS NULL"#,
      vec![agent.into()],
    )
    .await?
    .and_then(|row| row.try_get::<i64>("", "open").ok())
    .unwrap_or(0);

    if open == 0 {
      break;
    }

    tokio::time::sleep(OPEN_POLL).await;
  }

  let taken = all(
    db,
    r#"DELETE FROM "agent_executions" WHERE ("body"::jsonb)->>'agent' = $1
       RETURNING "id", ("body"::jsonb)->>'root' AS "root""#,
    vec![agent.into()],
  )
  .await?;
  let mut roots = std::collections::BTreeSet::new();
  let mut left = vec![];

  for row in &taken {
    if let Ok(Some(root)) = row.try_get::<Option<String>>("", "root") {
      roots.insert(root);
    }

    let Ok(id) = row.try_get::<String>("", "id") else { continue };

    if !agent_host::disk::plain_name(&id) {
      continue;
    }

    let directory = data_root.join("executions").join(&id);

    if let Err(why) = std::fs::remove_dir_all(&directory)
      && why.kind() != std::io::ErrorKind::NotFound
    {
      left.push(format!("{}: {why}", directory.display()));
    }
  }

  for root in roots {
    run(
      db,
      r#"DELETE FROM "execution_attempts" WHERE "root" = $1
         AND NOT EXISTS (SELECT 1 FROM "agent_executions" WHERE ("body"::jsonb)->>'root' = $1)"#,
      vec![root.into()],
    )
    .await?;
  }

  Ok(left)
}

/// One execution row, its body as both sides write it.
#[derive(Clone, Debug)]
pub struct Row {
  pub body: Map<String, Value>,
}

impl Row {
  pub fn id(&self) -> &str {
    self.body.get("id").and_then(Value::as_str).unwrap_or("")
  }

  pub fn root(&self) -> &str {
    self.body.get("root").and_then(Value::as_str).unwrap_or("")
  }

  pub fn text(&self, key: &str) -> Option<&str> {
    self.body.get(key).and_then(Value::as_str)
  }

  pub fn set(&mut self, key: &str, value: Value) {
    self.body.insert(key.to_owned(), value);
  }

  pub fn ended(&self) -> bool {
    self.body.get("endedAt").is_some_and(|ended| !ended.is_null())
  }

  /// A pane turn's row begun: its attempt counted under its root and the row written in one
  /// transaction.
  pub async fn begin(
    db: &DatabaseConnection,
    provider: &str,
    line: &str,
    name: &str,
    session: Option<&str>,
    command: &[String],
    identity: &Identity,
  ) -> Result<Self, Refusal> {
    let flag = |flag: &str| {
      command
        .iter()
        .position(|word| word == flag)
        .and_then(|at| command.get(at + 1))
        .map_or(Value::Null, |value| json!(value))
    };
    // What the identity names, read off the command where it names nothing; an empty name says the run has
    // none.
    let pick = |given: &Option<String>, named: Value| {
      given
        .as_ref()
        .map_or(named, |given| if given.is_empty() { Value::Null } else { json!(given) })
    };
    let mut body = Map::new();

    body.insert("id".into(), json!(format!("bmo-execution-{}", uuid::Uuid::new_v4())));
    body.insert("agent".into(), json!(identity.agent));
    body.insert("root".into(), json!(identity.root.clone().unwrap_or_else(|| execution_root(name))));
    body.insert("parent".into(), json!(identity.parent.as_deref().unwrap_or(name)));
    body.insert("role".into(), json!(identity.role));
    body.insert("model".into(), pick(&identity.model, flag("--model")));
    body.insert("effort".into(), pick(&identity.effort, flag("--effort")));
    body.insert("sessionId".into(), json!(session));
    body.insert("provider".into(), json!(provider));
    body.insert("line".into(), json!(line));
    body.insert("paneId".into(), Value::Null);
    body.insert("state".into(), json!("launching"));
    body.insert("startedAt".into(), json!(now_ms()));
    body.insert("endedAt".into(), Value::Null);
    body.insert("exitCode".into(), Value::Null);
    // The pane is the turn's own process, whatever else the caller said about who it is; written by
    // the insert, so a collector killed before its first save still leaves a row that says which
    // evidence ends it.
    body.insert("panePath".into(), json!(true));

    Self::open(db, body).await
  }

  /// A row begun from its first body: its attempt counted under its root and the row written in one
  /// transaction.
  pub async fn open(db: &DatabaseConnection, body: Map<String, Value>) -> Result<Self, Refusal> {
    let mut row = Self { body };
    let transaction = db.begin().await?;
    let counted = one(
      &transaction,
      r#"INSERT INTO "execution_attempts" ("root", "attempt") VALUES ($1, 1)
         ON CONFLICT ("root") DO UPDATE SET "attempt" = "execution_attempts"."attempt" + 1 RETURNING "attempt""#,
      vec![row.root().into()],
    )
    .await?
    .and_then(|counted| counted.try_get::<i64>("", "attempt").ok())
    .unwrap_or(1);

    row.set("attempt", json!(counted));
    run(
      &transaction,
      r#"INSERT INTO "agent_executions" ("id", "body") VALUES ($1, $2)"#,
      vec![row.id().into(), Value::Object(row.body.clone()).to_string().into()],
    )
    .await?;
    transaction.commit().await?;

    Ok(row)
  }

  /// The row written as it stands now. The keys that say whose row it is never change once written.
  pub async fn save(&self, db: &impl ConnectionTrait) -> Result<(), Refusal> {
    let held = one(db, r#"SELECT "body" FROM "agent_executions" WHERE "id" = $1"#, vec![self.id().into()])
      .await?
      .and_then(|held| held.try_get::<String>("", "body").ok())
      .and_then(|body| serde_json::from_str::<Map<String, Value>>(&body).ok());

    if let Some(previous) = held {
      for key in ["agent", "root", "parent", "attempt", "provider", "role", "nativeId", "effort"] {
        if previous.get(key) != self.body.get(key) {
          return Err(crate::lines::fault(format!("execution identity cannot change: {key}")));
        }
      }
    }

    run(
      db,
      r#"INSERT INTO "agent_executions" ("id", "body") VALUES ($1, $2)
         ON CONFLICT ("id") DO UPDATE SET "body" = EXCLUDED."body""#,
      vec![self.id().into(), Value::Object(self.body.clone()).to_string().into()],
    )
    .await?;

    Ok(())
  }

  /// The row written as it stands now only while the stored one is still open, so an end the run wrote
  /// itself in the meantime stands; whether it was written.
  pub async fn save_while_open(&self, db: &impl ConnectionTrait) -> Result<bool, Refusal> {
    let written = run(
      db,
      r#"UPDATE "agent_executions" SET "body" = $2 WHERE "id" = $1 AND ("body"::jsonb)->>'endedAt' IS NULL"#,
      vec![self.id().into(), Value::Object(self.body.clone()).to_string().into()],
    )
    .await?;

    Ok(written > 0)
  }
}

/// The collector's envelope: written when the turn opens with the identity of the collector that owns
/// the row, and again when it ends with how it ended. A collector cannot write anything at the moment it
/// is killed, so what a killed one leaves behind has to have been written before it died. Written whole
/// or not at all; a machine that cannot write it still runs the turn.
pub fn publish_envelope(data_root: &Path, id: &str, cause: Option<&str>) {
  let pid = std::process::id() as i32;
  let mut value = json!({ "owner": { "pid": pid, "birth": process_birth(pid) } });

  if let Some(cause) = cause {
    value["cause"] = json!(cause);
  }

  let directory: PathBuf = data_root.join("executions").join(id);
  let written = agent_host::disk::private_dir(&directory).and_then(|()| {
    agent_host::disk::publish_bytes(&directory.join(ENVELOPE), value.to_string().as_bytes())
  });

  if let Err(why) = written {
    eprintln!("pane execution envelope: {why}");
  }
}

/// The rows of the child runs a turn's program opens, out of its own evidence alone: a tool finishing
/// is not evidence that its child finished.
#[derive(Debug, Default)]
pub struct Children {
  rows: indexmap::IndexMap<String, Row>,
}

impl Children {
  pub fn known(&self, id: &str) -> bool {
    self.rows.contains_key(id)
  }

  /// One piece of evidence applied to its child's row, the row written where it moved.
  pub async fn update(
    &mut self,
    db: &DatabaseConnection,
    parent: &Row,
    evidence: &ChildEvidence,
  ) -> Result<(), Refusal> {
    let mut child = self
      .rows
      .get(&evidence.id)
      .cloned()
      .unwrap_or_else(|| native_child(parent, &evidence.id));

    if child.ended() {
      return Ok(());
    }

    if evidence.state == "requested"
      && child.text("nativeState").is_some_and(|state| !state.is_empty() && state != "requested")
    {
      return Ok(());
    }

    child.set("nativeState", json!(evidence.state));
    child.set("nativeEvidence", json!(evidence.evidence));
    child.set(
      "state",
      json!(match evidence.state {
        "requested" => "launching",
        "started" => "running",
        "completed" | "failed" => "exited",
        _ => "unknown",
      }),
    );

    if matches!(evidence.state, "completed" | "failed") {
      child.set("endedAt", json!(now_ms()));
      child.set("exitCode", json!(i32::from(evidence.state != "completed")));
    }

    child.save(db).await?;
    self.rows.insert(evidence.id.clone(), child);

    Ok(())
  }

  /// Every child still open closed as unresolved: its collector ended without terminal evidence.
  pub async fn finish(&mut self, db: &DatabaseConnection, parent: &Row) -> Result<(), Refusal> {
    let open: Vec<String> =
      self.rows.iter().filter(|(_, child)| !child.ended()).map(|(id, _)| id.clone()).collect();

    for id in open {
      let evidence = ChildEvidence {
        id,
        state: "unresolved",
        evidence: "collector ended without terminal evidence".to_owned(),
      };

      self.update(db, parent, &evidence).await?;
    }

    Ok(())
  }
}

/// A child's row: its parent's, under an id of its own made of the parent's and the provider's word for
/// the child, which is opaque data and never a path. A child of a pane turn has no pane of its own to be
/// absent from, so the evidence that ends it is its parent's ending and never a pane's.
pub fn native_child(parent: &Row, native: &str) -> Row {
  let mut body = parent.body.clone();
  let named = json!([parent.id(), parent.text("provider"), native]).to_string();
  let digest = format!("{:x}", Sha256::digest(named.as_bytes()));

  body.insert("id".into(), json!(format!("bmo-native-{digest}")));
  body.insert("parent".into(), json!(parent.id()));
  body.insert("nativeId".into(), json!(native));

  for key in ["sessionId", "model", "effort"] {
    body.insert(key.into(), Value::Null);
  }

  body.insert("line".into(), json!("native-child"));
  body.insert("sharedPane".into(), json!(true));
  body.remove("panePath");
  body.insert("startedAt".into(), json!(now_ms()));
  body.insert("endedAt".into(), Value::Null);
  body.insert("exitCode".into(), Value::Null);

  Row { body }
}
