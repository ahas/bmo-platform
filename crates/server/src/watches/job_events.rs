//! The job service's events an item's orderer and the owner are told of (docs/agent-control.md, 「An item's
//! change is told to its orderer」): read as a page past the cursor agency keeps, each event's notice owed in
//! agency's outbox — a close that handed its commit over and waits on its deployment, told once for each commit
//! and kept until it no longer waits, an item closed by somebody other than its orderer, a run's question asked,
//! a job's question the job service put to the owner once the supervisor's minute passed, a question past its due
//! time, a settling reply that withdraws the question's instruction, and one the job
//! service gave on the question's recommendation, told to the owner — and a note
//! added to the item of a job that runs, handed to that job (「Words for a running job」); and the title of an item
//! raised or changed, asked of the gate so a worker reads it translated (docs/worker-language.md). The cursor advances
//! only past events whose notices are owed and whose notes are kept. A pass with no cursor, or one whose cursor
//! left the service's window, reads to the end and tells nothing of what it passed but a commit waiting to go
//! live, owed or let go by the last event of each item it passed, since that notice stands on the item's state
//! rather than on its change.

use std::collections::BTreeMap;
use std::sync::Arc;

use sea_orm::DatabaseConnection;
use serde_json::Value;

use crate::job_notices;
use crate::outbox::{self, Keep, Notice, Receiver};
use crate::peers::gate::Gate;
use crate::queue::orderer::{JobService, Orderer};
use crate::queue::settings;
use crate::watches::closes::{self, Close};

/// The settings row the number of the last event read stands in.
pub const CURSOR_KEY: &str = "job_events_after";
/// The prefix of the key a run's question is owed under, its number after it.
pub const QUESTION_OWED: &str = "question-awaits-answer:";
/// Who a question carries as having put it to a person where the job service did, the supervisor's minute on it
/// having passed (the job service's docs/api.md, 「When the due time passes」).
pub const HANDED_BY: &str = "due time";
/// The prefix of the key a commit waiting to go live is owed under, its item's number after it.
pub const DEPLOY_OWED: &str = "item-awaits-deploy:";
/// The kinds read.
pub const KINDS: &str =
  "item.created,item.changed,note.created,question.created,question.changed,question.due,reply.created";

// A page of events past `after`, and whether more stand; none where the cursor left the service's window.
async fn page(service: &JobService, after: Option<i64>) -> Result<Option<(Vec<Value>, bool)>, String> {
  let mut query = vec![("kinds", KINDS.to_owned()), ("limit", "500".to_owned())];

  // A cursor of nothing reads from the oldest event the window holds.
  if let Some(after) = after.filter(|after| *after > 0) {
    query.push(("after", after.to_string()));
  }

  let answer = reqwest::Client::new()
    .get(format!("{}/events", service.url))
    .header(reqwest::header::ACCEPT, "application/json")
    .query(&query)
    .bearer_auth(&service.token)
    .send()
    .await
    .map_err(|why| why.to_string())?;

  if answer.status() == reqwest::StatusCode::GONE {
    return Ok(None);
  }

  if !answer.status().is_success() {
    return Err(format!("the job service answered {} to /events", answer.status().as_u16()));
  }

  let page: Value = answer.json().await.map_err(|why| why.to_string())?;

  Ok(Some((page["items"].as_array().cloned().unwrap_or_default(), page["more"] == true)))
}

// The cursor kept, so neither a restart nor the next pass reads an event twice.
async fn keep(db: &DatabaseConnection, at: i64) -> Result<(), String> {
  settings::set(db, CURSOR_KEY, &at.to_string()).await.map_err(|why| why.detail)
}

