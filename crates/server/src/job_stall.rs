//! A job's turn that makes no progress, told to its item's orderer (docs/agent-control.md, 「A run's tool calls
//! are held to a clock」, a job's turn that makes no progress): the turn's call clock says when it stalled and when
//! it moved again, and this owes the notice `job-stalled` in agency's outbox, kept until the turn moves or ends.
//! The row is the stall's mark, so a stall the outbox already owes for the item is not owed twice.

use std::collections::BTreeMap;

use policy::fill_marks;
use policy::job_notices::JOB_NOTICES;
use sea_orm::DatabaseConnection;
use serde_json::Value;

use crate::door::stall::{StallChange, Stalled};
use crate::job_notices::instruction;
use crate::outbox::{self, Keep, Notice};
use crate::queue::orderer::Orderer;

/// The event and the kind's first word of the notice.
pub const STALLED_EVENT: &str = "job-stalled";

fn key_of(item: i64) -> String {
  format!("{STALLED_EVENT}:{item}")
}

/// The notice of a stalled job: the job, its item, its worker, how long it has made no progress and since when,
/// and what it waits on.
pub fn stalled(member: &Value, stall: &Stalled, orderer: &Orderer) -> Value {
  let item = member["itemId"].as_i64().unwrap_or_default();
  let minutes = ((stall.at - stall.since) / 60_000).max(1);
  let waiting = match stall.waits.is_empty() {
    true => JOB_NOTICES.stalled_no_call.clone(),
    false => stall
      .waits
      .iter()
      .map(|waits| {
        let lines = match waits.lines.is_empty() {
          true => JOB_NOTICES.stalled_no_lines.clone(),
          false => waits.lines.join("\n"),
        };

        fill_marks(&JOB_NOTICES.stalled_call, &[("tool", &waits.tool), ("lines", &lines)])
      })
      .collect::<Vec<_>>()
      .join("\n"),
  };
  let order = fill_marks(
    &JOB_NOTICES.stalled_order,
    &[
      ("job", &member["id"].as_i64().unwrap_or_default().to_string()),
      ("item", &item.to_string()),
      ("worker", member["workerName"].as_str().unwrap_or_default()),
      ("minutes", &minutes.to_string()),
      ("since", &crate::run_clock::clock_time(stall.since)),
      ("waiting", &waiting),
    ],
  );

  instruction(
    STALLED_EVENT,
    item,
    orderer,
    format!("The job of item #{item} has made no progress for {minutes} minutes"),
    order,
  )
}

/// A stall of the job `member` owed to its item's orderer, kept until it is let go; none owed where one already
/// stands for the item, or where the job holds no item.
pub async fn tell(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  member: &Value,
  stall: &Stalled,
) {
  let item = member["itemId"].as_i64().unwrap_or_default();

  if item <= 0 {
    return;
  }

  match outbox::owed(db, &format!("{STALLED_EVENT}:")).await {
    Ok(owed) if owed.contains_key(&item.to_string()) => return,
    Ok(_) => {}
    Err(why) => eprintln!("job of item #{item}: its stall notices were not read: {why}"),
  }

  let orderer = crate::job_end::orderer(vars, member).await;
  let notice =
    Notice::instruction(stalled(member, stall, &orderer), Keep::Kept).marked(stall.since.to_string());

  match outbox::owe(db, vars, notice).await {
    Ok(()) => println!("job of item #{item}: its stall since {} owed to its orderer", stall.since),
    Err(why) => eprintln!("job of item #{item}: its stall was not owed: {why}"),
  }
}

/// The stall notice of an item let go, withdrawing the copy told.
pub async fn withdraw(db: &DatabaseConnection, vars: &BTreeMap<String, String>, item: i64) {
  if let Err(why) = outbox::let_go(db, vars, &key_of(item)).await {
    eprintln!("job of item #{item}: its stall notice was not let go: {why}");
  }
}

/// The stall notices of one job's turn: each change its call clock hands over told, and the notice let go as the
/// turn ends.
pub struct JobStall {
  job: i64,
  // The job as its row stands, read once a change comes; none before.
  member: Option<Value>,
}

impl JobStall {
  pub fn new(job: i64) -> Self {
    Self { job, member: None }
  }

  async fn member(&mut self, db: &DatabaseConnection) -> Option<&Value> {
    if self.member.is_none() {
      match crate::control::work::job(db, self.job).await {
        Ok((member, _)) => self.member = Some(member),
        Err(why) => eprintln!("job {}: its row was not read for its stall: {}", self.job, why.error),
      }
    }

    self.member.as_ref()
  }

  /// A change of the turn's progress told: a stall owed, a move after one let go.
  pub async fn heard(
    &mut self,
    db: &DatabaseConnection,
    vars: &BTreeMap<String, String>,
    change: &StallChange,
  ) {
    let Some(member) = self.member(db).await.cloned() else { return };

    match change {
      StallChange::Stalled(stall) => tell(db, vars, &member, stall).await,
      StallChange::Resumed => self.ended(db, vars).await,
    }
  }

  /// The turn ended, or moved again: a stall notice of its item let go.
  pub async fn ended(&mut self, db: &DatabaseConnection, vars: &BTreeMap<String, String>) {
    let Some(item) = self.member(db).await.and_then(|member| member["itemId"].as_i64()) else {
      return;
    };

    if item > 0 {
      withdraw(db, vars, item).await;
    }
  }
}
