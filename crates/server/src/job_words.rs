//! Words for a running job (docs/agent-control.md, 「Words for a running job」): a job addressed by itself or by
//! any session a turn of it started; every word for it, said to it or noted on its item, kept in the store
//! before anything is tried; the turn the job works now offered them at its next look, this process's or one
//! picked up across a restart; and any later turn of the job carrying what is still kept behind its own prompt.
//! A word is marked handed only once a turn of the job took it, so none is lost to a turn's end, a review
//! between stages or a restart. A word is kept as said and goes into a turn in the worker's language
//! (docs/worker-language.md, 「What agency translates on the way in」).

use std::collections::HashSet;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use policy::fill_marks;
use policy::worker_words::WORKER_WORDS;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, QueryResult};
use serde_json::Value;
use store::{Fault, all, one, run};

use crate::door::aside;
use crate::door::turn::AsideOutcome;
use crate::now_ms;
use crate::peers::gate::Gate;
use crate::run_log::lines::Line;
use crate::run_log::{RunLog, Target};
use crate::turn_pickup::{self, InFlight};

/// The states a job takes words in: from its launch to its end.
pub const RUNNING: [&str; 3] = ["starting", "running", "reviewing"];

/// The actor the job service writes its own records of an item under.
const SERVICE_ACTOR: &str = "bmo-job";

/// The line a word and what became of it stand as in the job's log, as every word said into a run does.
const WORD_LINE: &str = "word";

/// The columns a job is read by.
const COLUMNS: &str = r#"j."id", j."item_id", j."worker_name", j."item_agent", j."holder_name", j."room_id",
  j."state", j."session_id", j."run_name", j."started_at", j."notes_through""#;

/// One job as its words are kept and handed.
#[derive(Clone, Debug)]
pub struct Job {
  pub id: i64,
  pub item: i64,
  pub worker: String,
  pub agent: Option<String>,
  pub holder: String,
  pub room: Option<i64>,
  pub state: String,
  pub session: Option<String>,
  run: Option<String>,
  started_at: i64,
  notes_through: Option<i64>,
}

impl Job {
  fn of(row: &QueryResult) -> Result<Self, DbErr> {
    Ok(Self {
      id: row.try_get("", "id")?,
      item: row.try_get("", "item_id")?,
      worker: row.try_get("", "worker_name")?,
      agent: row.try_get("", "item_agent")?,
      holder: row.try_get("", "holder_name")?,
      room: row.try_get("", "room_id")?,
      state: row.try_get("", "state")?,
      session: row.try_get("", "session_id")?,
      run: row.try_get("", "run_name")?,
      started_at: row.try_get("", "started_at")?,
      notes_through: row.try_get("", "notes_through")?,
    })
  }

  /// Whether the job still runs, so words for it are kept rather than refused as ended.
  pub fn runs(&self) -> bool {
    RUNNING.contains(&self.state.as_str())
  }

  /// The run the job's lines stand under where no turn of it works now.
  fn own_run(&self) -> String {
    self.run.clone().unwrap_or_else(|| format!("job-{}", self.id))
  }

  /// Whether a note is one the job is not handed: written by its own turn, the job service's own record,
  /// carried by the prompt it was launched with, or a handoff, which the turn that goes on opens with already.
  fn passes_over(&self, note: &Note) -> bool {
    let own = note.actor == self.worker || self.agent.as_deref() == Some(note.actor.as_str());
    let carried = match self.notes_through {
      Some(through) => note.id <= through,
      None => note.created_at < self.started_at,
    };

    own || note.actor == SERVICE_ACTOR || carried || note.handoff
  }
}