/// One pass over the job service's events past the cursor, each one's notice owed; nothing where the deployment
/// names no job service or no receiver. An event whose notice could not be owed ends the pass there, the cursor
/// kept at the event before it, so the next pass takes it again. A stretch read without telling keeps its cursor
/// only once the commits waiting to go live it passed are owed or let go.
pub async fn pass(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  gate: &Arc<dyn Gate>,
) -> Result<(), String> {
  let (Some(service), Some(_)) = (JobService::of(vars), Receiver::of(vars)) else {
    return Ok(());
  };
  let kept = settings::get(db, CURSOR_KEY)
    .await
    .map_err(|why| why.detail)?
    .and_then(|value| value.trim().parse::<i64>().ok());
  let mut telling = kept.is_some();
  let mut after = kept;
  // The last event of each item a stretch read without telling passed.
  let mut passed: BTreeMap<i64, Value> = BTreeMap::new();

  loop {
    let Some((events, more)) = page(&service, after).await? else {
      if after.is_none_or(|after| after <= 0) {
        return Err("the job service answered 410 to a read from its oldest event".to_owned());
      }

      // The cursor left the window: read from the oldest again, telling nothing of what is passed.
      telling = false;
      after = None;
      continue;
    };

    for event in &events {
      let Some(id) = event["id"].as_i64() else { continue };

      if !telling {
        if event["kind"] == "item.changed"
          && let Some(item) = event["data"]["id"].as_i64()
        {
          passed.insert(item, event["data"].clone());
        }
      } else if let Err(why) = told(db, vars, gate, &service, event).await {
        if let Some(last) = after {
          keep(db, last).await?;
        }

        return Err(format!("event {id} was not owed, so the cursor stays before it: {why}"));
      }

      after = Some(id);
    }

    if telling && let Some(last) = after {
      keep(db, last).await?;
    }

    if !more {
      break;
    }
  }

  for data in passed.values() {
    deploy_told(db, vars, &service, data)
      .await
      .map_err(|why| format!("a commit waiting to go live was not owed, so the cursor stays: {why}"))?;
  }

  // Where no event stands yet, the cursor starts at nothing, so the next pass tells what comes.
  keep(db, after.unwrap_or(0)).await?;

  Ok(())
}

// Whether the item's own orderer closed it: its changer is the requester of the item the orderer was found on,
// or a supervisor agency holds.
async fn by_orderer(db: &DatabaseConnection, by: &str, orderer: &Orderer) -> bool {
  orderer.requester.as_deref() == Some(by)
    || crate::supervisors::named(db, by).await.ok().flatten().is_some()
}

// What an owed notice of waiting commits is marked with: each commit, after its repository where its close names
// one, so a commit going live or another one waiting owes the notice afresh.
fn waiting_mark(waiting: &[Close]) -> String {
  waiting
    .iter()
    .filter_map(|close| {
      let commit = close.waiting()?;

      Some(
        close
          .repository
          .as_ref()
          .map_or_else(|| commit.to_owned(), |repository| format!("{repository}={commit}")),
      )
    })
    .collect::<Vec<_>>()
    .join(",")
}

// The commits an item's closes merged that wait to go live owed to the item's orderer once for that set of commits,
// kept rather than told again while the item waits on them; and let go, its copy withdrawn, once none waits. A
// close that merged its commit leaves the item waiting for its repository's batch deployment, an older close
// leaves it waiting to go live, and the orderer puts each repository live.
async fn deploy_told(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  service: &JobService,
  data: &Value,
) -> Result<(), String> {
  let Some(id) = data["id"].as_i64() else { return Ok(()) };
  let key = format!("{DEPLOY_OWED}{id}");
  let owed = outbox::owed(db, &key).await?.remove("");
  // An item whose notices are switched off is told of no commit waiting (docs/agent-control.md, 「An item's notices
  // switched off」): its kept copy is withdrawn, and it is owed afresh by the event that switches them on again.
  let waiting: Vec<Close> = if data["state"] == "done" || crate::watches::queue_idle::notices_off(data) {
    vec![]
  } else {
    closes::of(&data["labels"])
      .into_iter()
      .filter(|close| close.waiting().is_some())
      .collect()
  };
  let mark = waiting_mark(&waiting);

  match (waiting.is_empty(), owed) {
    (false, owed) if owed.as_ref().is_none_or(|owed| owed.mark != mark) => {
      let orderer = service.orderer_of(data).await?;
      let notice = Notice::instruction(job_notices::awaits_deploy(data, &waiting, &orderer), Keep::Kept);

      outbox::owe(db, vars, notice.marked(&mark)).await?;
      println!("queue: item #{id}'s commits {mark} wait to go live, owed to its orderer");
    }
    (true, Some(owed)) => outbox::let_go_as_read(db, vars, &key, &owed).await?,
    _ => {}
  }

  Ok(())
}

// The title of the item an event carries asked of the gate ahead of any job, in a task nothing waits on.
fn title_asked(gate: &Arc<dyn Gate>, data: &Value) {
  if let Some(title) = data["title"].as_str() {
    crate::worker_english::ask_ahead(gate.clone(), title.to_owned());
  }
}

