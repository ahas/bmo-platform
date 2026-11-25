//! The agents' sleep (docs/sleep.md): once a day, at its hour on the deployment's clock and within the policy's
//! window, each supervisor with something in its brain to sleep on is run once apart from its sessions, on its own
//! road, while no card stands under a claim; what the run answers is written into its brain as one summary and told
//! to its owner in one record-only line. The judgement of when is a function of the moment it is handed, so a test
//! drives it with any clock; what reaches outside agency stands behind `Night`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use agent_session::{Conversation, Scope, Sessions, Start, Turn, TurnContext};
use chrono::{DateTime, Duration as Span, NaiveDateTime, Timelike};
use policy::sleep::{SLEEP, Sleep};
use sea_orm::{ConnectionTrait, QueryResult};
use serde::Deserialize;
use serde_json::{Value, json};
use store::{Fault, all, one, run};

use crate::herdr_backend::Shared;
use crate::lines::Trees;
use crate::outbox::Receiver;
use crate::peers::brain::Pending;
use crate::places::Ground;
use crate::road;

/// The folder under the agents' tree a sleep's conversation goes out in.
const FOLDER: &str = "sleep";

const HOUR_MS: i64 = 60 * 60 * 1_000;

/// A supervisor's sleep as its setting and its record hold it; a supervisor with no row sleeps at the policy's
/// hour and has never slept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setting {
  pub on: bool,
  /// Its own hour, none for the policy's.
  pub hour: Option<u32>,
  /// The day it last slept for, `YYYY-MM-DD`.
  pub slept_on: Option<String>,
  /// The moment it last tried, in epoch milliseconds.
  pub tried_at: Option<i64>,
}

impl Default for Setting {
  fn default() -> Self {
    Self {
      on: true,
      hour: None,
      slept_on: None,
      tried_at: None,
    }
  }
}

impl Setting {
  fn of(row: &QueryResult) -> Result<Self, Fault> {
    let on: Option<bool> = row.try_get("", "on")?;
    let hour: Option<i32> = row.try_get("", "hour")?;

    Ok(Self {
      on: on.unwrap_or(true),
      hour: hour.and_then(|hour| u32::try_from(hour).ok()),
      slept_on: row.try_get("", "slept_on")?,
      tried_at: row.try_get("", "tried_at")?,
    })
  }
}

/// What the clock makes of a supervisor at a moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Judged {
  /// Its setting says it does not sleep.
  Off,
  /// The moment stands outside its window.
  Outside,
  /// It already slept for the day this window is for.
  Slept,
  /// It tried within this hour already; the next hour of the window tries again.
  TriedThisHour,
  /// It is due to sleep for the day named.
  Due { date: String },
}

/// The moment on the deployment's clock.
fn local(at: i64, offset_hours: i32) -> NaiveDateTime {
  DateTime::from_timestamp_millis(at).unwrap_or_default().naive_utc() + Span::hours(i64::from(offset_hours))
}

/// Whether a supervisor sleeps now: its window opens at its hour and runs the policy's hours on, the day it is for
/// the date the window began on, one try an hour.
pub fn judged(setting: &Setting, rules: &Sleep, now: i64) -> Judged {
  if !setting.on {
    return Judged::Off;
  }

  let hour = setting.hour.unwrap_or(rules.hour) % 24;
  let here = local(now, rules.utc_offset_hours);
  let into = (here.hour() + 24 - hour) % 24;

  if into >= rules.window_hours {
    return Judged::Outside;
  }

  let began = here - Span::hours(i64::from(into));
  let date = began.date().format("%Y-%m-%d").to_string();

  if setting.slept_on.as_deref() == Some(date.as_str()) {
    return Judged::Slept;
  }

  // The deployment's clock is whole hours off UTC, so an hour of it is an hour of epoch milliseconds.
  if setting
    .tried_at
    .is_some_and(|tried| tried.div_euclid(HOUR_MS) == now.div_euclid(HOUR_MS))
  {
    return Judged::TriedThisHour;
  }

  Judged::Due { date }
}