async fn job_where(
  db: &impl ConnectionTrait,
  condition: &str,
  values: Vec<sea_orm::Value>,
) -> Result<Option<Job>, Fault> {
  let row = one(
    db,
    &format!(r#"SELECT {COLUMNS} FROM "jobs" j WHERE {condition} ORDER BY j."id" DESC LIMIT 1"#),
    values,
  )
  .await?;

  Ok(row.as_ref().map(Job::of).transpose()?)
}

/// One job by its id.
pub async fn named(db: &impl ConnectionTrait, id: i64) -> Result<Option<Job>, Fault> {
  job_where(db, r#"j."id" = $1"#, vec![id.into()]).await
}

/// The job a session id names: the session its turn works in now, or one an earlier turn of it started. A job
/// that runs is named before one that has ended, the newest first, since a process before this one may have
/// given an ended job's session the same id.
pub async fn of_session(db: &impl ConnectionTrait, session: &str) -> Result<Option<Job>, Fault> {
  let row = one(
    db,
    &format!(
      r#"SELECT {COLUMNS} FROM "jobs" j
         WHERE j."session_id" = $1
            OR EXISTS (SELECT 1 FROM "job_sessions" s WHERE s."session_id" = $1 AND s."job_id" = j."id")
         ORDER BY j."state" IN ('starting', 'running', 'reviewing') DESC, j."id" DESC LIMIT 1"#
    ),
    vec![session.into()],
  )
  .await?;

  Ok(row.as_ref().map(Job::of).transpose()?)
}

/// The running job whose turn a session this process holds carries now, where one does.
pub async fn working_in(db: &impl ConnectionTrait, session: &str) -> Result<Option<Job>, Fault> {
  job_where(
    db,
    r#"j."session_id" = $1 AND j."state" IN ('starting', 'running', 'reviewing')"#,
    vec![session.into()],
  )
  .await
}

/// The job that runs on an item, where one does.
pub async fn running_on(db: &impl ConnectionTrait, item: i64) -> Result<Option<Job>, Fault> {
  job_where(
    db,
    r#"j."item_id" = $1 AND j."state" IN ('starting', 'running', 'reviewing')"#,
    vec![item.into()],
  )
  .await
}

/// A session a turn of the job started, recorded against the job so its id addresses the job until it ends.
pub async fn session_began(db: &impl ConnectionTrait, job: i64, session: &str) {
  let written = run(
    db,
    r#"INSERT INTO "job_sessions" ("session_id", "job_id", "started_at") VALUES ($1, $2, $3)
       ON CONFLICT ("session_id", "job_id") DO NOTHING"#,
    vec![session.into(), job.into(), now_ms().into()],
  )
  .await;

  if let Err(why) = written {
    eprintln!("job {job}: the session {session} was not recorded against it: {}", why.detail);
  }
}

/// The turn of the job going now, as it is kept in flight: this process's or one it picked up.
pub async fn turn_of(db: &DatabaseConnection, job: i64) -> Option<InFlight> {
  let turns = turn_pickup::in_flight(db).await.ok()?;

  turns
    .into_iter()
    .filter(|turn| turn.job == Some(job))
    .max_by_key(|turn| turn.started_at)
}

/// A word for the job kept until a turn of it takes it; none where a note it came from was kept already.
async fn keep(
  db: &impl ConnectionTrait,
  job: i64,
  text: &str,
  by: &str,
  note: Option<i64>,
) -> Result<Option<i64>, Fault> {
  let row = one(
    db,
    r#"INSERT INTO "job_words" ("job_id", "text", "said_by", "note_id", "said_at") VALUES ($1, $2, $3, $4, $5)
       ON CONFLICT ("job_id", "note_id") WHERE "note_id" IS NOT NULL DO NOTHING RETURNING "id""#,
    vec![job.into(), text.into(), by.into(), note.into(), now_ms().into()],
  )
  .await?;

  Ok(row.map(|row| row.try_get::<i64>("", "id")).transpose()?)
}

// The words kept for the job that no turn took yet, in the order they were kept.
async fn waiting(db: &impl ConnectionTrait, job: i64) -> Result<Vec<(i64, String)>, Fault> {
  let rows = all(
    db,
    r#"SELECT "id", "text" FROM "job_words" WHERE "job_id" = $1 AND "handed_at" IS NULL ORDER BY "id""#,
    vec![job.into()],
  )
  .await?;

  Ok(
    rows
      .iter()
      .map(|row| Ok((row.try_get("", "id")?, row.try_get("", "text")?)))
      .collect::<Result<_, DbErr>>()?,
  )
}

// The kept words offered to a working turn now, by this process: an offer may stand for as long as the turn
// does, so its claim is held here rather than in the store, and a restart meanwhile leaves the word kept.
static OFFERING: LazyLock<Mutex<HashSet<(i64, i64)>>> = LazyLock::new(Mutex::default);

// The words offered now, each by its job and its own id.
fn offering() -> MutexGuard<'static, HashSet<(i64, i64)>> {
  OFFERING.lock().unwrap_or_else(PoisonError::into_inner)
}

// A kept word marked handed, so no offer or turn takes it again; false where one already did.
async fn claim(db: &impl ConnectionTrait, word: i64) -> bool {
  run(
    db,
    r#"UPDATE "job_words" SET "handed_at" = $2 WHERE "id" = $1 AND "handed_at" IS NULL"#,
    vec![word.into(), now_ms().into()],
  )
  .await
  .is_ok_and(|changed| changed == 1)
}

// Kept words a turn did not take, left for the next.
async fn release(db: &impl ConnectionTrait, words: &[i64]) {
  for word in words {
    if let Err(why) =
      run(db, r#"UPDATE "job_words" SET "handed_at" = NULL WHERE "id" = $1"#, vec![(*word).into()]).await
    {
      eprintln!("job word {word}: it was not left for the job's next turn: {}", why.detail);
    }
  }
}

/// What became of one word said to a job.
#[derive(Clone, Debug)]
pub struct Said {
  /// What the turn working now answered, none where no turn of the job took the offer.
  pub outcome: Option<AsideOutcome>,
  /// The turn offered the words, where one of the job's works now.
  pub turn: Option<InFlight>,
  /// The run the word's lines stand under.
  pub run: String,
  /// The line written under it.
  pub note: String,
}

/// A word for a running job: kept, written to its log, and offered with every word still kept for it to the
/// turn it works now, the outcome answered being this word's own.
pub async fn say(
  db: &DatabaseConnection,
  gate: &dyn Gate,
  job: &Job,
  text: &str,
  by: &str,
  note: Option<i64>,
) -> Result<Option<Said>, Fault> {
  let Some(word) = keep(db, job.id, text, by, note).await? else {
    return Ok(None);
  };

  Ok(Some(offered_now(db, gate, job, word, text).await?))
}

// A word kept for the job written to its log and offered, with every word still kept before it, to the turn the
// job works now; the outcome answered is this word's own.
async fn offered_now(
  db: &DatabaseConnection,
  gate: &dyn Gate,
  job: &Job,
  word: i64,
  text: &str,
) -> Result<Said, Fault> {
  let turn = turn_of(db, job.id).await;
  let run = turn.as_ref().map_or_else(|| job.own_run(), |turn| turn.run.clone());

  line(db, job, &run, "prompt", text).await;

  let offered = match &turn {
    Some(turn) => offer(db, gate, job, turn).await?,
    None => vec![],
  };
  let outcome = offered.iter().find(|(id, _)| *id == word).map(|(_, outcome)| *outcome);
  let note = account(outcome);

  line(db, job, &run, "note", &note).await;

  Ok(Said {
    outcome,
    turn,
    run,
    note,
  })
}

/// The line written under a word for what became of it.
fn account(outcome: Option<AsideOutcome>) -> String {
  match outcome {
    Some(AsideOutcome::Delivered) => "The turn took the words.".to_owned(),
    Some(AsideOutcome::Waiting) => format!(
      "The turn waits on a person's answer, so the words stayed out. {}",
      WORKER_WORDS.job.kept
    ),
    Some(AsideOutcome::Refused) | None => WORKER_WORDS.job.kept.clone(),
  }
}

// Every word kept for the job offered in order to its working turn in the worker's language, until one is not
// taken: a word the turn took marked handed, and one it did not left kept. Answers each word offered with what
// became of it; a word another offer holds is passed.
async fn offer(
  db: &DatabaseConnection,
  gate: &dyn Gate,
  job: &Job,
  turn: &InFlight,
) -> Result<Vec<(i64, AsideOutcome)>, Fault> {
  let mut offered = vec![];

  for (word, text) in waiting(db, job.id).await? {
    if !offering().insert((job.id, word)) {
      continue;
    }

    let text = crate::worker_english::english_one(gate, &text).await;
    let outcome = match aside::offer(&turn.run, &text) {
      Some(mut offer) => Some((&mut offer.answer).await.unwrap_or(AsideOutcome::Refused)),
      None => None,
    };
    let taken = outcome == Some(AsideOutcome::Delivered) && claim(db, word).await;

    offering().remove(&(job.id, word));

    if let Some(outcome) = outcome {
      offered.push((word, outcome));
    }

    if !taken {
      break;
    }
  }

  Ok(offered)
}

/// The words kept for the job that a turn of it now carries, taken until the turn says whether it went out.
#[derive(Debug, Default)]
pub struct Carried {
  words: Vec<i64>,
  /// The paragraph the turn carries behind its own prompt, none where nothing was kept.
  pub text: Option<String>,
}

/// Every word still kept for the job, taken for the turn about to go out and put into the worker's language.
pub async fn carried(db: &impl ConnectionTrait, gate: &dyn Gate, job: i64) -> Carried {
  let mut words = vec![];
  let mut texts = vec![];

  for (word, text) in waiting(db, job).await.unwrap_or_default() {
    if !offering().contains(&(job, word)) && claim(db, word).await {
      words.push(word);
      texts.push(text);
    }
  }

  if words.is_empty() {
    return Carried::default();
  }

  let text = std::iter::once(WORKER_WORDS.job.waited.clone())
    .chain(crate::worker_english::english(gate, texts).await)
    .collect::<Vec<_>>()
    .join("\n\n");

  Carried {
    words,
    text: Some(text),
  }
}

impl Carried {
  /// The prompt a turn goes out with: its own, and behind it what was kept for the job.
  pub fn behind(&self, prompt: &str) -> String {
    match &self.text {
      Some(text) => format!("{prompt}\n\n{text}"),
      None => prompt.to_owned(),
    }
  }

  /// The turn did not carry them to the end, so they wait for the job's next turn.
  pub async fn left(self, db: &impl ConnectionTrait) {
    release(db, &self.words).await;
  }

  /// The turn carried them, said in the job's log under its run.
  pub async fn taken(self, db: &DatabaseConnection, job: i64, run: &str) {
    if self.words.is_empty() {
      return;
    }

    if let Ok(Some(job)) = named(db, job).await {
      let said = fill_marks(&WORKER_WORDS.job.carried, &[("count", &self.words.len().to_string())]);

      line(db, &job, run, "note", &said).await;
    }
  }
}

/// A note added to an item, as the job service's `note.created` event carries it.
#[derive(Clone, Debug)]
pub struct Note {
  id: i64,
  item: i64,
  actor: String,
  text: String,
  created_at: i64,
  // A handoff a job's session left (docs/sessions.md, 「The handoff a job's session leaves」).
  handoff: bool,
}

impl Note {
  /// The note an event's data names; none where it names no note.
  pub fn of(data: &Value) -> Option<Self> {
    Some(Self {
      id: data["id"].as_i64()?,
      item: data["itemId"].as_i64()?,
      actor: data["actorName"].as_str().unwrap_or_default().to_owned(),
      text: data["text"].as_str().filter(|text| !text.trim().is_empty())?.to_owned(),
      created_at: data["createdAt"].as_i64().unwrap_or_default(),
      handoff: crate::job_handoff::is_handoff(data),
    })
  }
}

/// A note added to an item kept for the job that runs on it, where one does and the note is not the job's own.
/// The note is kept before this answers, so the pass reading the events moves its cursor past it only once it
/// stands; a note kept already is kept once. The offer to the job's working turn goes on apart, so the pass never
/// waits on a turn's look.
pub async fn note_added(db: &DatabaseConnection, gate: &Arc<dyn Gate>, note: Note) -> Result<(), String> {
  let Some(job) = running_on(db, note.item).await.map_err(|why| why.detail)? else {
    return Ok(());
  };

  if job.passes_over(&note) {
    return Ok(());
  }

  let text = fill_marks(
    &WORKER_WORDS.job.note,
    &[("item", &note.item.to_string()), ("actor", &note.actor), ("note", note.text.trim())],
  );
  let Some(word) = keep(db, job.id, &text, &note.actor, Some(note.id)).await.map_err(|why| why.detail)?
  else {
    return Ok(());
  };
  let (db, gate) = (db.clone(), gate.clone());

  println!("queue: item #{}'s note {} kept for job {}", note.item, note.id, job.id);
  tokio::spawn(async move {
    if let Err(why) = offered_now(&db, gate.as_ref(), &job, word, &text).await {
      eprintln!("job {}: the note {} was not offered to its turn: {}", job.id, note.id, why.detail);
    }
  });

  Ok(())
}

/// Every running job's kept words offered again to the turn it works now, so a word kept while that turn could
/// not take it reaches the turn rather than waiting for the next. Each job's offer goes on apart.
pub async fn offer_kept(db: &DatabaseConnection, gate: &Arc<dyn Gate>) {
  let Ok(rows) = all(
    db,
    &format!(
      r#"SELECT {COLUMNS} FROM "jobs" j WHERE j."state" IN ('starting', 'running', 'reviewing')
         AND EXISTS (SELECT 1 FROM "job_words" w WHERE w."job_id" = j."id" AND w."handed_at" IS NULL)"#
    ),
    vec![],
  )
  .await
  else {
    return;
  };

  for job in rows.iter().filter_map(|row| Job::of(row).ok()) {
    let Some(turn) = turn_of(db, job.id).await else { continue };
    let (db, gate) = (db.clone(), gate.clone());

    tokio::spawn(async move {
      if let Ok(offered) = offer(&db, gate.as_ref(), &job, &turn).await {
        for (_, outcome) in offered {
          line(&db, &job, &turn.run, "note", &account(Some(outcome))).await;
        }
      }
    });
  }
}

/// The newest note an item's notes as a job's launch read them name: the mark a later note is handed past, 0
/// for an item read with no note, and none where the item was read without its notes or they name no ids, so
/// a later note is told apart by when it was added instead.
pub fn notes_through(item: &Value) -> Option<i64> {
  let notes = item.get("notes")?.as_array()?;

  if notes.is_empty() {
    return Some(0);
  }

  notes.iter().filter_map(|note| note.get("id").and_then(Value::as_i64)).max()
}

// One line of the job's words in its log, under the run named; a line that could not be written is said.
async fn line(db: &DatabaseConnection, job: &Job, run: &str, kind: &'static str, text: &str) {
  if text.trim().is_empty() {
    return;
  }

  let mut log = RunLog::new(Target {
    agent: job.worker.clone(),
    run: run.to_owned(),
    chat_id: job.room,
    job_id: Some(job.id),
  })
  .keeping_launch();
  let line = Line {
    kind,
    name: Some(WORD_LINE.to_owned()),
    text: text.to_owned(),
    ..Line::default()
  };

  if log.write(db, line).await.is_none() {
    eprintln!("job {}: a line of its words was not written", job.id);
  }
}
