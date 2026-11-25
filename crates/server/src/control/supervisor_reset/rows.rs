//! The rows of agency's store a supervisor's reset takes (docs/supervisors.md, 「Starting it afresh」): for
//! the conversation range, its room sessions with their kept turns, its run log lines and its runs in the
//! run ledger, and those of the rooms it answers alone, counted for a dry run or deleted in one write. The
//! person range holds no row of agency's: what a supervisor remembers is the brain's.

use sea_orm::{DatabaseConnection, TransactionTrait};
use store::{all, one, run};

use super::super::Refused;

/// What a reset may take: what the supervisor said, or what it remembers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
  Conversation,
  Person,
}

impl Range {
  pub fn name(self) -> &'static str {
    match self {
      Self::Conversation => "conversation",
      Self::Person => "person",
    }
  }
}

/// The supervisor a reset is for, and the rooms the caller says it answers alone, whose whole run log goes
/// with it.
#[derive(Clone, Debug)]
pub struct Scope {
  pub agent: String,
  pub alone: Vec<i64>,
}

/// A condition built up with its placeholders numbered as they are written.
#[derive(Clone, Debug, Default)]
pub struct Condition {
  pub text: String,
  pub args: Vec<sea_orm::Value>,
}

impl Condition {
  fn say(mut self, text: &str) -> Self {
    self.text.push_str(text);
    self
  }

  fn arg(mut self, arg: sea_orm::Value) -> Self {
    self.args.push(arg);
    self.text.push_str(&format!("${}", self.args.len()));
    self
  }

  fn agent(self, scope: &Scope) -> Self {
    self.arg(scope.agent.clone().into())
  }

  /// A room column among the rooms answered alone; no room is a list nothing is in.
  fn in_rooms(mut self, column: &str, scope: &Scope) -> Self {
    self.text.push_str(column);
    self.text.push_str(" IN (");

    if scope.alone.is_empty() {
      self.text.push_str("NULL");
    }

    for (at, room) in scope.alone.iter().enumerate() {
      if at > 0 {
        self.text.push_str(", ");
      }

      self = self.arg((*room).into());
    }

    self.text.push(')');
    self
  }
}

/// The rows of one table a reset takes in one range.
#[derive(Clone, Debug)]
pub struct Pick {
  pub table: &'static str,
  pub range: Range,
  pub condition: Condition,
}

/// How many rows of one table went, or would go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counted {
  pub table: &'static str,
  pub range: Range,
  pub rows: i64,
}

fn of_agent(table: &'static str, scope: &Scope) -> Pick {
  Pick {
    table,
    range: Range::Conversation,
    condition: Condition::default().say("agent = ").agent(scope),
  }
}

/// Every pick of the ranges asked, in the order they are deleted: a pick reading another table comes
/// before that table's own.
pub fn picks(scope: &Scope, ranges: &[Range]) -> Vec<Pick> {
  let mine = || Condition::default().say("agent = ").agent(scope).say(" OR ").in_rooms("chat_id", scope);
  let mut all = vec![
    Pick {
      table: "agent_log",
      range: Range::Conversation,
      condition: mine(),
    },
    Pick {
      table: "runs",
      range: Range::Conversation,
      condition: mine(),
    },
    of_agent("room_session_turns", scope),
    of_agent("room_sessions", scope),
  ];

  all.retain(|one| ranges.contains(&one.range));
  all
}

/// The runs the supervisor's own log lines went under, read before those lines go: the rooms' owner keeps what
/// each run wrote down under the run's name, and a chat run's name says its room but not the agent that ran it.
pub async fn runs_of(db: &DatabaseConnection, agent: &str) -> Result<Vec<String>, Refused> {
  let rows = all(
    db,
    r#"SELECT DISTINCT "run" FROM "agent_log" WHERE "agent" = $1 ORDER BY "run""#,
    vec![agent.into()],
  )
  .await?;

  rows.iter().map(|row| row.try_get::<String>("", "run").map_err(Refused::from)).collect()
}

/// How many rows each pick would take, nothing written.
pub async fn count(db: &DatabaseConnection, picks: &[Pick]) -> Result<Vec<Counted>, Refused> {
  let mut counted = vec![];

  for pick in picks {
    let sql = format!("SELECT count(*) AS n FROM {} WHERE {}", pick.table, pick.condition.text);
    let rows = one(db, &sql, pick.condition.args.clone())
      .await?
      .map(|row| row.try_get::<i64>("", "n"))
      .transpose()?
      .unwrap_or_default();

    counted.push(Counted {
      table: pick.table,
      range: pick.range,
      rows,
    });
  }

  Ok(counted)
}

/// Deletes every pick's rows in order, as one commit, and says how many of each went.
pub async fn delete(db: &DatabaseConnection, picks: &[Pick]) -> Result<Vec<Counted>, Refused> {
  let written = db.begin().await?;
  let mut counted = vec![];

  for pick in picks {
    let sql = format!("DELETE FROM {} WHERE {}", pick.table, pick.condition.text);
    let gone = run(&written, &sql, pick.condition.args.clone()).await?;

    counted.push(Counted {
      table: pick.table,
      range: pick.range,
      rows: i64::try_from(gone).unwrap_or(i64::MAX),
    });
  }

  written.commit().await?;

  Ok(counted)
}
