//! Words said into a run's working turn (docs/agent-control.md, 「Words that arrive while a turn is
//! working」): a run's session reached by its id, offered to the turn it is working and answered with what
//! became of them; and a repository's worker reached by the repository, offered to the job's turn going in
//! its checkout, or where nothing works there carried in a turn of its own on the worker's session under
//! the worker's own lock and clock (docs/agent-control.md, 「Words said to a worker」).
//!
//! The session push is the door: what the run took is repeated to any later turn of the same run, and an
//! offer the turn did not take is answered refused as the turn ends. Agency keeps no words for a run past
//! the answer, so a refused word belongs to whoever said it, which may say it again as a turn of its own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use agent_session::{Aside, Fault, FaultKind, Scope, SessionId, Sessions};
use axum::http::StatusCode;
use base64::Engine as _;
use bmo_agency_client::{WordsAsk, WordsSaid, WordsSent};
use policy::fill_marks;
use policy::worker_words::WORKER_WORDS;
use store::{all, one};

use crate::control::Refused;
use crate::door::aside;
use crate::door::turn::{AsideOutcome, Overrun, Overtime, Watch};
use crate::herdr_backend::Shared;
use crate::job_words;
use crate::places::{AgentRow, Ground};
use crate::run_log::RunLog;
use crate::run_log::Target;
use crate::run_log::lines::Line;
use crate::worker_session::{self, Missed, PROVIDER, Side, Turned};
use crate::{Refusal, now_ms};

/// Who the owner of the deployment says a word in the control port's name.
pub const OWNER: &str = "owner";

/// The states a slot request carries while its run goes.
const OPEN: [&str; 3] = ["waiting", "reserved", "active"];

/// The line a word's words and what became of them stand as in a run's log.
const WORD_LINE: &str = "word";

/// The turns of a worker's own session running now, by the worker, so words said meanwhile go into one.
static HANDING: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Mutex::default);

fn handing() -> MutexGuard<'static, HashMap<String, String>> {
  HANDING.lock().unwrap_or_else(PoisonError::into_inner)
}

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

fn faulted(fault: Fault) -> Refused {
  let status = match fault.kind {
    FaultKind::Unknown | FaultKind::UnknownTurn => StatusCode::NOT_FOUND,
    FaultKind::Busy => StatusCode::CONFLICT,
    FaultKind::Unreachable => StatusCode::SERVICE_UNAVAILABLE,
    FaultKind::Refused => StatusCode::UNPROCESSABLE_ENTITY,
  };

  Refused::new(status, fault.message).with_code(fault.kind.code())
}

/// The words a body says, read and checked: one word or one file at least, within the policy's bounds.
pub fn asked(body: serde_json::Value) -> Result<WordsAsk, Refused> {
  let ask: WordsAsk = serde_json::from_value(body)
    .map_err(|why| bad(format!("the words do not read: {why}")).with_field("words"))?;
  let words = ask.words.trim();

  if words.is_empty() && ask.files.is_empty() {
    return Err(bad("words or files (one at least) are required").with_field("words"));
  }

  if words.encode_utf16().count() > WORKER_WORDS.most_words {
    return Err(bad(format!(
      "the words are longer than a word may be: {} UTF-16 units at most",
      WORKER_WORDS.most_words
    )));
  }

  if ask.files.len() > WORKER_WORDS.most_files {
    return Err(bad(format!("a word carries {} files at most", WORKER_WORDS.most_files)));
  }

  for file in &ask.files {
    if file.name.trim().is_empty() {
      return Err(bad("a file said with the words names no file"));
    }

    if file.data.len().div_ceil(4).saturating_mul(3) > WORKER_WORDS.most_file_bytes {
      return Err(
        Refused::new(
          StatusCode::PAYLOAD_TOO_LARGE,
          format!("a file said with the words holds {} bytes at most", WORKER_WORDS.most_file_bytes),
        )
        .with_code("too-large"),
      );
    }
  }

  Ok(ask)
}