/// The instruction a sleep is asked, for the day named.
pub fn instruction(rules: &Sleep, date: &str) -> String {
  policy::fill_marks(&rules.instruction, &[("date", date), ("budget", &rules.budget_tokens.to_string())])
}

/// What a sleep answers: the summary and the counts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Report {
  pub summary: String,
  #[serde(default)]
  pub learned: i64,
  #[serde(default)]
  pub merged: i64,
  #[serde(default)]
  pub closed: i64,
}

/// The report a sleep's answer holds: the one JSON object it was asked for, read by the JSON reader from its first
/// opening brace to its last closing one, since a model may put the object inside a fence or a sentence; none
/// where that does not read or holds no summary.
pub fn report_of(answer: &str) -> Option<Report> {
  let start = answer.find('{')?;
  let end = answer.rfind('}')?;
  let report: Report = serde_json::from_str(answer.get(start..=end)?).ok()?;

  (!report.summary.trim().is_empty()).then_some(report)
}

/// The receiver's record-only line a sleep is told by.
pub fn line(agent: &str, date: &str, report: &Report) -> Value {
  json!({
    "code": "slept",
    "agent": agent,
    "date": date,
    "learned": report.learned,
    "merged": report.merged,
    "closed": report.closed,
  })
}

/// A boxed answer of `Night`.
pub type Going<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

/// What a sleep reaches outside agency's own store.
pub trait Night: Send + Sync {
  /// Whether any card of any node stands under a claim now.
  fn claimed(&self) -> Going<'_, bool>;
  /// What the agent's brain holds to sleep on; none where it has no brain.
  fn pending<'a>(&'a self, agent: &'a str) -> Going<'a, Option<Pending>>;
  /// The sleep run on the agent's road apart from its sessions, answered with its words.
  fn sleep<'a>(&'a self, agent: &'a str, instruction: &'a str) -> Going<'a, String>;
  /// The summary written into the agent's brain.
  fn summary<'a>(&'a self, agent: &'a str, date: &'a str, summary: &'a str) -> Going<'a, ()>;
  /// The owner told in one record-only line.
  fn tell<'a>(&'a self, agent: &'a str, line: Value) -> Going<'a, ()>;
}

/// What became of a supervisor due to sleep.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
  /// A card stood under a claim, so the sleep was put off to the next hour.
  Claimed,
  /// The supervisor has no brain.
  NoBrain,
  /// Its brain held nothing to sleep on, so the day counts as slept with no run.
  Rested,
  /// It slept, its summary written; `told` says whether the owner's line was kept.
  Slept { report: Report, told: bool },
  /// The sleep did not go out or did not answer as asked; the next hour of the window tries again.
  Failed(String),
}

impl Outcome {
  /// Whether the day counts as slept.
  pub fn slept(&self) -> bool {
    matches!(self, Self::Rested | Self::Slept { .. })
  }
}

/// One supervisor's sleep for the day named, through what it reaches.
pub async fn sleep_one(night: &dyn Night, agent: &str, date: &str) -> Outcome {
  match night.claimed().await {
    Ok(false) => {}
    Ok(true) => return Outcome::Claimed,
    Err(why) => return Outcome::Failed(format!("the cards could not be read: {why}")),
  }

  match night.pending(agent).await {
    Ok(None) => return Outcome::NoBrain,
    Ok(Some(pending)) if !pending.any() => return Outcome::Rested,
    Ok(Some(_)) => {}
    Err(why) => return Outcome::Failed(format!("the brain could not be read: {why}")),
  }

  let answer = match night.sleep(agent, &instruction(&SLEEP, date)).await {
    Ok(answer) => answer,
    Err(why) => return Outcome::Failed(why),
  };
  let Some(report) = report_of(&answer) else {
    return Outcome::Failed("the sleep answered no report".to_owned());
  };

  if let Err(why) = night.summary(agent, date, report.summary.trim()).await {
    return Outcome::Failed(format!("the summary was not written: {why}"));
  }

  let told = match night.tell(agent, line(agent, date, &report)).await {
    Ok(()) => true,
    Err(why) => {
      eprintln!("sleep {agent}: the owner's line was not kept: {why}");

      false
    }
  };

  Outcome::Slept { report, told }
}

