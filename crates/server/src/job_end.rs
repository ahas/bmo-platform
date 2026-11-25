//! A job's end giving back its slot, reaching its orderer, and its lease kept for the item's close or given
//! back (docs/api.md, 「assignments and jobs」): the slot every request of the job's run held, given back
//! whatever its lease is bound to; the report a run left, owed to the item's orderer in agency's outbox, which
//! queues it through the receiver's instruction route, and a failure in another room owed to the owner as well; the job service's lease on the
//! item kept standing where the job ended, so the item stays leased until its close ends the lease, and given
//! back where it was stopped, so the item is open again for a rerun. A run's end is written where the run
//! ends, so neither the room's session being replaced nor the harness that launched the job being gone loses
//! it: the outbox keeps the notice until the receiver takes it, and the receiver keeps it until a turn does.

use std::collections::BTreeMap;

use policy::job_notices::JOB_NOTICES;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};
use store::run;

use crate::order_run::dispatch;
use crate::outbox::{Keep, Notice};
use crate::places::Ground;

fn setting<'a>(vars: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
  vars.get(name).map(String::as_str).map(str::trim).filter(|value| !value.is_empty())
}

/// Whether these settings stand a test's server: a ground a test stands reaches no peer, whatever its
/// environment names, so a test never posts into a room or a queue a person holds.
pub(crate) fn test_ground(vars: &BTreeMap<String, String>) -> bool {
  setting(vars, "BMO_TEST_GROUND").is_some_and(|set| set == "1")
}

/// The token a peer is called with: a value the deployment names, or the file it names.
pub(crate) fn token(vars: &BTreeMap<String, String>, variable: &str, file: &str) -> Option<String> {
  if test_ground(vars) {
    return None;
  }

  if let Some(token) = setting(vars, variable) {
    return Some(token.to_owned());
  }

  setting(vars, file)
    .and_then(|path| std::fs::read_to_string(path).ok())
    .map(|token| token.trim().to_owned())
    .filter(|token| !token.is_empty())
}

pub(crate) fn address(vars: &BTreeMap<String, String>, variable: &str) -> Option<String> {
  if test_ground(vars) {
    eprintln!("{variable}: a test ground reaches no peer; the call is not made");

    return None;
  }

  setting(vars, variable).map(|url| url.trim_end_matches('/').to_owned())
}

/// The slot a job's run holds given back as the job's end is written (docs/role-slots.md, 「The line」): every
/// request asked under the job's owner given back as that owner's release gives it back, kept with the reason
/// the end carries, whatever its lease is bound to. Every write of a job's end calls it, so no ended job keeps
/// a slot of its role; a request its holder already gave back is left as it stands, and a store that would
/// not take it is said and never fails the end. The job's own session is started for each turn and ended with
/// it, so no role session is left for this release to end.
pub async fn slot_given_back(db: &DatabaseConnection, events: &crate::events::Bus, id: i64, why: &str) {
  let owner = format!("{}{id}", dispatch::JOB_OWNER);
  let why: String = format!("its job ended: {}", why.trim()).chars().take(REASON_CHARS).collect();

  match dispatch::release_owner_for(db, events, &owner, &why).await {
    Ok(done) if done.released + done.cancelled + done.reservations > 0 => {
      println!(
        "job {id}: its slot given back with its end: {} let in, {} waiting, {} reservations",
        done.released, done.cancelled, done.reservations
      );
    }
    Ok(_) => {}
    Err(fault) => eprintln!("job {id}: its slot was not given back with its end: {fault}"),
  }
}

// The most characters of an end's reason a given-back request keeps, within what the line keeps of one.
const REASON_CHARS: usize = 400;

/// Whether a job's end keeps its item's lease standing for the item's close: a job that ended, holding a
/// lease. A stopped job's item is open again for a rerun.
pub fn keeps_lease(member: &Value) -> bool {
  member["state"].as_str() == Some("ended") && member["leaseId"].as_i64().is_some_and(|lease| lease > 0)
}

/// A job's end delivered: its lease kept for the item's close where it ended, which the lease pass renews
/// until the close ends it, or given back to the job service where it was stopped; and its report queued for
/// the room's supervisor. Each call stands on its own, and a call that fails is said and never fails the job.
pub async fn delivered(ground: &Ground, member: &Value) {
  if keeps_lease(member)
    && let Err(why) = run(
      &ground.db,
      r#"UPDATE "jobs" SET "lease_kept" = true WHERE "id" = $1"#,
      vec![id_of(member).into()],
    )
    .await
  {
    eprintln!("job {}: its lease was not kept for the item's close: {why:?}", id_of(member));
  }

  if ground.testing() {
    eprintln!("job {}: its end was not delivered: the ground is a test's", id_of(member));

    return;
  }

  delivered_with(&ground.db, ground.vars(), member).await;
}

