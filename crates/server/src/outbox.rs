//! Agency's outbox (docs/agent-control.md, 「An item's change is told to its orderer」): every notice owed to an
//! orderer is written as a row of the store's `notices` table before it is told, and sent until the
//! receiver takes it — tried as it is written, then by the sender every few seconds, the wait between tries
//! doubling. A row told is let go, kept with its instruction's id until what it tells of moves on, or kept
//! standing and told again while its condition holds; a kept or standing row let go withdraws the copy told last.
//! Which notices stand, and when each condition ends, is the business of the module that owes it.

use std::collections::BTreeMap;
use std::time::Duration;

use policy::job_notices::JOB_NOTICES;
use reqwest::StatusCode;
use sea_orm::{DatabaseConnection, QueryResult};
use serde_json::{Value, json};
use store::{Fault, all, one, run};

use crate::job_end::{address, token};

/// The receiver as the notices reach it: its address and token.
#[derive(Clone, Debug)]
pub struct Receiver {
  url: String,
  token: String,
}

impl Receiver {
  /// The receiver the deployment names; none where it names none, or the ground is a test's.
  pub fn of(vars: &BTreeMap<String, String>) -> Option<Self> {
    let (Some(url), Some(token)) = (
      address(vars, "BMO_RECEIVER_URL"),
      token(vars, "BMO_RECEIVER_TOKEN", "BMO_RECEIVER_TOKEN_FILE"),
    ) else {
      return None;
    };

    Some(Self { url, token })
  }

  /// A line the receiver keeps in the agent's owner's private room, in its record alone (the receiver's
  /// `docs/api.md`, 「Kept lines」): one try, and why it was not kept where it was not.
  pub async fn keep_line(&self, agent: &str, line: &Value) -> Result<(), String> {
    let client = reqwest::Client::builder()
      .timeout(Duration::from_secs(JOB_NOTICES.answer_seconds))
      .build()
      .map_err(|why| why.to_string())?;
    let answer = client
      .post(format!("{}/lines/{agent}", self.url))
      .bearer_auth(&self.token)
      .json(&json!({ "line": line }))
      .send()
      .await
      .map_err(|why| why.to_string())?;
    let status = answer.status();

    if status.is_success() {
      Ok(())
    } else {
      Err(format!("the receiver answered {} to a kept line", status.as_u16()))
    }
  }

  // One try at handing the receiver a notice by its route.
  async fn hand(&self, route: Route, body: &Value) -> Answer {
    let client = match reqwest::Client::builder()
      .timeout(Duration::from_secs(JOB_NOTICES.answer_seconds))
      .build()
    {
      Ok(client) => client,
      Err(why) => return Answer::Failed(why.to_string()),
    };
    let asked = match route {
      Route::Instruction => client.post(format!("{}/instructions", self.url)).json(body),
      Route::Withdrawal => client.delete(format!("{}/instructions/{}", self.url, body["id"])),
    };
    let answer = match asked.bearer_auth(&self.token).send().await {
      Ok(answer) => answer,
      Err(why) => return Answer::Failed(why.to_string()),
    };
    let status = answer.status();

    // An instruction already gone needs no withdrawing.
    if route == Route::Withdrawal && status == StatusCode::NOT_FOUND {
      return Answer::Taken(None);
    }

    if status.is_success() {
      let id = answer.json::<Value>().await.ok().and_then(|taken| taken["id"].as_i64());

      return match (route, id) {
        (Route::Instruction, None) => {
          Answer::Failed("the receiver took an instruction and answered no id for it".to_owned())
        }
        _ => Answer::Taken(id),
      };
    }

    let why = format!("the receiver answered {} to {}", status.as_u16(), route.word());

    if status.is_server_error()
      || matches!(
        status,
        StatusCode::UNAUTHORIZED
          | StatusCode::FORBIDDEN
          | StatusCode::REQUEST_TIMEOUT
          | StatusCode::TOO_MANY_REQUESTS
      )
    {
      Answer::Failed(why)
    } else {
      Answer::Refused(why)
    }
  }
}

// What one try came to: taken, with the instruction's id where the route answers one; refused for good; or failed,
// to be tried again.
enum Answer {
  Taken(Option<i64>),
  Refused(String),
  Failed(String),
}

