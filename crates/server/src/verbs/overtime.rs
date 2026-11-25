//! The supervisor's verdict on a run past its clock, from a shell (docs/agent-control.md, 「A clock asks
//! before it cuts」): the asks still open, and keep or cut on one of them. The run's door reads the verdict
//! on its next look, within seconds; the ask's closing is published for the consumer that queued its order.

use policy::fill_marks;
use policy::verb_words::VERB_WORDS;

use super::{Answer, VerbError};
use crate::places::Ground;
use crate::run_clock::{self, NotSettled, Verdict};

/// What the verb was asked: the list, or a verdict on an ask with the length a keep holds for and why.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OvertimeAsked {
  pub list: bool,
  pub keep: Option<i64>,
  pub cut: Option<i64>,
  pub length: Option<String>,
  pub why: Option<String>,
}

// A moment in epoch milliseconds as the machine's clock reads it.
fn clock(at_ms: i64) -> String {
  chrono::DateTime::from_timestamp_millis(at_ms)
    .map(|at| at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
    .unwrap_or_else(|| at_ms.to_string())
}

/// A length as a keep takes it, in minutes: `20m`, `1h` or a plain number of minutes. The grammar is this
/// verb's own, two units and a bare count, which no general format describes.
pub fn length_minutes(text: &str) -> Option<i64> {
  let text = text.trim();
  let digits = text.find(|letter: char| !letter.is_ascii_digit()).unwrap_or(text.len());
  let (amount, unit) = text.split_at(digits);
  let amount: i64 = amount.parse().ok().filter(|amount| *amount > 0)?;

  match unit.trim().to_ascii_lowercase().as_str() {
    "" | "m" => Some(amount),
    "h" => amount.checked_mul(60),
    _ => None,
  }
}

/// The verb: the open asks listed, or the verdict given and what it came to.
pub async fn overtime(ground: &Ground, asked: &OvertimeAsked) -> Result<Answer, VerbError> {
  let words = &VERB_WORDS.overtime;

  if asked.list {
    let asks = run_clock::open_asks(&ground.db).await.map_err(VerbError::Store)?;

    if asks.is_empty() {
      return Ok(vec![words.none_open.clone()].into());
    }

    return Ok(
      asks
        .iter()
        .map(|ask| {
          fill_marks(
            &words.open,
            &[
              ("ask", &ask.ask.to_string()),
              ("run", &ask.run),
              ("what", &ask.what),
              ("asked_at", &clock(ask.asked_at)),
            ],
          )
        })
        .collect::<Vec<_>>()
        .into(),
    );
  }

  let (ask, verdict) = match (asked.keep, asked.cut) {
    (Some(ask), None) => {
      let minutes = match asked.length.as_deref() {
        Some(length) => Some(
          length_minutes(length)
            .ok_or_else(|| VerbError::Refused(fill_marks(&words.length, &[("length", length)])))?,
        ),
        None => None,
      };

      (ask, Verdict::Keep(minutes))
    }
    (None, Some(ask)) => (ask, Verdict::Cut),
    _ => return Err(VerbError::Refused(words.usage.clone())),
  };
  let named = ask.to_string();

  match run_clock::settle(
    &ground.db,
    &ground.events,
    ask,
    &verdict,
    asked.why.as_deref().unwrap_or_default(),
  )
  .await
  {
    Ok(settled) => {
      let line = match settled.kept_until {
        Some(until) => {
          fill_marks(&words.kept, &[("ask", &named), ("run", &settled.run), ("until", &clock(until))])
        }
        None => fill_marks(&words.cut, &[("ask", &named), ("run", &settled.run)]),
      };

      Ok(vec![line].into())
    }
    Err(NotSettled::Unknown) => Err(VerbError::Refused(fill_marks(&words.unknown, &[("ask", &named)]))),
    Err(NotSettled::Closed) => Err(VerbError::Refused(fill_marks(&words.closed, &[("ask", &named)]))),
    Err(NotSettled::Store(why)) => Err(VerbError::Store(why)),
  }
}