/// Who says the words: the body's own name, the owner where it names none.
pub fn sender(ask: &WordsAsk) -> String {
  ask.by.as_deref().map(str::trim).filter(|by| !by.is_empty()).unwrap_or(OWNER).to_owned()
}

/// Words said to a run's session: offered to the turn it carries now, and answered with what became of
/// them. A run whose session has ended is refused as ended, and one whose session stands at rest with its
/// next turn its caller's.
pub async fn to_session(
  ground: Arc<Ground>,
  sessions: Shared,
  id: &str,
  ask: WordsAsk,
) -> Result<WordsSent, Refused> {
  if ask.worker.is_some() {
    return Err(bad("a run's session names no worker").with_field("worker"));
  }

  let session = SessionId(id.to_owned());
  let held = sessions.held_scope(&session).await;

  // A session a running job's turn works in takes the job's road.
  if held.is_ok()
    && let Some(job) = job_words::working_in(&ground.db, id).await?
  {
    return to_job(&ground, job, ask).await;
  }

  let (agent, scope) = match held {
    Ok(held) => held,
    Err(fault) if fault.kind == FaultKind::Unknown => {
      // A session this process does not hold may be one a job's turn ran in: a turn picked up across a restart,
      // or one of an earlier stage, both of which address the job.
      if let Some(job) = job_words::of_session(&ground.db, id).await? {
        return to_job(&ground, job, ask).await;
      }

      return Err(if ended(&ground.db, id).await? {
        Refused::new(StatusCode::GONE, format!("the run {id} has ended, so words said to it reach no turn"))
          .with_code("run-ended")
      } else {
        Refused::new(StatusCode::NOT_FOUND, format!("no run is named {id}")).with_code("unknown-session")
      });
    }
    Err(fault) => return Err(faulted(fault)),
  };

  let Scope::Apart(apart) = &scope else {
    return Err(
      Refused::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("{id} is no run: words to a room or a supervisor go by the room's own road"),
      )
      .with_code("not-a-run"),
    );
  };

  let by = sender(&ask);

  if !may_say(&ground, id, &by).await? {
    return Err(
      Refused::new(
        StatusCode::FORBIDDEN,
        format!(
          "{by} may not say words to the run {id}: only the owner and the supervisor that asked for it"
        ),
      )
      .with_code("not-the-sender"),
    );
  }

  let job = job_of_session(&ground, id).await?;
  let Some((turn, run)) = sessions.turn_going(&session) else {
    return Err(
      Refused::new(
        StatusCode::CONFLICT,
        format!("no turn is working in {id}; the run's next turn is its caller's"),
      )
      .with_code("no-turn"),
    );
  };
  let prompt = with_files(&ground, &agent, &ask)?;

  // The words stand in the run's record as they are said, and what became of them once the turn answers.
  let at = Where::at(&agent, &run, job.as_ref());

  write_line(&ground, &at, "prompt", Some(WORD_LINE), &prompt).await?;

  let said = sessions.push(&session, turn, &prompt).await.map_err(faulted)?;
  let (word, note) = account_of(said, &apart.provider);

  write_line(&ground, &at, "note", Some(WORD_LINE), &note).await?;

  Ok(WordsSent {
    said: word,
    session_id: Some(id.to_owned()),
    run: Some(run),
    turn_id: Some(turn),
    job_id: job.map(|job| job.id),
    reason: (said != Aside::Delivered).then_some(note),
  })
}

