//! What an item's orderer is told of a change in the queue (docs/agent-control.md, 「An item's change is told to
//! its orderer」): each an order under the sort `job` in the orderer's room, with a kind and an event of its row
//! and the item's or the question's number, owed in agency's outbox (`crate::outbox`), which tells the receiver.

use std::collections::BTreeMap;

use policy::fill_marks;
use policy::job_notices::JOB_NOTICES;
use policy::machine_turn::MACHINE_TURN;
use serde_json::{Value, json};

use crate::outbox::{self, Keep, Notice};
use crate::queue::orderer::{JobService, Orderer};
use crate::watches::closes::Close;

/// An instruction for an orderer: an order under the sort `job` in its room, the owner's private room where it
/// names none, of the row `event` about `number`, so a newer one of the same kind replaces one still pending.
pub fn instruction(event: &str, number: i64, orderer: &Orderer, summary: String, order: String) -> Value {
  json!({
    "room": orderer.room,
    "said": { "order": order },
    "agentName": Value::Null,
    "sort": "job",
    "kind": format!("{event}:{number}"),
    "summary": summary,
    "events": [format!("{event}:#{number}")],
    "run": Value::Null,
  })
}

fn title_of(item: &Value) -> &str {
  item["title"].as_str().unwrap_or_default()
}

fn id_of(item: &Value) -> i64 {
  item["id"].as_i64().unwrap_or_default()
}

/// An open item in a queue that gives no lease.
pub fn paused(item: i64, title: &str, queue: &str, orderer: &Orderer) -> Value {
  let number = item.to_string();
  let order =
    fill_marks(&JOB_NOTICES.paused_order, &[("item", &number), ("title", title), ("queue", queue)]);

  instruction(
    "item-queue-paused",
    item,
    orderer,
    format!("Item #{item} waits on its paused queue"),
    order,
  )
}

/// An open item whose assignee no checkout agency holds is of.
pub fn unlaunchable(item: i64, title: &str, assignee: &str, orderer: &Orderer) -> Value {
  let number = item.to_string();
  let order = fill_marks(
    &JOB_NOTICES.unlaunchable_order,
    &[("item", &number), ("title", title), ("assignee", assignee)],
  );

  instruction("item-unlaunchable", item, orderer, format!("Item #{item} names no repository"), order)
}

/// An item whose `agent` label no launch takes: none named, one the agency does not register, one switched off,
/// or one no job can run on.
pub fn agent_refused(item: i64, title: &str, label: &str, why: &str, orderer: &Orderer) -> Value {
  let number = item.to_string();
  let order = fill_marks(
    &JOB_NOTICES.agent_order,
    &[("item", &number), ("title", title), ("label", label), ("why", why)],
  );

  instruction(
    "item-agent-refused",
    item,
    orderer,
    format!("Item #{item} names no agent a job can run on"),
    order,
  )
}

/// An open item whose `role` label names no role agency holds, with the roles it holds.
pub fn role_unknown(item: i64, title: &str, label: &str, roles: &[String], orderer: &Orderer) -> Value {
  let number = item.to_string();
  let roles = roles.join(", ");
  let order = fill_marks(
    &JOB_NOTICES.role_order,
    &[("item", &number), ("title", title), ("label", label), ("roles", &roles)],
  );

  instruction(
    "item-role-unknown",
    item,
    orderer,
    format!("Item #{item} names no role agency holds"),
    order,
  )
}

/// An open item whose model no agent of its role that is switched on runs, so no launch takes it
/// (docs/repositories.md, 「An agent an item names」): its role, its model and the role's agents with their roads.
pub fn no_candidate(
  item: i64,
  title: &str,
  role: &str,
  model: &str,
  agents: &str,
  orderer: &Orderer,
) -> Value {
  let number = item.to_string();
  let order = fill_marks(
    &JOB_NOTICES.no_candidate_order,
    &[
      ("item", &number),
      ("title", title),
      ("model", model),
      ("role", role),
      ("agents", agents),
    ],
  );

  instruction(
    "item-no-candidate",
    item,
    orderer,
    format!("Item #{item} names a model no {role} agent runs"),
    order,
  )
}