/// The receiver's route a notice goes by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
  /// `POST /instructions`, answered with the instruction's id.
  Instruction,
  /// `DELETE /instructions/{id}`, the id standing in the body's `id`.
  Withdrawal,
}

impl Route {
  fn word(self) -> &'static str {
    match self {
      Self::Instruction => "instruction",
      Self::Withdrawal => "withdrawal",
    }
  }

  fn read(word: &str) -> Self {
    match word {
      "withdrawal" => Self::Withdrawal,
      _ => Self::Instruction,
    }
  }
}

/// How a row is kept once the receiver took it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
  /// Let go: told at least once, and done.
  Once,
  /// Kept with the instruction's id until it is let go, then withdrawn.
  Kept,
  /// Told again every `retell_seconds` and at every start while it is owed, then let go and withdrawn.
  Standing,
}

impl Keep {
  fn word(self) -> &'static str {
    match self {
      Self::Once => "once",
      Self::Kept => "kept",
      Self::Standing => "standing",
    }
  }

  fn read(word: &str) -> Self {
    match word {
      "kept" => Self::Kept,
      "standing" => Self::Standing,
      _ => Self::Once,
    }
  }
}

/// One notice owed: its key, the route it goes by, the body it is told with, how it is kept once told, and the
/// mark it was owed for, which its owner reads to tell whether the same notice is owed already.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
  pub key: String,
  pub route: Route,
  pub body: Value,
  pub keep: Keep,
  pub mark: String,
}

impl Notice {
  /// An instruction, keyed by its kind.
  pub fn instruction(body: Value, keep: Keep) -> Self {
    Self {
      key: body["kind"].as_str().unwrap_or_default().to_owned(),
      route: Route::Instruction,
      body,
      keep,
      mark: String::new(),
    }
  }

  /// The same notice, owed for `mark`.
  pub fn marked(self, mark: impl Into<String>) -> Self {
    Self {
      mark: mark.into(),
      ..self
    }
  }

  /// The same notice under a key of its own, where one kind is told to two rooms.
  pub fn keyed(self, key: impl Into<String>) -> Self {
    Self {
      key: key.into(),
      ..self
    }
  }
}

/// A row owed, as its owner reads it back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owed {
  pub mark: String,
  /// When it was first owed, in epoch milliseconds.
  pub owed_at: i64,
  /// When the receiver last took it, where it did.
  pub told_at: Option<i64>,
  /// The row as it was read: a let-go naming it lets nothing go that was owed again since.
  pub version: i64,
}

fn fault(why: Fault) -> String {
  why.detail
}

/// The notice owed: written over any row of its key and due at once, then tried once. Nothing is owed where the
/// deployment names no receiver. Fails only where the store would not take the row; a try that fails leaves the
/// row to the sender.
pub async fn owe(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  notice: Notice,
) -> Result<(), String> {
  let Some(receiver) = Receiver::of(vars) else { return Ok(()) };
  let now = crate::now_ms();

  run(
    db,
    r#"INSERT INTO "notices" ("key", "route", "body", "keep", "mark", "owed_at", "due_at")
       VALUES ($1, $2, $3, $4, $5, $6, $6)
       ON CONFLICT ("key") DO UPDATE SET "route" = excluded."route", "body" = excluded."body",
         "keep" = excluded."keep", "mark" = excluded."mark", "version" = nextval('notice_versions'),
         "due_at" = excluded."due_at", "tries" = 0, "failure" = NULL,
         "owed_at" = CASE WHEN "notices"."route" = 'withdrawal' THEN excluded."owed_at" ELSE "notices"."owed_at" END,
         "instruction_id" = CASE WHEN "notices"."route" = 'withdrawal' THEN NULL ELSE "notices"."instruction_id" END"#,
    vec![
      notice.key.clone().into(),
      notice.route.word().into(),
      notice.body.to_string().into(),
      notice.keep.word().into(),
      notice.mark.into(),
      now.into(),
    ],
  )
  .await
  .map_err(fault)?;

  tried(db, &receiver, &notice.key, now).await;

  Ok(())
}