/// Words said to a repository's worker: the job's where one runs in one of its checkouts, into its turn or kept
/// for its next, or, where none runs there, carried in a turn of its own on the worker's session. The owner's
/// alone.
pub async fn to_repository(
  ground: Arc<Ground>,
  repository: &str,
  ask: WordsAsk,
) -> Result<WordsSent, Refused> {
  if sender(&ask) != OWNER {
    return Err(
      Refused::new(StatusCode::FORBIDDEN, "words to a repository's worker are the owner's alone")
        .with_code("not-the-sender"),
    );
  }

  let held = crate::repositories::named(&ground.db, repository)
    .await
    .map_err(|why| Refused::failed(why.detail))?
    .ok_or_else(|| {
      Refused::new(StatusCode::NOT_FOUND, format!("no repository is named {repository}"))
        .with_code("unknown-repository")
    })?;
  let checkouts = checkouts_of(&ground, repository).await?;
  let named = ask.worker.as_deref().map(str::trim).filter(|worker| !worker.is_empty());

  if let Some(worker) = named
    && !checkouts.iter().any(|checkout| checkout.name == worker)
  {
    return Err(
      Refused::new(StatusCode::UNPROCESSABLE_ENTITY, format!("{worker} is no checkout of {repository}"))
        .with_code("unknown-worker"),
    );
  }

  let jobs = open_jobs(&ground, &checkouts).await?;
  let chosen = match named {
    Some(worker) => jobs.iter().find(|job| job.worker == worker).cloned(),
    None => {
      let mut working = None;

      for job in &jobs {
        if job_words::turn_of(&ground.db, job.id).await.is_some() {
          working = Some(job.clone());
          break;
        }
      }

      working.or_else(|| jobs.first().cloned())
    }
  };

  // A job that runs in the checkout takes the words by its own road, into its turn or kept for its next.
  if let Some(chosen) = &chosen
    && let Some(job) = job_words::named(&ground.db, chosen.id).await?.filter(job_words::Job::runs)
  {
    return to_job(&ground, job, ask).await;
  }

  let worker = match (&chosen, named) {
    (Some(job), _) => job.worker.clone(),
    (None, Some(worker)) => worker.to_owned(),
    (None, None) => held.name.clone(),
  };
  let row = AgentRow::named(&ground.db, &worker)
    .await
    .map_err(|why| Refused::failed(why.detail))?
    .ok_or_else(|| {
      Refused::new(StatusCode::NOT_FOUND, format!("no worker is named {worker}"))
        .with_code("unknown-worker")
    })?;

  if row.node.is_some() {
    return Err(
      Refused::new(
        StatusCode::CONFLICT,
        format!("{worker} stands on another machine, so no turn of its own can be stood here"),
      )
      .with_code("worker-on-node"),
    );
  }

  let prompt = with_files(&ground, &worker, &ask)?;

  // A turn of the worker's own already running takes the words at its next look.
  let running = { handing().get(&worker).cloned() };

  if let Some(run) = running {
    let at = Where::of_worker(&worker, &run);

    write_line(&ground, &at, "prompt", Some(WORD_LINE), &prompt).await?;

    let Some(mut offer) = aside::offer(&run, &prompt) else {
      return Ok(WordsSent {
        said: WordsSaid::Refused,
        run: Some(run),
        reason: Some("the word's turn is not listening for words at this moment".to_owned()),
        ..WordsSent::default()
      });
    };
    let said = match (&mut offer.answer).await {
      Ok(outcome) => outcome,
      Err(_) => AsideOutcome::Refused,
    };

    return refused_or_taken(&ground, &at, run, said).await;
  }

  // A session whose role's order names no model on Claude Code cannot stand a turn of the worker's own.
  word_turn_can_stand(&ground, &worker).await?;

  // The worker's own session is one turn at a time: a job's turn working there holds its lock.
  let Some(lock) = crate::places::locks::try_hold(&crate::places::locks::agent_lock_path(
    &ground.roots.data,
    &worker_session::held::scope_of(&worker),
  )) else {
    return Ok(WordsSent {
      said: WordsSaid::Refused,
      reason: Some(format!("another turn works at {worker} now, so the words did not go in")),
      ..WordsSent::default()
    });
  };

  // A worker pinned to another provider than Claude Code's holds no session a word's turn could go on.
  if let Some((provider, _)) = crate::queue::routes::worker_pin(&ground.db, &worker)
    .await
    .map_err(|why| Refused::failed(why.0))?
    .filter(|(provider, _)| provider != PROVIDER)
  {
    let note = fill_marks(&WORKER_WORDS.held.pinned, &[("provider", &provider)]);
    let run = word_run(&worker);

    write_word(&ground, &Where::of_worker(&worker, &run), &prompt, &note).await?;

    return Ok(WordsSent {
      said: WordsSaid::Refused,
      run: Some(run),
      reason: Some(note),
      ..WordsSent::default()
    });
  }

  let run = word_run(&worker);

  write_word(&ground, &Where::of_worker(&worker, &run), &prompt, "").await?;
  handing().insert(worker.clone(), run.clone());

  let title = fill_marks(&WORKER_WORDS.title, &[("worker", &worker)]);
  let bound = Duration::from_secs(WORKER_WORDS.answer_minutes.saturating_mul(60));
  let started = run.clone();

  tokio::spawn(async move {
    let _lock = lock;

    if let Err(why) = own_turn(&ground, &row, &started, &prompt, &title, bound).await {
      eprintln!("word {worker}: {}", why.detail);
    }

    handing().remove(&worker);
  });

  Ok(WordsSent {
    said: WordsSaid::Started,
    run: Some(run),
    ..WordsSent::default()
  })
}