/// The same, kept from the settings the deployment names, so a test reaches it with a map of its own.
pub async fn delivered_with(db: &DatabaseConnection, vars: &BTreeMap<String, String>, member: &Value) {
  if test_ground(vars) {
    eprintln!("job {}: its end was not delivered: the ground is a test's", id_of(member));

    return;
  }

  if !keeps_lease(member) {
    return_lease(vars, member).await;
  }

  tell_room(db, vars, member).await;
}

/// The lease a stopped job's item was held by, given back, so the item is open again for a rerun. A lease
/// already gone is no failure.
async fn return_lease(vars: &BTreeMap<String, String>, member: &Value) {
  let lease = member["leaseId"].as_i64().unwrap_or_default();

  if lease <= 0 {
    return;
  }

  let (Some(url), Some(token)) =
    (address(vars, "BMO_JOB_URL"), token(vars, "BMO_JOB_TOKEN", "BMO_JOB_TOKEN_FILE"))
  else {
    eprintln!("job {}: its lease {lease} was not returned: no job service is named", id_of(member));
    return;
  };

  let answer = reqwest::Client::new()
    .delete(format!("{url}/leases/{lease}"))
    .bearer_auth(token)
    .send()
    .await;

  match answer {
    Ok(answer) if answer.status().is_success() => {}
    Ok(answer) if answer.status() == reqwest::StatusCode::NOT_FOUND => {}
    Ok(answer) => {
      eprintln!("job {}: its lease {lease} was not returned: {}", id_of(member), answer.status())
    }
    Err(why) => eprintln!("job {}: its lease {lease} was not returned: {why}", id_of(member)),
  }
}