/// The rows owed under a prefix of their key, by what follows the prefix; a withdrawal is no notice owed.
pub async fn owed(db: &DatabaseConnection, prefix: &str) -> Result<BTreeMap<String, Owed>, String> {
  let rows = all(
    db,
    r#"SELECT "key", "mark", "owed_at", "told_at", "version" FROM "notices"
        WHERE "key" LIKE $1 ESCAPE '\' AND "route" <> 'withdrawal'"#,
    vec![format!("{}%", prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")).into()],
  )
  .await
  .map_err(fault)?;
  let mut owed = BTreeMap::new();

  for row in rows {
    let key: String = row.try_get("", "key").map_err(|why| why.to_string())?;

    owed.insert(
      key.strip_prefix(prefix).unwrap_or(&key).to_owned(),
      Owed {
        mark: row.try_get("", "mark").map_err(|why| why.to_string())?,
        owed_at: row.try_get("", "owed_at").map_err(|why| why.to_string())?,
        told_at: row.try_get("", "told_at").map_err(|why| why.to_string())?,
        version: row.try_get("", "version").map_err(|why| why.to_string())?,
      },
    );
  }

  Ok(owed)
}

/// A row let go: gone where it was never told or told once, and turned into the withdrawal of the copy told last
/// where it is kept or standing and the receiver answered an id for it.
pub async fn let_go(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  key: &str,
) -> Result<(), String> {
  let taken = one(
    db,
    r#"DELETE FROM "notices" WHERE "key" = $1 AND "route" <> 'withdrawal'
       RETURNING "route", "keep", "instruction_id""#,
    vec![key.into()],
  )
  .await
  .map_err(fault)?;

  withdrawn(db, vars, key, taken).await
}

/// The same, only where the row still stands as `owed` read it: a row owed again since stays.
pub async fn let_go_as_read(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  key: &str,
  owed: &Owed,
) -> Result<(), String> {
  let taken = one(
    db,
    r#"DELETE FROM "notices" WHERE "key" = $1 AND "route" <> 'withdrawal' AND "version" = $2
       RETURNING "route", "keep", "instruction_id""#,
    vec![key.into(), owed.version.into()],
  )
  .await
  .map_err(fault)?;

  withdrawn(db, vars, key, taken).await
}

// The copy a row let go was told with withdrawn, where it is an instruction kept past its telling.
async fn withdrawn(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  key: &str,
  taken: Option<QueryResult>,
) -> Result<(), String> {
  let Some(row) = taken else { return Ok(()) };
  let route = Route::read(&row.try_get::<String>("", "route").map_err(|why| why.to_string())?);
  let keep = Keep::read(&row.try_get::<String>("", "keep").map_err(|why| why.to_string())?);
  let id: Option<i64> = row.try_get("", "instruction_id").map_err(|why| why.to_string())?;

  match (route, keep, id) {
    (Route::Instruction, Keep::Kept | Keep::Standing, Some(id)) => {
      owe(
        db,
        vars,
        Notice {
          key: key.to_owned(),
          route: Route::Withdrawal,
          body: json!({ "id": id }),
          keep: Keep::Once,
          mark: String::new(),
        },
      )
      .await
    }
    _ => Ok(()),
  }
}

/// Every standing row due at once, and every row still owed tried at once: what agency's start does, so each
/// standing notice is told again and none waits out a wait the process before set. Answers how many are due.
pub async fn restarted(db: &DatabaseConnection) -> Result<u64, String> {
  run(
    db,
    r#"UPDATE "notices" SET "due_at" = $1 WHERE "keep" = 'standing' OR "due_at" IS NOT NULL"#,
    vec![crate::now_ms().into()],
  )
  .await
  .map_err(fault)
}

/// One pass of the sender: every row due tried, the longest due first. Answers how many the receiver took.
pub async fn send_due(db: &DatabaseConnection, vars: &BTreeMap<String, String>) -> Result<usize, String> {
  let Some(receiver) = Receiver::of(vars) else { return Ok(0) };
  let now = crate::now_ms();
  let due = all(
    db,
    r#"SELECT "key" FROM "notices" WHERE "due_at" <= $1 ORDER BY "due_at", "key" LIMIT 200"#,
    vec![now.into()],
  )
  .await
  .map_err(fault)?;
  let mut taken = 0;

  for row in due {
    let key: String = row.try_get("", "key").map_err(|why| why.to_string())?;

    if tried(db, &receiver, &key, now).await {
      taken += 1;
    }
  }

  Ok(taken)
}

// The wait after the `tries`-th failed try, in milliseconds.
fn backoff(tries: i64) -> i64 {
  let doubled = JOB_NOTICES
    .retry_first_seconds
    .saturating_mul(1u64 << tries.clamp(0, 20))
    .min(JOB_NOTICES.retry_most_seconds);

  i64::try_from(doubled).unwrap_or(i64::MAX / 1000) * 1000
}

// One try at the row of `key`, where it is due at `now`: held to this sender while it goes, then written told —
// let go, kept or due again by its keep — or due again after its wait. Answers whether the receiver took it. A row
// owed again while the try went is left as it was owed.
async fn tried(db: &DatabaseConnection, receiver: &Receiver, key: &str, now: i64) -> bool {
  let held_until = now + i64::try_from(JOB_NOTICES.answer_seconds * 2000).unwrap_or(i64::MAX / 2);
  let claimed = one(
    db,
    r#"UPDATE "notices" SET "due_at" = $2 WHERE "key" = $1 AND "due_at" IS NOT NULL AND "due_at" <= $3
       RETURNING "route", "body", "keep", "version", "tries""#,
    vec![key.into(), held_until.into(), now.into()],
  )
  .await;
  let row = match claimed {
    Ok(Some(row)) => row,
    Ok(None) => return false,
    Err(why) => {
      eprintln!("outbox: {key} was not taken up to be sent: {}", why.detail);
      return false;
    }
  };
  let read = |column: &str| row.try_get::<String>("", column).unwrap_or_default();
  let route = Route::read(&read("route"));
  let keep = Keep::read(&read("keep"));
  let body: Value = serde_json::from_str(&read("body")).unwrap_or(Value::Null);
  let version: i64 = row.try_get("", "version").unwrap_or_default();
  let tries: i64 = row.try_get("", "tries").unwrap_or_default();
  let (taken, written) = match receiver.hand(route, &body).await {
    Answer::Taken(id) => (true, told(db, key, version, keep, id).await),
    Answer::Refused(why) => {
      eprintln!("outbox: {key} was refused and is not sent again: {why}");
      (false, told(db, key, version, keep, None).await)
    }
    Answer::Failed(why) => {
      eprintln!("outbox: {key} was not told, try {}: {why}", tries + 1);
      (false, failed(db, key, version, tries, &why).await)
    }
  };

  if let Err(why) = written {
    eprintln!("outbox: what became of {key} was not written: {why}");
  }

  taken
}

// A row the receiver took, or refused for good: let go, kept with the instruction's id, or due again after the
// retelling's wait.
async fn told(
  db: &DatabaseConnection,
  key: &str,
  version: i64,
  keep: Keep,
  id: Option<i64>,
) -> Result<(), String> {
  let now = crate::now_ms();
  let next = match keep {
    Keep::Once => {
      return run(
        db,
        r#"DELETE FROM "notices" WHERE "key" = $1 AND "version" = $2"#,
        vec![key.into(), version.into()],
      )
      .await
      .map(|_| ())
      .map_err(fault);
    }
    Keep::Kept => None,
    Keep::Standing => Some(now + i64::try_from(JOB_NOTICES.retell_seconds * 1000).unwrap_or(i64::MAX / 2)),
  };

  run(
    db,
    r#"UPDATE "notices" SET "told_at" = $3, "instruction_id" = COALESCE($4, "instruction_id"), "due_at" = $5,
         "tries" = 0, "failure" = NULL
       WHERE "key" = $1 AND "version" = $2"#,
    vec![key.into(), version.into(), now.into(), id.into(), next.into()],
  )
  .await
  .map(|_| ())
  .map_err(fault)
}

// A try that failed, the `tries`-th before it having failed too: the row due again after a wait that doubles with
// each failed try.
async fn failed(
  db: &DatabaseConnection,
  key: &str,
  version: i64,
  tries: i64,
  why: &str,
) -> Result<(), String> {
  run(
    db,
    r#"UPDATE "notices" SET "tries" = "tries" + 1, "failure" = $3, "due_at" = $4
       WHERE "key" = $1 AND "version" = $2"#,
    vec![key.into(), version.into(), why.into(), (crate::now_ms() + backoff(tries)).into()],
  )
  .await
  .map(|_| ())
  .map_err(fault)
}