/// Words said to a job by its id (docs/api.md, 「assignments and jobs」, 「Words and notes for a running job」).
pub async fn to_job_named(ground: Arc<Ground>, id: i64, ask: WordsAsk) -> Result<WordsSent, Refused> {
  let job = job_words::named(&ground.db, id).await?.ok_or_else(|| {
    Refused::new(StatusCode::NOT_FOUND, format!("no job is numbered {id}")).with_code("unknown-job")
  })?;

  to_job(&ground, job, ask).await
}

/// Words said to a job, by its id or by a session a turn of it ran in: kept on the job, offered to the turn it
/// works now, and carried by its next turn where none took them. A job that has ended refuses them as ended.
async fn to_job(ground: &Ground, job: job_words::Job, ask: WordsAsk) -> Result<WordsSent, Refused> {
  if ask.worker.is_some() {
    return Err(bad("a job's words name no worker").with_field("worker"));
  }

  if !job.runs() {
    return Err(
      Refused::new(
        StatusCode::GONE,
        format!("the job {} has ended, so words said to it reach no turn", job.id),
      )
      .with_code("run-ended"),
    );
  }

  let by = sender(&ask);

  if !may_say_to_job(ground, &job, &by).await? {
    return Err(
      Refused::new(
        StatusCode::FORBIDDEN,
        format!(
          "{by} may not say words to the job {}: only the owner and the supervisor that asked for it",
          job.id
        ),
      )
      .with_code("not-the-sender"),
    );
  }

  let prompt = with_files(ground, &job.worker, &ask)?;
  let said = job_words::say(&ground.db, ground.peers.gate.as_ref(), &job, &prompt, &by, None)
    .await?
    .ok_or_else(|| Refused::failed(format!("the words for job {} were not kept", job.id)))?;
  let word = match said.outcome {
    Some(AsideOutcome::Delivered) => WordsSaid::Delivered,
    Some(AsideOutcome::Waiting) => WordsSaid::Waiting,
    Some(AsideOutcome::Refused) | None => WordsSaid::Kept,
  };
  let taken = said.outcome == Some(AsideOutcome::Delivered);

  Ok(WordsSent {
    said: word,
    session_id: said.turn.as_ref().map(|turn| turn.session.clone()).or(job.session),
    run: Some(said.run),
    turn_id: said.turn.as_ref().filter(|_| taken).map(|turn| turn.turn),
    job_id: Some(job.id),
    reason: (!taken).then_some(said.note),
  })
}