/// An item moved while a job of agency's still runs on it: held, dropped, merged or leased by another holder.
pub fn moved(item: &Value, job: i64, worker: &str, orderer: &Orderer) -> Value {
  let id = id_of(item);
  let number = id.to_string();
  let job = job.to_string();
  let state = moved_state(item);
  let merged = item["mergedInto"]
    .as_i64()
    .map(|into| format!(" into item #{into}"))
    .unwrap_or_default();
  let order = fill_marks(
    &JOB_NOTICES.moved_order,
    &[
      ("item", &number),
      ("title", title_of(item)),
      ("state", &state),
      ("merged", &merged),
      ("job", &job),
      ("worker", worker),
    ],
  );

  instruction(
    "item-moved-under-job",
    id,
    orderer,
    format!("Item #{id} moved under its running job"),
    order,
  )
}

/// The state a moved item is told in: its own, or leased by another holder where a lease of another stands.
pub fn moved_state(item: &Value) -> String {
  match item["state"].as_str().unwrap_or("gone") {
    "leased" | "open" => "leased by another holder".to_owned(),
    state => state.to_owned(),
  }
}

/// An item moved to done by a caller other than its orderer.
pub fn closed(item: &Value, by: &str, orderer: &Orderer) -> Value {
  let id = id_of(item);
  let number = id.to_string();
  let order =
    fill_marks(&JOB_NOTICES.closed_order, &[("item", &number), ("title", title_of(item)), ("by", by)]);

  instruction("item-closed", id, orderer, format!("Item #{id} was closed by {by}"), order)
}

/// A held item whose every item it waits on has ended, `label` the label naming them and `waited` each of
/// them as the job service answers it (docs/agent-control.md, 「An item's change is told to its orderer」).
pub fn wait_ended(item: i64, title: &str, label: &str, waited: &[Value], orderer: &Orderer) -> Value {
  let number = item.to_string();
  let waited = waited
    .iter()
    .map(|other| {
      let state = other["state"].as_str().unwrap_or_default();
      let state = match other["labels"][MACHINE_TURN.close_label.as_str()].as_str() {
        Some(phase) => format!("{state}, {} {phase}", MACHINE_TURN.close_label),
        None => state.to_owned(),
      };

      fill_marks(
        &JOB_NOTICES.waited_item,
        &[("item", &id_of(other).to_string()), ("title", title_of(other)), ("state", &state)],
      )
    })
    .collect::<Vec<_>>()
    .join("; ");
  let order = fill_marks(
    &JOB_NOTICES.wait_ended_order,
    &[("item", &number), ("title", title), ("label", label), ("waited", &waited)],
  );

  instruction("item-wait-ended", item, orderer, format!("Item #{item} waits on nothing more"), order)
}

/// The commits an item's closes merged, each waiting for its repository's batch deployment, `waiting` the closes
/// that wait (docs/agent-control.md, 「An item's change is told to its orderer」). A close that names no repository
/// is of the item's own.
pub fn awaits_deploy(item: &Value, waiting: &[Close], orderer: &Orderer) -> Value {
  let id = id_of(item);
  let number = id.to_string();
  let commits: Vec<&str> = waiting.iter().filter_map(Close::waiting).collect();
  let merged = waiting
    .iter()
    .filter_map(|close| {
      let repository = close
        .repository
        .as_deref()
        .or_else(|| item["assigneeName"].as_str())
        .unwrap_or("the host");

      Some(fill_marks(
        &JOB_NOTICES.merged_commit,
        &[("commit", close.waiting()?), ("repository", repository)],
      ))
    })
    .collect::<Vec<_>>()
    .join("; ");
  let order = fill_marks(
    &JOB_NOTICES.deploy_order,
    &[("item", &number), ("title", title_of(item)), ("merged", &merged)],
  );

  instruction(
    "item-awaits-deploy",
    id,
    orderer,
    format!("Item #{id} waits to go live: {}", commits.join(", ")),
    order,
  )
}

