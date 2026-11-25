//! The facts of a person's supply (docs/supply.md, 「The facts told」): `supply.changed` published once as a run
//! moves to another source or finds none left, and the fact told, through the outbox, to the supervisor in the
//! room the run answers as a row that opens no turn; no source left told once while it stays so. The owner is not
//! told.

use policy::fill_marks;
use policy::supply::SUPPLY;
use serde::Serialize;
use serde_json::{Value, json};

use super::Closed;
use super::meter::Metered;
use crate::events::Event;
use crate::outbox::{Keep, Notice};
use crate::places::Ground;

/// A run moved to another source.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplyMoved {
  pub fact: &'static str,
  pub run: String,
  pub account: Option<i64>,
  pub job: Option<i64>,
  pub supervisor: Option<String>,
  pub provider: String,
  pub model: String,
  pub from: String,
  pub to: String,
  pub reason: String,
}

impl Event for SupplyMoved {
  const NAME: &'static str = "supply.changed";
}

/// A run found no source left.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplyClosed {
  pub fact: &'static str,
  pub run: String,
  pub account: Option<i64>,
  pub job: Option<i64>,
  pub supervisor: Option<String>,
  pub provider: String,
  pub model: String,
  pub source: String,
  /// Why no source is left, the word the room's line carries.
  pub why: &'static str,
  pub reason: String,
  pub reopens_at: Option<i64>,
  pub sentence: String,
}

impl Event for SupplyClosed {
  const NAME: &'static str = "supply.changed";
}

/// Who a run's facts are told to: the supervisor it answers for and the room.
#[derive(Clone, Debug, Default)]
pub struct Told {
  pub supervisor: Option<String>,
  pub room: Option<i64>,
}

// The fact told to the person's own supervisor in the run's room, owed in the outbox under its key. It rides that
// room's rounds for good: a turn it opened could fail for want of supply, and a fact told of that failure would
// wake the next. A fixed line the room hears at once as the receiver's own goes with it where one is given.
async fn order(ground: &Ground, told: &Told, key: String, said: String, keep: Keep, line: Option<Value>) {
  let mark = if line.is_some() { WITH_LINE } else { "" };
  let (Some(supervisor), Some(room)) = (&told.supervisor, told.room) else {
    return;
  };
  let mut body = json!({
    "agentName": supervisor,
    "room": room,
    "said": { "fact": said },
    "sort": "notice",
    "kind": key,
    "summary": said,
    "ridesForGood": true,
  });

  if let Some(line) = line {
    body["line"] = line;
  }

  if let Err(why) =
    crate::outbox::owe(&ground.db, ground.vars(), Notice::instruction(body, keep).marked(mark)).await
  {
    eprintln!("agency: the supply notice {key}: {why}");
  }
}

// The mark a fact told with the room's line is owed under, so a fact owed before the line was carried is told again,
// this time with it.
const WITH_LINE: &str = "line";

// The key no source left is kept under: one a supervisor and room, so it is told once while it stays so.
fn closed_key(supervisor: &str, room: i64) -> String {
  format!("supply:supply-closed:{supervisor}:{room}")
}

/// A run moved from one source to another, published and told once.
pub async fn moved(ground: &Ground, metered: &Metered, from: &str, to: &str, why: &str) {
  let chosen = &metered.chosen;
  let told = Told {
    supervisor: supervisor_of(ground, &metered.agent).await,
    room: metered.room,
  };

  moved_on(ground, chosen, metered.job, &told, from, to, why).await;
}

/// The same, for a run named by what it chose and who it is told to.
pub async fn moved_on(
  ground: &Ground,
  chosen: &super::Chosen,
  job: Option<i64>,
  told: &Told,
  from: &str,
  to: &str,
  why: &str,
) {
  ground.events.publish(SupplyMoved {
    fact: "supply-moved",
    run: chosen.run.clone(),
    account: chosen.account,
    job,
    supervisor: told.supervisor.clone(),
    provider: chosen.provider.clone(),
    model: chosen.model.clone(),
    from: from.to_owned(),
    to: to.to_owned(),
    reason: why.to_owned(),
  });

  let said = fill_marks(&SUPPLY.words.moved_told, &[("from", from), ("to", to), ("why", why)]);

  order(ground, told, format!("supply:supply-moved:{}", chosen.run), said, Keep::Once, None).await;
}

/// A run that found no source left, published and told once.
#[allow(clippy::too_many_arguments)]
pub async fn closed(
  ground: &Ground,
  run: &str,
  account: Option<i64>,
  job: Option<i64>,
  provider: &str,
  model: &str,
  told: &Told,
  closed: &Closed,
) {
  ground.events.publish(SupplyClosed {
    fact: "supply-closed",
    run: run.to_owned(),
    account,
    job,
    supervisor: told.supervisor.clone(),
    provider: provider.to_owned(),
    model: model.to_owned(),
    source: closed.source.clone(),
    why: closed.why.word(),
    reason: closed.reason.clone(),
    reopens_at: closed.reopens_at,
    sentence: closed.sentence.clone(),
  });
  let (Some(supervisor), Some(room)) = (&told.supervisor, told.room) else {
    return;
  };
  let key = closed_key(supervisor, room);

  // Told already while none is left, with the room's line: a later run finding none tells nothing again.
  match crate::outbox::owed(&ground.db, &key).await {
    Ok(owed) if owed.get("").is_some_and(|told| told.mark == WITH_LINE) => return,
    Ok(_) => {}
    Err(why) => eprintln!("agency: the supply notice {key} could not be read: {why}"),
  }

  order(ground, told, key, closed.sentence.clone(), Keep::Kept, Some(line(closed))).await;
}

/// The fixed line the room hears at once where no source is left (the receiver's `docs/codes.md`, 「Fixed lines」):
/// why, and the moment it reopens in epoch seconds where the hook named one. The room's own screen says it in the
/// reader's language; no turn has to go out for the person to hear it.
pub fn line(closed: &Closed) -> Value {
  json!({
    "code": "supply-closed",
    "why": closed.why.word(),
    "until": closed.reopens_at.map(|at| at / 1000),
  })
}

/// A run of the supervisor gone out on a source in the room: no source left, told there, is taken away and its row
/// withdrawn where no round took it, so the next closing is told afresh.
pub async fn reopened(ground: &Ground, told: &Told) {
  let (Some(supervisor), Some(room)) = (&told.supervisor, told.room) else {
    return;
  };
  let key = closed_key(supervisor, room);

  if let Err(why) = crate::outbox::let_go(&ground.db, ground.vars(), &key).await {
    eprintln!("agency: the supply notice {key} was not let go: {why}");
  }
}

/// A person's own key or login the provider refused, told to their own supervisor room once.
pub async fn refused(ground: &Ground, told: &Told, run: &str, source: &str, why: &str) {
  let said = fill_marks(&SUPPLY.words.refused_told, &[("source", source), ("why", why)]);

  order(ground, told, format!("supply:supply-refused:{run}"), said, Keep::Once, None).await;
}

/// The supervisor an agent's runs answer for: the agent itself where it is a supervisor.
pub async fn supervisor_of(ground: &Ground, agent: &str) -> Option<String> {
  crate::supervisors::named(&ground.db, agent)
    .await
    .ok()
    .flatten()
    .map(|supervisor| supervisor.name)
}