/// Whether the sender may say to a job: the owner, the holder that launched it, or the supervisor that asked
/// for its seat.
async fn may_say_to_job(ground: &Ground, job: &job_words::Job, by: &str) -> Result<bool, Refused> {
  if by == OWNER || by == job.holder {
    return Ok(true);
  }

  let asked = one(
    &ground.db,
    r#"SELECT "asked_by" FROM "role_slot_requests" WHERE "owner" = $1 ORDER BY "sequence" DESC LIMIT 1"#,
    vec![format!("{}{}", crate::order_run::dispatch::JOB_OWNER, job.id).into()],
  )
  .await?
  .and_then(|row| row.try_get::<Option<String>>("", "asked_by").ok().flatten());

  Ok(asked.is_some_and(|asked| asked == by))
}

/// One turn of words on the worker's own session, on the line a job runs on and in a session its role keeps:
/// its answer kept under the word's run, and what ended it where it gave none.
async fn own_turn(
  ground: &Ground,
  worker: &AgentRow,
  run: &str,
  prompt: &str,
  title: &str,
  bound: Duration,
) -> Result<(), Refusal> {
  let (model, resumed) = worker_model(ground, &worker.name).await?;
  let mut watch = WordWatch;
  let turned = worker_session::turn(
    &Side { ground },
    worker_session::Asked {
      worker,
      model,
      session: resumed,
      prompt: prompt.to_owned(),
      name: run.to_owned(),
      title: title.to_owned(),
      label: "word".to_owned(),
      clock: Some(bound),
    },
    &mut watch,
  )
  .await?;
  let at = Where::of_worker(&worker.name, run);

  match turned {
    Turned {
      reached: false,
      missed,
      ..
    } => write_word(ground, &at, "", &held_line(missed.unwrap_or(Missed::Unsubmitted))).await,
    Turned {
      reply: Some(reply), ..
    } => write_line(ground, &at, "text", None, reply.text.trim_end()).await,
    Turned { reply: None, .. } => write_word(ground, &at, "", &WORKER_WORDS.ended.silent).await,
  }
}

/// The model and the session a word's turn on a worker goes on: the newest session its role keeps there on a
/// login that answers, else the model the worker role's order names on Claude Code.
async fn worker_model(ground: &Ground, worker: &str) -> Result<(String, Option<String>), Refusal> {
  let now = now_ms();

  for session in crate::worker_session::records::sessions(&ground.db, worker).await?.iter().rev() {
    let open = crate::control::claude_accounts::account_open(
      crate::control::claude_accounts::Judging::of(ground),
      &session.account,
      now,
    )
    .await
    .map_err(|refused| Refusal::failed(refused.error))?;

    if !open {
      continue;
    }

    let ttl = crate::lines::standing::session_ttl_ms(ground, &session.model).await;
    let resumed =
      crate::worker_session::records::resumable(&ground.db, worker, &session.model, &session.account, ttl)
        .await?;

    return Ok((session.model.clone(), resumed));
  }

  let model = crate::order::role_entry(&ground.db, crate::agent_roles::job_role(), PROVIDER)
    .await
    .ok()
    .map(|entry| entry.model)
    .filter(|model| !model.trim().is_empty())
    .ok_or_else(|| {
      Refusal::failed(format!(
        "the {} role's order names no model on {PROVIDER}, so no turn of {worker}'s own can stand",
        crate::agent_roles::job_role()
      ))
    })?;

  Ok((model, None))
}

// The worker role's model named, or a refusal where it names none, so a route answers before a turn stands
// and says why.
async fn word_turn_can_stand(ground: &Ground, worker: &str) -> Result<(), Refused> {
  worker_model(ground, worker)
    .await
    .map(|_| ())
    .map_err(|why| Refused::new(StatusCode::SERVICE_UNAVAILABLE, why.detail).with_code("no-word-road"))
}

/// One job's run as the words said into it are kept: its id, the checkout it runs on, and the room its lines
/// stand under.
#[derive(Clone, Debug)]
struct JobAt {
  id: i64,
  worker: String,
  room: Option<i64>,
}