/// A deployment that ended without going live (docs/agent-control.md, 「A deployment that did not go live is
/// told」), for the orderer of the waiting `items` it carries, with the last lines of its unit's `journal`: an order
/// under the sort `job` of its own kind for the run's unit, an event for each item it carries, or one naming none.
pub fn deploy_failed(
  failure: &crate::watches::deploy_failures::Failure,
  items: &[Value],
  journal: &[String],
  orderer: &Orderer,
) -> Value {
  let event = crate::watches::deploy_failures::FAILED_EVENT;
  let listed = match items.is_empty() {
    true => JOB_NOTICES.deploy_failed_no_items.clone(),
    false => items
      .iter()
      .map(|item| {
        fill_marks(
          &JOB_NOTICES.deploy_failed_item,
          &[("item", &id_of(item).to_string()), ("title", title_of(item))],
        )
      })
      .collect::<Vec<_>>()
      .join("; "),
  };
  let commit = failure.commit.get(..12).unwrap_or(&failure.commit);
  let order = fill_marks(
    &JOB_NOTICES.deploy_failed_order,
    &[
      ("repository", &failure.repository),
      ("commit", commit),
      ("unit", &failure.unit),
      ("outcome", &failure.outcome),
      ("stage", failure.stage.as_deref().unwrap_or(&JOB_NOTICES.deploy_no_stage)),
      ("reason", failure.reason.as_deref().unwrap_or(&JOB_NOTICES.deploy_no_reason)),
      ("items", &listed),
      ("journal", &journal.join("\n")),
    ],
  );
  let events: Vec<String> = match items.is_empty() {
    true => vec![event.to_owned()],
    false => items.iter().map(|item| format!("{event}:#{}", id_of(item))).collect(),
  };

  json!({
    "room": orderer.room,
    "said": { "order": order },
    "agentName": Value::Null,
    "sort": "job",
    "kind": format!("{event}:{}", failure.unit),
    "summary": format!("The deployment of {} at {commit} did not go live", failure.repository),
    "events": events,
    "run": Value::Null,
  })
}

/// A deployment the deploy queue's flush started that went live (docs/agent-control.md, 「The deploy queue is
/// flushed every hour」), for the orderer of the waiting `items` it carries: an order under the sort `job` of its own
/// kind for the run's unit, an event for each item it carries.
pub fn deploy_live(
  repository: &str,
  commit: &str,
  unit: &str,
  items: &[Value],
  orderer: &Orderer,
) -> Value {
  let event = "deploy-live";
  let listed = items
    .iter()
    .map(|item| {
      fill_marks(
        &JOB_NOTICES.deploy_failed_item,
        &[("item", &id_of(item).to_string()), ("title", title_of(item))],
      )
    })
    .collect::<Vec<_>>()
    .join("; ");
  let commit = commit.get(..12).unwrap_or(commit);
  let order = fill_marks(
    &JOB_NOTICES.deploy_live_order,
    &[("repository", repository), ("commit", commit), ("unit", unit), ("items", &listed)],
  );
  let events: Vec<String> = items.iter().map(|item| format!("{event}:#{}", id_of(item))).collect();

  json!({
    "room": orderer.room,
    "said": { "order": order },
    "agentName": Value::Null,
    "sort": "job",
    "kind": format!("{event}:{unit}"),
    "summary": format!("The hourly flush put {repository} live at {commit}"),
    "events": events,
    "run": Value::Null,
  })
}

/// Commits waiting for `repository`'s batch deployment that are not going live (docs/agent-control.md, 「The deploy
/// queue is flushed every hour」), for the orderer in `room` of the waiting `items`: an order under the sort `job` of
/// the kind `event` for the repository, an event for each item, its summary `summary`.
pub fn deploy_stalled(
  event: &str,
  repository: &str,
  summary: &str,
  order: &str,
  items: &[i64],
  room: Option<i64>,
) -> Value {
  json!({
    "room": room,
    "said": { "order": order },
    "agentName": Value::Null,
    "sort": "job",
    "kind": format!("{event}:{repository}"),
    "summary": summary,
    "events": items.iter().map(|item| format!("{event}:#{item}")).collect::<Vec<_>>(),
    "run": Value::Null,
  })
}

/// A run's question, for the orderer to put to the owner.
pub fn question(question: &Value, orderer: &Orderer) -> Value {
  question_told(question, orderer, &JOB_NOTICES.question_order)
}