/// Every supervisor with its sleep setting, the default where it has none.
pub async fn settings<C: ConnectionTrait>(db: &C) -> Result<Vec<(String, Setting)>, Fault> {
  all(
    db,
    r#"SELECT s."name", z."on", z."hour", z."slept_on", z."tried_at" FROM "supervisors" s
       LEFT JOIN "supervisor_sleep" z ON z."agent" = s."name" ORDER BY s."name""#,
    vec![],
  )
  .await?
  .iter()
  .map(|row| Ok((row.try_get::<String>("", "name")?, Setting::of(row)?)))
  .collect()
}

/// One supervisor's sleep setting, the default where it has none.
pub async fn setting<C: ConnectionTrait>(db: &C, agent: &str) -> Result<Setting, Fault> {
  match one(
    db,
    r#"SELECT "on", "hour", "slept_on", "tried_at" FROM "supervisor_sleep" WHERE "agent" = $1"#,
    vec![agent.into()],
  )
  .await?
  {
    Some(row) => Setting::of(&row),
    None => Ok(Setting::default()),
  }
}

/// A supervisor's setting written: whether it sleeps and its own hour, none for the policy's.
pub async fn set<C: ConnectionTrait>(
  db: &C,
  agent: &str,
  on: bool,
  hour: Option<u32>,
) -> Result<(), Fault> {
  run(
    db,
    r#"INSERT INTO "supervisor_sleep" ("agent", "on", "hour") VALUES ($1, $2, $3)
       ON CONFLICT ("agent") DO UPDATE SET "on" = $2, "hour" = $3"#,
    vec![agent.into(), on.into(), hour.map(|hour| hour as i32).into()],
  )
  .await
  .map(|_| ())
}

async fn tried<C: ConnectionTrait>(db: &C, agent: &str, at: i64) -> Result<(), Fault> {
  run(
    db,
    r#"INSERT INTO "supervisor_sleep" ("agent", "tried_at") VALUES ($1, $2)
       ON CONFLICT ("agent") DO UPDATE SET "tried_at" = $2"#,
    vec![agent.into(), at.into()],
  )
  .await
  .map(|_| ())
}

async fn slept<C: ConnectionTrait>(db: &C, agent: &str, date: &str) -> Result<(), Fault> {
  run(
    db,
    r#"INSERT INTO "supervisor_sleep" ("agent", "slept_on") VALUES ($1, $2)
       ON CONFLICT ("agent") DO UPDATE SET "slept_on" = $2"#,
    vec![agent.into(), date.into()],
  )
  .await
  .map(|_| ())
}

/// One pass of the clock at the moment named: every supervisor due sleeps, one after another, each try recorded and
/// each day slept recorded; what became of each one due, by its name.
pub async fn pass<C: ConnectionTrait>(
  db: &C,
  night: &dyn Night,
  rules: &Sleep,
  now: i64,
) -> Result<Vec<(String, Outcome)>, Fault> {
  let mut came = vec![];

  for (agent, setting) in settings(db).await? {
    let Judged::Due { date } = judged(&setting, rules, now) else {
      continue;
    };

    tried(db, &agent, now).await?;

    let outcome = sleep_one(night, &agent, &date).await;

    if outcome.slept() {
      slept(db, &agent, &date).await?;
    }

    match &outcome {
      Outcome::Slept { report, told } => println!(
        "sleep {agent}: slept for {date}: {} learned, {} merged, {} closed{}",
        report.learned,
        report.merged,
        report.closed,
        if *told { "" } else { "; the owner was not told" }
      ),
      Outcome::Rested => println!("sleep {agent}: nothing to sleep on for {date}"),
      Outcome::Claimed => println!("sleep {agent}: a card stands under a claim; tried again next hour"),
      Outcome::NoBrain => {}
      Outcome::Failed(why) => eprintln!("sleep {agent}: did not sleep for {date}: {why}"),
    }

    came.push((agent, outcome));
  }

  Ok(came)
}

