//! A turn re-attached to by its caller's key (docs/api.md, 「sessions」; docs/sessions.md, 「A restart hands
//! the session over」). A send naming a key holds its turn under it: the turns standing under a key — those
//! this server sent and those it picked up at its start — are held here, each with the one answer every
//! caller waiting on it is given once it settles, and a keyed turn's end is kept in the settings for the
//! span the policy names, so a repeat after the end, after a restart of either side, is answered with it
//! rather than sending the turn again.

use std::collections::HashMap;
use std::sync::Mutex;

use agent_session::{Delivery, Fault, Outcome, TurnAnswer, TurnEnd};
use policy::turn_keys::TURN_KEYS;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::now_ms;
use crate::queue::settings;

/// The settings family a keyed turn's end is kept under, by its session and its key.
pub const PREFIX: &str = "turn_end:";

/// What a keyed turn's end is kept as: what the send answers of it, the session and the key it stands under,
/// and when it ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptEnd {
  pub session: String,
  pub key: String,
  pub turn: i64,
  pub outcome: String,
  pub delivered: bool,
  #[serde(default)]
  pub ending: Option<String>,
  #[serde(default)]
  pub reason: Option<String>,
  #[serde(default)]
  pub answer: Option<String>,
  /// Each turn's answer the round spanned, which `answer` was built from.
  #[serde(default)]
  pub answers: Vec<TurnAnswer>,
  /// The program's own session the turn answered on, which a later start names to continue it.
  #[serde(default)]
  pub harness_session: Option<String>,
  pub at: i64,
}

impl KeptEnd {
  /// A turn's end as it is kept under its session and key.
  pub fn of(session: &str, key: &str, end: &TurnEnd) -> Self {
    Self {
      session: session.to_owned(),
      key: key.to_owned(),
      turn: end.turn,
      outcome: outcome_word(end.outcome),
      delivered: end.delivery == Delivery::Taken,
      ending: end.ending.clone(),
      reason: end.reason.clone(),
      answer: end.answer.clone(),
      answers: end.answers.clone(),
      harness_session: end.reply.as_ref().and_then(|reply| reply.session_id.clone()),
      at: now_ms(),
    }
  }

  /// The send's answer for this end, echoing the prompt and the context of the send it answers.
  pub fn answer(&self, prompt: &str, context: Value) -> Value {
    json!({
      "id": self.turn,
      "sessionId": self.session,
      "prompt": prompt,
      "context": context,
      "state": if self.outcome == outcome_word(Outcome::Stopped) { "interrupted" } else { "ended" },
      "delivery": if self.delivered { "delivered" } else { "blocked" },
      "outcome": self.outcome,
      "ending": self.ending,
      "reason": self.reason,
      "answer": self.answer,
      "answers": self.answers,
      "harnessSession": self.harness_session,
      "text": self.answer.as_ref().map(|answer| json!([answer])).unwrap_or_else(|| json!([])),
      "waiting": Value::Null,
    })
  }
}

/// An outcome as the send answers it.
pub fn outcome_word(outcome: Outcome) -> String {
  format!("{outcome:?}").to_lowercase()
}

/// How a turn standing under a key settled: its end, or the refusal its send met before it began.
#[derive(Clone, Debug)]
pub enum Settled {
  Ended(KeptEnd),
  Refused(Fault),
}

/// A send's claim on its key: the turn standing under it to wait on, its end as kept, or the key held for a
/// turn this send now begins, whose settling it waits on.
pub enum Claim {
  Standing(Waiting),
  Kept(KeptEnd),
  New(Waiting),
}

/// A wait on the turn standing under a key.
pub struct Waiting(watch::Receiver<Option<Settled>>);

impl Waiting {
  /// How the turn settled, once it has; none where the turn was let go of with no settling, which the
  /// process ending is.
  pub async fn settled(mut self) -> Option<Settled> {
    let settled = self.0.wait_for(Option::is_some).await.map(|settled| settled.clone());

    settled.unwrap_or_else(|_| self.0.borrow().clone())
  }
}

// The turns standing under their keys, each by its session and key with the answer its callers wait on.
type Standing = HashMap<(String, String), watch::Sender<Option<Settled>>>;