/// A job's question the job service put to the owner once the supervisor's minute on it passed, told to the
/// orderer under the same key as a question asked, so a settling reply withdraws it alike.
pub fn question_handed(question: &Value, orderer: &Orderer) -> Value {
  question_told(question, orderer, &JOB_NOTICES.question_handed_order)
}

fn question_told(question: &Value, orderer: &Orderer, words: &str) -> Value {
  let id = id_of(question);
  let number = id.to_string();
  let item = question["itemId"]
    .as_i64()
    .map(|item| item.to_string())
    .unwrap_or_else(|| "none".to_owned());
  let choices = choices_of(question);
  let order = fill_marks(
    words,
    &[
      ("question", &number),
      ("item", &item),
      ("text", question["text"].as_str().unwrap_or_default()),
      ("choices", if choices.is_empty() { "an open answer" } else { &choices }),
    ],
  );

  instruction("question-awaits-answer", id, orderer, format!("Question #{id} awaits the owner"), order)
}

fn choices_of(question: &Value) -> String {
  question["choices"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|choice| {
      choice
        .as_str()
        .map(str::to_owned)
        .or_else(|| choice["label"].as_str().map(str::to_owned))
    })
    .collect::<Vec<_>>()
    .join(" / ")
}

/// A job waiting for a road or a slot, with why.
pub fn waiting(item: i64, worker: &str, why: &str, orderer: &Orderer) -> Value {
  let number = item.to_string();
  let order =
    fill_marks(&JOB_NOTICES.waiting_order, &[("item", &number), ("worker", worker), ("why", why)]);

  instruction("job-waiting", item, orderer, format!("The job of item #{item} waits"), order)
}

/// An ended job's item still leased, not closed.
pub fn awaits_close(item: i64, lease: i64, orderer: &Orderer) -> Value {
  let number = item.to_string();
  let lease = lease.to_string();
  let order = fill_marks(&JOB_NOTICES.close_order, &[("item", &number), ("lease", &lease)]);

  instruction("job-awaits-close", item, orderer, format!("Item #{item} awaits its close"), order)
}

/// A person's question still open past its due time, told to the orderer of the item it was asked on.
pub fn question_due(question: &Value, orderer: &Orderer) -> Value {
  let id = id_of(question);
  let number = id.to_string();
  let item = question["itemId"].as_i64().map_or_else(|| "none".to_owned(), |item| item.to_string());
  let choices = choices_of(question);
  let order = fill_marks(
    &JOB_NOTICES.question_due_order,
    &[
      ("question", &number),
      ("item", &item),
      ("asker", question["askerName"].as_str().unwrap_or("a run")),
      ("text", question["text"].as_str().unwrap_or_default()),
      ("choices", if choices.is_empty() { "an open answer" } else { &choices }),
    ],
  );

  instruction("job-question-due", id, orderer, format!("Question #{id} is past its due time"), order)
}

/// A question its due time passed on with no answer, settled on its recommendation, told to the orderer of the
/// item it was asked on: the settling reply's answer, its `choice` where it named one of the choices and its
/// `text` otherwise, and the recommendation's reason; the item as the question names it, none where the service
/// no longer holds it.
pub fn question_settled(reply: &Value, question: Option<&Value>, orderer: &Orderer) -> Value {
  let id = reply["questionId"].as_i64().unwrap_or_default();
  let number = id.to_string();
  let answer = match reply["choice"].as_str() {
    Some(choice) if !choice.is_empty() => choice.to_owned(),
    _ => reply["text"].as_str().unwrap_or_default().to_owned(),
  };
  let item = question
    .and_then(|question| question["itemId"].as_i64())
    .map(|item| fill_marks(&JOB_NOTICES.question_settled_item, &[("item", &item.to_string())]))
    .unwrap_or_default();
  let order = fill_marks(
    &JOB_NOTICES.question_settled_order,
    &[
      ("question", &number),
      ("item", &item),
      ("answer", &answer),
      ("why", reply["why"].as_str().unwrap_or_default()),
    ],
  );

  instruction(
    "job-question-settled",
    id,
    orderer,
    format!("Question #{id} was settled on its recommendation"),
    order,
  )
}

