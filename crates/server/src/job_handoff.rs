//! The handoff a job's session leaves as the job's work moves to another session (docs/sessions.md, 「The
//! handoff a job's session leaves」; docs/api.md, 「assignments and jobs」): the session's own words where it
//! could be asked, else the last lines the job's run logged; noted onto the job's item in the job service under
//! the handoff label; and the next session's prompt, or the prompt of a relaunch of the item, opened on it.

use policy::fill_marks;
use policy::job_handoff::JOB_HANDOFF;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};

use crate::places::Ground;
use crate::queue::orderer::JobService;
use store::{all, one};

/// Where a handoff's words came from, as its note's `source` label says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
  /// The session wrote it.
  Session,
  /// The session could not be asked, and its run log's last lines stand in its place.
  Log,
}

impl Source {
  fn word(self) -> &'static str {
    match self {
      Self::Session => "session",
      Self::Log => "log",
    }
  }
}

/// A handoff: its words and where they came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handoff {
  pub text: String,
  pub source: Source,
}

/// The handoff a session's answer gives: none where it answered nothing, or the word `none`.
pub fn answered(answer: Option<&str>) -> Option<Handoff> {
  let text = answer.map(str::trim).filter(|text| !text.is_empty())?;

  if text.trim_matches(|c: char| !c.is_alphanumeric()).eq_ignore_ascii_case("none") {
    return None;
  }

  Some(Handoff {
    text: text.to_owned(),
    source: Source::Session,
  })
}

/// The handoff of a session that could not be asked, `why`, written from the last lines the job's run logged,
/// oldest first.
pub async fn from_log(db: &DatabaseConnection, job: i64, why: &str) -> Handoff {
  let rows = all(
    db,
    r#"SELECT "kind", "text" FROM "agent_log"
       WHERE "job_id" = $1 AND "kind" <> 'prompt' AND "text" <> '' ORDER BY "id" DESC LIMIT $2"#,
    vec![job.into(), (JOB_HANDOFF.log_lines as i64).into()],
  )
  .await
  .unwrap_or_default();
  let mut lines: Vec<String> = rows
    .iter()
    .filter_map(|row| {
      let kind: String = row.try_get("", "kind").ok()?;
      let text: String = row.try_get("", "text").ok()?;

      Some(format!("- {kind}: {}", cut(text.trim(), JOB_HANDOFF.line_chars)))
    })
    .collect();

  lines.reverse();

  Handoff {
    text: fill_marks(&JOB_HANDOFF.from_log, &[("why", why), ("lines", &lines.join("\n"))]),
    source: Source::Log,
  }
}

// The first `most` characters of a line, said to be cut where it was.
fn cut(text: &str, most: usize) -> String {
  let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");

  match one_line.char_indices().nth(most) {
    Some((at, _)) => format!("{} …", &one_line[..at]),
    None => one_line,
  }
}

/// The handoff noted onto the job's item in the job service under the handoff label; a note the service does
/// not take, or a deployment that names no job service, is said and stops nothing.
pub async fn noted(ground: &Ground, job: i64, item: i64, handoff: &Handoff) {
  let Some(service) = JobService::of(ground.vars()) else {
    return;
  };
  let note = json!({
    "text": handoff.text,
    "actorName": JOB_HANDOFF.actor,
    "labels": { "kind": JOB_HANDOFF.kind, "job": job.to_string(), "source": handoff.source.word() },
  });

  match crate::job_review::call(
    &service,
    reqwest::Method::POST,
    &format!("/items/{item}/notes"),
    Some(&note),
  )
  .await
  {
    Ok(_) => println!("job {job}: its handoff was noted on item {item} ({})", handoff.source.word()),
    Err(why) => eprintln!("job {job}: its handoff was not noted on item {item}: {why}"),
  }
}

/// The handoff of a job whose road stopped answering under its turn, so its session cannot be asked: written
/// from its run log and noted on its item, for the turn on the next road or login to open on.
pub async fn road_stopped(ground: &Ground, job: i64) -> Handoff {
  let handoff = from_log(&ground.db, job, &JOB_HANDOFF.why.road_stopped).await;
  let item = one(&ground.db, r#"SELECT "item_id" FROM "jobs" WHERE "id" = $1"#, vec![job.into()])
    .await
    .ok()
    .flatten()
    .and_then(|row| row.try_get::<i64>("", "item_id").ok());

  if let Some(item) = item {
    noted(ground, job, item, &handoff).await;
  }

  handoff
}

/// Whether a note of an item is a handoff.
pub fn is_handoff(note: &Value) -> bool {
  note["labels"]["kind"].as_str() == Some(JOB_HANDOFF.kind.as_str())
}

/// The handoff a launch of `item` opens on: the newest handoff note the job service answers for the item,
/// unless a job of the item ended answered after it was written; none where the deployment names no job
/// service or the read fails.
pub async fn of_item(ground: &Ground, item: i64) -> Option<String> {
  let service = JobService::of(ground.vars())?;
  let mut newest: Option<Value> = None;
  let mut after: Option<i64> = None;

  loop {
    let path = match after {
      Some(after) => format!("/items/{item}/notes?limit=500&after={after}"),
      None => format!("/items/{item}/notes?limit=500"),
    };
    let page = match crate::job_review::call(&service, reqwest::Method::GET, &path, None).await {
      Ok(page) => page,
      Err(why) => {
        eprintln!("item {item}: its notes were not read for a handoff: {why}");
        return None;
      }
    };
    let notes = page["items"].as_array().cloned().unwrap_or_default();

    if let Some(note) = notes.iter().rev().find(|note| is_handoff(note)) {
      newest = Some(note.clone());
    }

    match (page["more"].as_bool(), notes.last().and_then(|note| note["id"].as_i64())) {
      (Some(true), Some(last)) => after = Some(last),
      _ => break,
    }
  }

  let note = newest?;
  let written = moment(&note["createdAt"]).unwrap_or_default();
  let answered_since = one(
    &ground.db,
    r#"SELECT 1 AS "found" FROM "jobs"
       WHERE "item_id" = $1 AND "state" = 'ended' AND "ended_at" > $2 LIMIT 1"#,
    vec![item.into(), written.into()],
  )
  .await
  .ok()
  .flatten()
  .is_some();

  if answered_since {
    return None;
  }

  note["text"].as_str().map(str::trim).filter(|text| !text.is_empty()).map(str::to_owned)
}

// A moment the job service answers, as epoch milliseconds or an RFC 3339 time.
fn moment(value: &Value) -> Option<i64> {
  value.as_i64().or_else(|| {
    value
      .as_str()
      .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
      .map(|time| time.timestamp_millis())
  })
}

/// A prompt opened on a handoff: ahead of everything but the item's title, which stays the prompt's first line
/// where it stands there.
pub fn ahead(handoff: &str, prompt: &str) -> String {
  let (title, rest) = match prompt.split_once('\n') {
    Some((first, rest)) if first.starts_with("# ") => (Some(first), rest.trim_start_matches('\n')),
    _ => (None, prompt),
  };
  let opened = fill_marks(&JOB_HANDOFF.opened, &[("handoff", handoff), ("prompt", rest)]);

  match title {
    Some(title) => format!("{title}\n\n{opened}"),
    None => opened,
  }
}