/// What a sleep reaches in a running agency: the arbiter's master, the brain, the supervisor's road and its
/// sessions, and the receiver.
pub struct Nightly {
  pub ground: Arc<Ground>,
  pub sessions: Shared,
}

impl Nightly {
  async fn claimed_now(&self) -> Result<bool, String> {
    let Some(master) = crate::cards::master_of(&self.ground) else {
      return Ok(false);
    };
    let picture = master.picture().await.map_err(|why| format!("{why:?}"))?;

    Ok(claimed_in(&picture))
  }

  async fn slept_now(&self, agent: &str, instruction: &str) -> Result<String, String> {
    let supervisor = crate::supervisors::named(&self.ground.db, agent)
      .await
      .map_err(|why| why.detail)?
      .ok_or_else(|| format!("no supervisor named {agent} is held"))?;
    let order = supervisor.order();
    let ask = road::Ask::of(road::Row::Supervisor(agent), "chat", &order);
    let resolution = road::resolve(&self.ground, &ask).await?;
    let Some(taken) = resolution.road() else {
      return Err(resolution.sentence());
    };
    let folder = Trees::of(&self.ground).agents.join(FOLDER);

    std::fs::create_dir_all(&folder).map_err(|why| format!("{}: {why}", folder.display()))?;

    let session = self
      .sessions
      .start(Start {
        agent: agent.to_owned(),
        scope: Scope::Conversation(Conversation {
          provider: taken.provider.clone(),
          model: taken.model.clone(),
          cwd: folder.to_string_lossy().into_owned(),
        }),
      })
      .await
      .map_err(|why| format!("the sleep's session did not start: {why:?}"))?;
    let ended = self
      .sessions
      .send_turn(Turn {
        session: session.id.clone(),
        prompt: instruction.to_owned(),
        context: TurnContext::default(),
      })
      .await;

    if let Err(why) = self.sessions.end(&session.id).await {
      eprintln!("sleep {agent}: its session did not end: {why:?}");
    }

    let ended = ended.map_err(|why| format!("the sleep's turn did not go out: {why:?}"))?;

    ended
      .answer
      .filter(|answer| !answer.trim().is_empty())
      .ok_or_else(|| ended.reason.unwrap_or_else(|| "the sleep's turn answered nothing".to_owned()))
  }
}

/// Whether any card of any node of the master's picture stands under a claim.
pub fn claimed_in(picture: &Value) -> bool {
  picture["nodes"].as_array().into_iter().flatten().any(|node| {
    node["cards"]
      .as_array()
      .into_iter()
      .flatten()
      .any(|card| card.get("claim").is_some_and(|claim| !claim.is_null()))
  })
}

impl Night for Nightly {
  fn claimed(&self) -> Going<'_, bool> {
    Box::pin(self.claimed_now())
  }

  fn pending<'a>(&'a self, agent: &'a str) -> Going<'a, Option<Pending>> {
    Box::pin(async move { self.ground.peers.brain.pending(agent).await.map_err(|why| why.to_string()) })
  }

  fn sleep<'a>(&'a self, agent: &'a str, instruction: &'a str) -> Going<'a, String> {
    Box::pin(self.slept_now(agent, instruction))
  }

  fn summary<'a>(&'a self, agent: &'a str, date: &'a str, summary: &'a str) -> Going<'a, ()> {
    Box::pin(async move {
      self.ground.peers.brain.slept(agent, date, summary).await.map_err(|why| why.to_string())
    })
  }

  fn tell<'a>(&'a self, agent: &'a str, line: Value) -> Going<'a, ()> {
    Box::pin(async move {
      match Receiver::of(self.ground.vars()) {
        Some(receiver) => receiver.keep_line(agent, &line).await,
        None => Err("the deployment names no receiver".to_owned()),
      }
    })
  }
}