impl JobAt {
  fn of(row: &sea_orm::QueryResult) -> Result<Self, sea_orm::DbErr> {
    Ok(Self {
      id: row.try_get("", "id")?,
      worker: row.try_get("", "worker_name")?,
      room: row.try_get("", "room_id")?,
    })
  }
}

// Every job going in one of these checkouts, newest first.
async fn open_jobs(ground: &Ground, checkouts: &[Checkout]) -> Result<Vec<JobAt>, Refused> {
  let mut jobs = vec![];

  for checkout in checkouts {
    let rows = all(
      &ground.db,
      r#"SELECT "id", "worker_name", "room_id" FROM "jobs"
         WHERE "worker_name" = $1 AND "state" IN ('starting', 'running', 'reviewing') ORDER BY "id" DESC"#,
      vec![checkout.name.clone().into()],
    )
    .await?;

    for row in rows {
      jobs.push(JobAt::of(&row)?);
    }
  }

  jobs.sort_by_key(|job| std::cmp::Reverse(job.id));

  Ok(jobs)
}

/// One checkout of a repository, as its words may name it.
#[derive(Clone, Debug)]
struct Checkout {
  name: String,
}

// Every checkout of a repository: its base, its support worktrees and its node checkouts.
async fn checkouts_of(ground: &Ground, repository: &str) -> Result<Vec<Checkout>, Refused> {
  let workers = crate::queue::workers::workers(&ground.db, &ground.source).await?;

  Ok(
    workers
      .into_iter()
      .filter(|worker| !worker.builtin)
      .filter(|worker| worker.support_of.as_deref().unwrap_or(&worker.name) == repository)
      .map(|worker| Checkout { name: worker.name })
      .collect(),
  )
}

// The job whose turn a session carries, where one does.
async fn job_of_session(ground: &Ground, session: &str) -> Result<Option<JobAt>, Refused> {
  let row = one(
    &ground.db,
    r#"SELECT "id", "worker_name", "session_id", "room_id" FROM "jobs"
       WHERE "session_id" = $1 ORDER BY "id" DESC LIMIT 1"#,
    vec![session.into()],
  )
  .await?;

  Ok(row.as_ref().map(JobAt::of).transpose()?)
}

/// Whether a session id names a run that is no job's and has ended: the slot it was let in on is given back. A
/// job's sessions are the job's, and answer as it does.
async fn ended(db: &impl sea_orm::ConnectionTrait, session: &str) -> Result<bool, Refused> {
  let seat = one(
    db,
    r#"SELECT "state" FROM "role_slot_requests" WHERE "session" = $1 ORDER BY "sequence" DESC LIMIT 1"#,
    vec![session.into()],
  )
  .await?;

  if let Some(row) = seat
    && let Ok(state) = row.try_get::<String>("", "state")
    && !OPEN.contains(&state.as_str())
  {
    return Ok(true);
  }

  Ok(false)
}

/// Whether the sender may say to this run: the owner, or the supervisor that asked for it.
async fn may_say(ground: &Ground, session: &str, by: &str) -> Result<bool, Refused> {
  if by == OWNER {
    return Ok(true);
  }

  let asked = one(
    &ground.db,
    r#"SELECT "asked_by" FROM "role_slot_requests" WHERE "session" = $1 ORDER BY "sequence" DESC LIMIT 1"#,
    vec![session.into()],
  )
  .await?
  .and_then(|row| row.try_get::<Option<String>>("", "asked_by").ok().flatten());

  Ok(asked.is_some_and(|asked| asked == by))
}