// One event's notice owed to whom it calls for.
async fn told(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  gate: &Arc<dyn Gate>,
  service: &JobService,
  event: &Value,
) -> Result<(), String> {
  let data = &event["data"];

  match event["kind"].as_str().unwrap_or_default() {
    // An item's title is asked of the gate as the item is raised, so a worker reads it in its own language
    // (docs/worker-language.md, 「What agency translates on the way in」).
    "item.created" => title_asked(gate, data),
    "item.changed" => {
      // A change that left the item's state where it was may be a new title; one kept already asks nothing more
      // of the translator.
      if data["previousState"].is_null() {
        title_asked(gate, data);
      }

      deploy_told(db, vars, service, data).await?;

      let moved_to_done =
        data["state"] == "done" && data["previousState"].as_str().is_some_and(|was| was != "done");

      if !moved_to_done {
        return Ok(());
      }

      let by = data["changedBy"].as_str().unwrap_or("the shared token");
      let orderer = service.orderer_of(data).await?;

      if by_orderer(db, by, &orderer).await {
        return Ok(());
      }

      outbox::owe(db, vars, Notice::instruction(job_notices::closed(data, by, &orderer), Keep::Once))
        .await?;
      println!("queue: item #{} closed by {by}, owed to its orderer", data["id"]);
    }
    // A note on the item of a job that runs is the job's, kept for it and handed to its turn
    // (docs/agent-control.md, 「Words for a running job」); a note on any other item tells nobody.
    "note.created" => {
      if let Some(note) = crate::job_words::Note::of(data) {
        crate::job_words::note_added(db, gate, note).await?;
      }
    }
    "question.created" => {
      let Some(id) = data["id"].as_i64() else { return Ok(()) };

      // An asker that tells of its question itself has already put it, or closed it unsent.
      if data["toldByAsker"].as_bool().unwrap_or(false) {
        return Ok(());
      }

      let orderer = question_orderer(service, data).await?;

      outbox::owe(db, vars, Notice::instruction(job_notices::question(data, &orderer), Keep::Kept)).await?;
      println!("queue: question #{id} owed to its orderer");
    }
    // The job service put the question to the owner itself, the supervisor's minute on it having passed: told
    // once, by the change that moved it, never by a later change of the same question.
    "question.changed" => {
      let Some(id) = data["id"].as_i64() else { return Ok(()) };
      let handed = data["addressee"] == "person"
        && data["readdressedBy"] == HANDED_BY
        && data["readdressedAt"].as_i64().is_some_and(|at| Some(at) == event["at"].as_i64());

      if !handed {
        return Ok(());
      }

      let orderer = question_orderer(service, data).await?;

      outbox::owe(db, vars, Notice::instruction(job_notices::question_handed(data, &orderer), Keep::Kept))
        .await?;
      println!("queue: question #{id} put to the owner by its due time, owed to its orderer");
    }
    "question.due" => {
      let Some(question) = data["id"].as_i64() else { return Ok(()) };

      let orderer = question_orderer(service, data).await?;

      crate::job_review::question_due(db, vars, question).await?;
      outbox::owe(db, vars, Notice::instruction(job_notices::question_due(data, &orderer), Keep::Once))
        .await?;
      println!("queue: question #{question} past its due time, owed to its orderer");
    }
    "reply.created" => {
      let settles = data["settles"].as_bool().unwrap_or(false);
      let Some(question) = data["questionId"].as_i64().filter(|_| settles) else {
        return Ok(());
      };

      // A question settled on its recommendation went unanswered: its item and its orderer are read before
      // anything moves, so a read that fails leaves the event whole for the next pass, and the orderer's notice is
      // owed last, so taking the event again never tells it twice.
      let settled = if data["source"] == "recommendation" {
        let asked = service.question(question).await?;
        let orderer = match &asked {
          Some(asked) => question_orderer(service, asked).await?,
          None => Orderer::default(),
        };

        Some((asked, orderer))
      } else {
        None
      };

      crate::job_review::ruled(db, vars, data).await;
      outbox::let_go(db, vars, &format!("{QUESTION_OWED}{question}")).await?;
      println!("queue: question #{question} answered, its instruction let go");

      if let Some((asked, orderer)) = settled {
        let notice = job_notices::question_settled(data, asked.as_ref(), &orderer);

        outbox::owe(db, vars, Notice::instruction(notice, Keep::Once)).await?;
        println!("queue: question #{question} settled on its recommendation, owed to its orderer");
      }
    }
    _ => {}
  }

  Ok(())
}

/// The orderer of the item a question was asked on, the room the question names itself where the item names
/// none, and the owner's private room where neither does.
async fn question_orderer(service: &JobService, question: &Value) -> Result<Orderer, String> {
  let orderer = match question["itemId"].as_i64() {
    Some(item) => service.orderer_of_id(item).await?.1,
    None => Orderer {
      room: crate::queue::orderer::room_of(question),
      requester: None,
    },
  };

  Ok(Orderer {
    room: orderer.room.or_else(|| crate::queue::orderer::room_of(question)),
    ..orderer
  })
}