/// The job's report owed to the item's orderer: an instruction that waits in the receiver for the next turn,
/// naming the item and carrying the report whole; and a failure in a room other than the owner's own owed to the
/// owner as well. Once both rows are written the job's end is marked owed, so the sender owes no end twice and
/// owes one a stop cut off. No receiver named is a deployment that carries none, and then no one is told.
async fn tell_room(db: &DatabaseConnection, vars: &BTreeMap<String, String>, member: &Value) {
  if crate::outbox::Receiver::of(vars).is_none() {
    eprintln!("job {}: its end was not told to the room: no receiver is named", id_of(member));

    return;
  }

  let orderer = orderer(vars, member).await;
  let ended = Notice::instruction(instruction_body(member, &orderer), Keep::Once);
  let mut owed = true;

  if let Err(why) = crate::outbox::owe(db, vars, ended).await {
    eprintln!("job {}: its end was not owed to the room: {why}", id_of(member));
    owed = false;
  }

  if crate::job_notices::owner_hears_failure(member, &orderer) {
    // Keyed by the job, so the orderer's own `job-ended` and another failure of the item are never replaced by it.
    let failed = Notice::instruction(crate::job_notices::failed(member, orderer.room), Keep::Once)
      .keyed(format!("job-failed:{}@owner", id_of(member)));

    if let Err(why) = crate::outbox::owe(db, vars, failed).await {
      eprintln!("job {}: its failure was not owed to the owner's room: {why}", id_of(member));
      owed = false;
    }
  }

  if owed
    && let Err(why) =
      run(db, r#"UPDATE "jobs" SET "end_owed" = true WHERE "id" = $1"#, vec![id_of(member).into()]).await
  {
    eprintln!("job {}: its end was not marked owed: {}", id_of(member), why.detail);
  }
}

/// The ends no notice is owed for yet, owed (docs/agent-control.md, 「An item's change is told to its orderer」):
/// each job ended at least `owe_grace_seconds` ago whose end's notice was not written — a stop of agency's cut it
/// off, the store refused it, or the end was written where no server told it. Answers how many were owed.
pub async fn unowed_ends(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
) -> Result<usize, String> {
  if crate::outbox::Receiver::of(vars).is_none() {
    return Ok(0);
  }

  let grace = i64::try_from(JOB_NOTICES.owe_grace_seconds * 1000).unwrap_or(i64::MAX / 2);
  let rows = store::all(
    db,
    r#"SELECT "id" FROM "jobs" WHERE NOT "end_owed" AND "ended_at" <= $1
         AND "state" NOT IN ('starting', 'running', 'reviewing')
       ORDER BY "id""#,
    vec![(crate::now_ms() - grace).into()],
  )
  .await
  .map_err(|why| why.detail)?;
  let mut owed = 0;

  for row in rows {
    let id: i64 = row.try_get("", "id").map_err(|why| why.to_string())?;
    let (member, _) = crate::control::work::job(db, id).await.map_err(|why| why.error)?;

    tell_room(db, vars, &member).await;
    println!("job {id}: its end owed, no notice of it having been written");
    owed += 1;
  }

  Ok(owed)
}

/// The orderer a job's end is told to: the item's, up the chain it was raised from, read from the job service;
/// where the service cannot be read or the chain finds no room, the room the job was launched for, with the
/// item's requester where it was read.
pub async fn orderer(vars: &BTreeMap<String, String>, member: &Value) -> crate::queue::orderer::Orderer {
  let launched = member["roomId"].as_i64().filter(|room| *room != 0);
  let found = match (crate::queue::orderer::JobService::of(vars), member["itemId"].as_i64()) {
    (Some(service), Some(item)) if item > 0 => match service.orderer_of_id(item).await {
      Ok((_, orderer)) => orderer,
      Err(why) => {
        eprintln!("job {}: its item's orderer was not read: {why}", id_of(member));
        crate::queue::orderer::Orderer::default()
      }
    },
    _ => crate::queue::orderer::Orderer::default(),
  };

  crate::queue::orderer::Orderer {
    room: found.room.or(launched),
    requester: found.requester,
  }
}

fn id_of(member: &Value) -> i64 {
  member["id"].as_i64().unwrap_or_default()
}

/// What the room's supervisor is told: an order carrying the item, the worker, the state and the report;
/// where the job ended holding a lease, the lease that still stands on the item. The order carries no
/// instruction: the supervisor's manual says how to relay an end and close its item. A job that ended `ended`
/// had its report passed by its review, whose checks told it to the same supervisor, so the order names that
/// report and the item it stands on instead of carrying its words (docs/job-review.md, 「The job's end」); a job
/// that ended any other way carries its report whole, and one that left none says so.
pub fn order(member: &Value) -> String {
  let item = member["itemId"].as_i64().unwrap_or_default();
  let worker = member["workerName"].as_str().unwrap_or_default();
  let state = member["state"].as_str().unwrap_or_default();
  let outcome = member["outcome"].as_str().map(str::trim).filter(|why| !why.is_empty());
  let report = member["report"].as_str().map(str::trim).filter(|said| !said.is_empty());
  let ended = match (state, outcome) {
    ("ended", None) => "ended".to_owned(),
    ("stopped", None) => "stopped".to_owned(),
    (state, Some(outcome)) if outcome == state => state.to_owned(),
    (state, Some(outcome)) => format!("{state}, its outcome {outcome}"),
    (state, None) => state.to_owned(),
  };
  let reviewed = state == "ended" && outcome.is_none() && report.is_some();
  let mut text = if reviewed {
    format!(
      "The job of item #{item} on worker {worker} {ended}.\n\nIts report is the one its review passed, which \
       the review's checks told you; job_reviews on item #{item} reads it again."
    )
  } else {
    format!(
      "The job of item #{item} on worker {worker} {ended}.\n\nIts report, as the run left it:\n\n{}",
      report.unwrap_or("The run left no report; its end is all there is to tell.")
    )
  };

  if keeps_lease(member) {
    let lease = member["leaseId"].as_i64().unwrap_or_default();

    text.push_str(&format!("\n\nThe item stays leased under lease {lease}: it is not closed."));
  }

  text
}

/// The instruction queued for the orderer: the order, the item it names as news of the queue, the orderer's
/// room, the owner's private room where it names none, the requester of the item that room was found on, and
/// the run it was. It is a job's status notice, an order under the sort `job`.
pub fn instruction_body(member: &Value, orderer: &crate::queue::orderer::Orderer) -> Value {
  let item = member["itemId"].as_i64().unwrap_or_default();
  let run = member["runId"].as_i64().filter(|run| *run != 0);
  json!({
    "room": orderer.room,
    "said": { "order": order(member) },
    "agentName": orderer.requester,
    "sort": "job",
    "kind": format!("job-ended:{item}"),
    "summary": format!("The job of item #{item} ended"),
    "events": [format!("job-ended:#{item}")],
    "run": run,
  })
}