/// The words the run is told: every file said with them written where its program can read it, and named
/// before the words as said.
fn with_files(ground: &Ground, worker: &str, ask: &WordsAsk) -> Result<String, Refused> {
  let words = ask.words.trim().to_owned();

  if ask.files.is_empty() {
    return Ok(words);
  }

  let folder = ground.roots.data.join("word-files").join(slug(worker)).join(now_ms().to_string());

  std::fs::create_dir_all(&folder)
    .map_err(|why| Refused::failed(format!("{} could not be made: {why}", folder.display())))?;

  let engine = base64::engine::general_purpose::STANDARD;
  let mut named = Vec::new();

  for (at, file) in ask.files.iter().enumerate() {
    let bytes = engine.decode(file.data.trim()).map_err(|why| {
      bad(format!("the file {} does not read as base64: {why}", file.name)).with_field("files")
    })?;
    let name = match Path::new(&file.name).file_name().and_then(|name| name.to_str()) {
      Some(name) if !name.trim().is_empty() => name.to_owned(),
      _ => format!("attachment-{at}"),
    };
    let path: PathBuf = folder.join(format!("{at}-{name}"));

    std::fs::write(&path, &bytes)
      .map_err(|why| Refused::failed(format!("{} could not be written: {why}", path.display())))?;
    named.push(format!("- {} ({} bytes)", path.display(), bytes.len()));
  }

  Ok(format!(
    "The owner said these words and attached these files; they stand at these paths, which are theirs to \
     read:\n{}\n\n{words}",
    named.join("\n")
  ))
}

fn slug(text: &str) -> String {
  let kept: String = text
    .chars()
    .map(|letter| {
      if letter.is_ascii_alphanumeric() || matches!(letter, '.' | '_' | '-') {
        letter
      } else {
        '-'
      }
    })
    .collect();

  kept.trim_matches('-').to_owned()
}

/// Where one word's lines stand in a run's log.
#[derive(Clone, Debug)]
struct Where {
  agent: String,
  run: String,
  chat: Option<i64>,
  job: Option<i64>,
}

impl Where {
  fn at(agent: &str, run: &str, job: Option<&JobAt>) -> Self {
    Self {
      agent: agent.to_owned(),
      run: run.to_owned(),
      chat: job.and_then(|job| job.room),
      job: job.map(|job| job.id),
    }
  }

  fn of_worker(worker: &str, run: &str) -> Self {
    Self {
      agent: worker.to_owned(),
      run: run.to_owned(),
      chat: None,
      job: None,
    }
  }
}

async fn write_line(
  ground: &Ground,
  at: &Where,
  kind: &'static str,
  name: Option<&str>,
  text: &str,
) -> Result<(), Refusal> {
  if text.trim().is_empty() {
    return Ok(());
  }

  let mut log = RunLog::new(Target {
    agent: at.agent.clone(),
    run: at.run.clone(),
    chat_id: at.chat,
    job_id: at.job,
  })
  .keeping_launch();
  let line = Line {
    kind,
    name: name.map(str::to_owned),
    text: text.to_owned(),
    ..Line::default()
  };

  match log.write(&ground.db, line).await {
    Some(_) => Ok(()),
    None => Err(Refusal::failed(format!("a line of {}'s words was not written", at.agent))),
  }
}

// The words as they were said, and what became of them where that is a line of its own.
async fn write_word(ground: &Ground, at: &Where, words: &str, note: &str) -> Result<(), Refusal> {
  write_line(ground, at, "prompt", Some(WORD_LINE), words).await?;

  if !note.trim().is_empty() {
    write_line(ground, at, "note", Some(WORD_LINE), note).await?;
  }

  Ok(())
}

fn account_of(said: Aside, provider: &str) -> (WordsSaid, String) {
  match said {
    Aside::Delivered => (WordsSaid::Delivered, "The turn took the words.".to_owned()),
    Aside::Waiting => (
      WordsSaid::Waiting,
      "The run waits on a person's answer, so the words stayed out and go in once it is answered."
        .to_owned(),
    ),
    Aside::Refused if !crate::door::turn::takes_words_mid_turn(provider) => (
      WordsSaid::Refused,
      format!(
        "The run's provider {provider} takes no words while a turn works, so the words did not go in."
      ),
    ),
    Aside::Refused => (
      WordsSaid::Refused,
      "The words did not go in: the turn took none, so they belong to the run's next turn.".to_owned(),
    ),
  }
}