/// Whether a job's end is a failure the owner's private room is told of as well: its outcome `failed` or
/// `blocked`, its orderer a room other than the owner's private room.
pub fn owner_hears_failure(member: &Value, orderer: &Orderer) -> bool {
  orderer.room.is_some() && matches!(member["outcome"].as_str(), Some("failed" | "blocked"))
}

/// A job that failed in a room other than the owner's, told in the owner's private room.
pub fn failed(member: &Value, room: Option<i64>) -> Value {
  let item = member["itemId"].as_i64().unwrap_or_default();
  let order = fill_marks(
    &JOB_NOTICES.failed_order,
    &[
      ("job", &member["id"].as_i64().unwrap_or_default().to_string()),
      ("item", &item.to_string()),
      ("worker", member["workerName"].as_str().unwrap_or_default()),
      ("outcome", member["outcome"].as_str().unwrap_or_default()),
      ("room", &room.map_or_else(|| "none".to_owned(), |room| room.to_string())),
    ],
  );

  instruction(
    "job-failed",
    item,
    &Orderer::default(),
    format!("The job of item #{item} failed"),
    order,
  )
}

/// A job's wait owed to its item's orderer as it begins (「A job's wait is told as it begins」), the orderer read as
/// a job's end reads it, so a job service that does not answer sends it to the room the job was launched for; a
/// deployment naming no job service or no receiver tells nothing, and a notice the store would not take is said
/// and fails nothing.
pub async fn tell_waiting(
  db: &sea_orm::DatabaseConnection,
  vars: &BTreeMap<String, String>,
  member: &Value,
  why: &str,
) {
  let (Some(_), Some(_)) = (JobService::of(vars), outbox::Receiver::of(vars)) else {
    return;
  };
  let item = member["itemId"].as_i64().unwrap_or_default();
  let worker = member["workerName"].as_str().unwrap_or_default();

  if item <= 0 {
    return;
  }

  let orderer = crate::job_end::orderer(vars, member).await;

  match outbox::owe(db, vars, Notice::instruction(waiting(item, worker, why, &orderer), Keep::Once)).await {
    Ok(()) => {
      println!("job of item #{item}: its wait owed to its orderer");

      if let Err(why) = store::run(
        db,
        r#"UPDATE "jobs" SET "wait_unowed_at" = NULL WHERE "id" = $1"#,
        vec![member["id"].as_i64().unwrap_or_default().into()],
      )
      .await
      {
        eprintln!("job of item #{item}: its wait was not marked owed: {}", why.detail);
      }
    }
    Err(why) => eprintln!("job of item #{item}: its wait was not owed: {why}"),
  }
}

/// The waits no notice is owed for yet, owed (docs/agent-control.md, 「An item's change is told to its orderer」):
/// each job still waiting, of an item, whose wait began at least `owe_grace_seconds` ago and was not owed — a stop
/// of agency's cut it off, or the store refused it. Answers how many were owed.
pub async fn unowed_waits(
  db: &sea_orm::DatabaseConnection,
  vars: &BTreeMap<String, String>,
) -> Result<usize, String> {
  if JobService::of(vars).is_none() || outbox::Receiver::of(vars).is_none() {
    return Ok(0);
  }

  let grace = i64::try_from(JOB_NOTICES.owe_grace_seconds * 1000).unwrap_or(i64::MAX / 2);
  let rows = store::all(
    db,
    r#"SELECT "id", "held_for" FROM "jobs" WHERE "held_for" IS NOT NULL AND "wait_unowed_at" <= $1
         AND "item_id" > 0 AND "state" IN ('starting', 'running', 'reviewing')
       ORDER BY "id""#,
    vec![(crate::now_ms() - grace).into()],
  )
  .await
  .map_err(|why| why.detail)?;
  let mut owed = 0;

  for row in rows {
    let id: i64 = row.try_get("", "id").map_err(|why| why.to_string())?;
    let why: String = row.try_get("", "held_for").map_err(|why| why.to_string())?;
    let (member, _) = crate::control::work::job(db, id).await.map_err(|why| why.error)?;

    tell_waiting(db, vars, &member, &why).await;
    println!("job {id}: its wait owed, no notice of it having been written");
    owed += 1;
  }

  Ok(owed)
}