/// The turns standing under their callers' keys in this process, by session and key.
#[derive(Default)]
pub struct TurnKeys {
  standing: Mutex<Standing>,
}

impl TurnKeys {
  fn standing(&self) -> std::sync::MutexGuard<'_, Standing> {
    self.standing.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
  }

  /// A send's claim on its key: attached to the turn standing under it, answered with its end where one is
  /// kept, or holding the key for the turn it begins. The key is held before the store is read, so two sends
  /// of one key never both begin a turn.
  pub async fn claim(&self, db: &sea_orm::DatabaseConnection, session: &str, key: &str) -> Claim {
    let waiting = {
      let mut standing = self.standing();
      let named = (session.to_owned(), key.to_owned());

      if let Some(held) = standing.get(&named) {
        return Claim::Standing(Waiting(held.subscribe()));
      }

      let (held, waiting) = watch::channel(None);

      standing.insert(named, held);
      Waiting(waiting)
    };

    match kept(db, session, key).await {
      Some(end) => {
        self.settle(session, key, Settled::Ended(end.clone()));
        Claim::Kept(end)
      }
      None => Claim::New(waiting),
    }
  }

  /// A turn picked up at the server's start held under its key, so a repeat of its send attaches to it.
  pub fn stand(&self, session: &str, key: &str) {
    self
      .standing()
      .entry((session.to_owned(), key.to_owned()))
      .or_insert_with(|| watch::Sender::new(None));
  }

  /// Whether a turn stands under a key in this process.
  pub fn holds(&self, session: &str, key: &str) -> bool {
    self.standing().contains_key(&(session.to_owned(), key.to_owned()))
  }

  /// The turn under a key settled: every caller waiting on it is answered, and the key no longer holds a
  /// standing turn. A key holding none is left as it is.
  pub fn settle(&self, session: &str, key: &str, settled: Settled) {
    if let Some(held) = self.standing().remove(&(session.to_owned(), key.to_owned())) {
      held.send_replace(Some(settled));
    }
  }

  /// A keyed turn's end kept in the store and handed to every caller waiting on its key. A store that would
  /// not take it is said and never fails the turn: the callers waiting now are answered all the same.
  pub async fn ended(&self, db: &sea_orm::DatabaseConnection, session: &str, key: &str, end: &TurnEnd) {
    let kept = KeptEnd::of(session, key, end);
    let text = serde_json::to_string(&kept).unwrap_or_default();

    if let Err(why) = settings::set(db, &setting(session, key), &text).await {
      eprintln!("{session}: the end of the turn under key {key} was not kept: {}", why.detail);
    }

    self.settle(session, key, Settled::Ended(kept));
  }
}

// The settings key a session's keyed end stands under.
fn setting(session: &str, key: &str) -> String {
  format!("{PREFIX}{session}:{key}")
}

/// The end kept under a session's key, where one is kept, is the session's and the key's, and is younger than
/// the span the policy keeps one for.
pub async fn kept(db: &sea_orm::DatabaseConnection, session: &str, key: &str) -> Option<KeptEnd> {
  let text = settings::get(db, &setting(session, key)).await.ok().flatten()?;
  let kept: KeptEnd = serde_json::from_str(&text).ok()?;

  (kept.session == session && kept.key == key && !stale(&kept, now_ms())).then_some(kept)
}

// Whether a kept end is past the span the policy keeps one for.
fn stale(kept: &KeptEnd, now: i64) -> bool {
  let span = i64::try_from(TURN_KEYS.kept_seconds.saturating_mul(1000)).unwrap_or(i64::MAX);

  now.saturating_sub(kept.at) > span
}

/// The kept ends past the policy's span, and those that do not read, dropped; how many were.
pub async fn prune(db: &sea_orm::DatabaseConnection) -> usize {
  let now = now_ms();
  let mut dropped = 0;

  for rest in settings::named(db, PREFIX).await.unwrap_or_default() {
    let name = format!("{PREFIX}{rest}");
    let Ok(Some(text)) = settings::get(db, &name).await else { continue };
    let gone = serde_json::from_str::<KeptEnd>(&text).map_or(true, |kept| stale(&kept, now));

    if gone && settings::forget(db, &name).await.is_ok() {
      dropped += 1;
    }
  }

  dropped
}