fn held_line(missed: Missed) -> String {
  let held = &WORKER_WORDS.held;

  match missed {
    Missed::Person => held.person.clone(),
    Missed::Untrusted => held.untrusted.clone(),
    Missed::Stopped => held.stopped.clone(),
    Missed::Disabled => held.disabled.clone(),
    Missed::Lost => held.lost.clone(),
    Missed::Resting => held.resting.clone(),
    Missed::Unsubmitted | Missed::Idle => held.unsubmitted.clone(),
  }
}

fn word_run(worker: &str) -> String {
  format!("{}-word-{}-{}", policy::deployment::DEPLOYMENT.agent_key, slug(worker), now_ms())
}

/// What a job's report says of the words said into its turn while it ran, read off its run's own log, none
/// where no word was said. A word and what became of it stand together.
pub async fn report(db: &sea_orm::DatabaseConnection, job: i64) -> Result<Option<String>, Refusal> {
  let rows = all(
    db,
    r#"SELECT "kind", "text" FROM "agent_log"
       WHERE "job_id" = $1 AND "name" = $2 AND "kind" IN ('prompt', 'note') ORDER BY "id""#,
    vec![job.into(), WORD_LINE.into()],
  )
  .await?;

  if rows.is_empty() {
    return Ok(None);
  }

  let mut lines = vec!["Words said into this job's turn while it ran:".to_owned()];
  let mut said: Option<String> = None;

  for row in rows {
    let kind: String = row.try_get("", "kind")?;
    let text: String = row.try_get("", "text")?;

    if kind == "prompt" {
      if let Some(before) = said.replace(text) {
        lines.push(format!("- {before}"));
      }
    } else if let Some(before) = said.take() {
      lines.push(format!("- {before}\n  {text}"));
    } else {
      lines.push(format!("- {text}"));
    }
  }

  if let Some(said) = said {
    lines.push(format!("- {said}"));
  }

  Ok(Some(lines.join("\n")))
}

/// A job's report with the words said into its turn, where any were.
pub async fn with_words(
  db: &sea_orm::DatabaseConnection,
  job: i64,
  report: Option<String>,
) -> Result<Option<String>, Refusal> {
  let Some(words) = self::report(db, job).await? else {
    return Ok(report);
  };

  Ok(Some(match report {
    Some(report) if !report.trim().is_empty() => format!("{report}\n\n{words}"),
    _ => words,
  }))
}

/// The words an offer to a word's own turn was answered with, what became of them written in that turn's run.
async fn refused_or_taken(
  ground: &Ground,
  at: &Where,
  run: String,
  said: AsideOutcome,
) -> Result<WordsSent, Refused> {
  let (word, note) = match said {
    AsideOutcome::Delivered => (WordsSaid::Delivered, "The turn took the words.".to_owned()),
    AsideOutcome::Waiting => (
      WordsSaid::Waiting,
      "The run waits on a person's answer, so the words stayed out and go in once it is answered."
        .to_owned(),
    ),
    AsideOutcome::Refused => (
      WordsSaid::Refused,
      "The words did not go in: the turn took none, so they belong to the worker's next turn.".to_owned(),
    ),
  };

  write_line(ground, at, "note", Some(WORD_LINE), &note).await.map_err(Refused::from)?;

  Ok(WordsSent {
    said: word,
    run: Some(run),
    reason: (said != AsideOutcome::Delivered).then_some(note),
    ..WordsSent::default()
  })
}

/// What a word's own turn is watched with: its clock cuts it, so a program that never answers holds the
/// worker no longer than the policy names (docs/agent-control.md, 「A clock asks, it never cuts」).
struct WordWatch;

impl Watch for WordWatch {
  async fn overtime(&mut self, _overdue: &Overrun) -> Overtime {
    Overtime::Cut
  }
}
