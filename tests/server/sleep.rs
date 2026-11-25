//! The agents' sleep (docs/sleep.md): the clock's judgement driven by moments a test hands it — the hour, the
//! window, the day a window past midnight is for, one try an hour, a day slept and a setting turned off; one
//! supervisor's sleep through a stand-in for what it reaches — put off under a card's claim, passed over with no
//! brain, rested with nothing to sleep on, and otherwise run, its summary written labelled with the day and its owner
//! told in one line; and the pass over the store, recording each try and each day slept.

use std::sync::Mutex;

use agency_server::peers::brain::Pending;
use agency_server::sleep::{
  Going, Judged, Night, Outcome, Report, Setting, claimed_in, instruction, judged, pass, report_of, set,
  setting, sleep_one,
};
use policy::sleep::{SLEEP, Sleep};
use serde_json::{Value, json};

use crate::support::scratch::Scratch;

const HOUR: i64 = 60 * 60 * 1_000;

/// 2026-11-24 00:00 on the deployment's clock, in epoch milliseconds.
fn midnight() -> i64 {
  chrono::NaiveDate::from_ymd_opt(2026, 11, 24)
    .unwrap()
    .and_hms_opt(0, 0, 0)
    .unwrap()
    .and_utc()
    .timestamp_millis()
    - i64::from(SLEEP.utc_offset_hours) * HOUR
}

fn rules() -> Sleep {
  Sleep {
    utc_offset_hours: SLEEP.utc_offset_hours,
    hour: 4,
    window_hours: 3,
    tick_seconds: SLEEP.tick_seconds,
    start_delay_seconds: SLEEP.start_delay_seconds,
    budget_tokens: SLEEP.budget_tokens,
    instruction: SLEEP.instruction.clone(),
  }
}

fn due(date: &str) -> Judged {
  Judged::Due {
    date: date.to_owned(),
  }
}

#[test]
fn a_supervisor_is_due_within_its_window_once_a_day_and_once_an_hour() {
  let rules = rules();
  let fresh = Setting::default();
  let at = |hours: f64| midnight() + (hours * HOUR as f64) as i64;

  assert_eq!(judged(&fresh, &rules, at(3.99)), Judged::Outside);
  assert_eq!(judged(&fresh, &rules, at(4.0)), due("2026-11-24"));
  assert_eq!(judged(&fresh, &rules, at(6.9)), due("2026-11-24"));
  assert_eq!(judged(&fresh, &rules, at(7.0)), Judged::Outside);

  // Tried at 04:10: not again before 05:00, and again then.
  let tried = Setting {
    tried_at: Some(at(4.17)),
    ..Setting::default()
  };

  assert_eq!(judged(&tried, &rules, at(4.9)), Judged::TriedThisHour);
  assert_eq!(judged(&tried, &rules, at(5.0)), due("2026-11-24"));

  // Slept for the day: nothing more that window, and the next day's window is due again.
  let slept = Setting {
    slept_on: Some("2026-11-24".to_owned()),
    ..Setting::default()
  };

  assert_eq!(judged(&slept, &rules, at(5.5)), Judged::Slept);
  assert_eq!(judged(&slept, &rules, at(28.0)), due("2026-11-25"));

  let off = Setting {
    on: false,
    ..Setting::default()
  };

  assert_eq!(judged(&off, &rules, at(4.5)), Judged::Off);
}

#[test]
fn a_window_past_midnight_is_for_the_day_it_began_on_and_a_supervisors_own_hour_stands() {
  let rules = rules();
  let late = Setting {
    hour: Some(23),
    ..Setting::default()
  };

  assert_eq!(judged(&late, &rules, midnight() + 23 * HOUR + HOUR / 2), due("2026-11-24"));
  assert_eq!(judged(&late, &rules, midnight() + 25 * HOUR + HOUR / 2), due("2026-11-24"));
  assert_eq!(judged(&late, &rules, midnight() + 26 * HOUR), Judged::Outside);
  assert_eq!(judged(&late, &rules, midnight() + 4 * HOUR), Judged::Outside);
}

#[test]
fn the_instruction_names_the_day_and_the_budget_and_the_answer_is_read_out_of_its_words() {
  let asked = instruction(&SLEEP, "2026-11-24");

  assert!(
    asked.contains("2026-11-24") && asked.contains(&SLEEP.budget_tokens.to_string()),
    "{asked}"
  );
  assert!(!asked.contains("{date}") && !asked.contains("{budget}"));

  let fenced = "Done.\n```json\n{\"summary\": \"Learned two facts.\", \"learned\": 2, \"merged\": 1, \"closed\": 0}\n```";

  assert_eq!(
    report_of(fenced),
    Some(Report {
      summary: "Learned two facts.".to_owned(),
      learned: 2,
      merged: 1,
      closed: 0,
    })
  );
  assert_eq!(report_of("I slept well."), None);
  assert_eq!(report_of("{\"summary\": \"  \"}"), None);
}

#[test]
fn a_card_under_a_claim_anywhere_holds_the_cards() {
  let free = json!({ "nodes": [{ "cards": [{ "claim": null, "turn": { "serviceName": "llm" } }] }] });
  let claimed = json!({ "nodes": [{ "cards": [{ "claim": null }] }, { "cards": [{ "claim": { "claimantName": "owner-a" } }] }] });

  assert!(!claimed_in(&free));
  assert!(claimed_in(&claimed));
  assert!(!claimed_in(&json!({})));
}

/// A stand-in for what a sleep reaches, recording what it was asked.
#[derive(Default)]
struct StandIn {
  claimed: bool,
  pending: Option<Pending>,
  answer: String,
  asked: Mutex<Vec<String>>,
  summaries: Mutex<Vec<(String, String, String)>>,
  told: Mutex<Vec<(String, Value)>>,
}

impl Night for StandIn {
  fn claimed(&self) -> Going<'_, bool> {
    Box::pin(async move { Ok(self.claimed) })
  }

  fn pending<'a>(&'a self, _agent: &'a str) -> Going<'a, Option<Pending>> {
    Box::pin(async move { Ok(self.pending) })
  }

  fn sleep<'a>(&'a self, agent: &'a str, instruction: &'a str) -> Going<'a, String> {
    Box::pin(async move {
      self.asked.lock().unwrap().push(format!("{agent}: {instruction}"));

      Ok(self.answer.clone())
    })
  }

  fn summary<'a>(&'a self, agent: &'a str, date: &'a str, summary: &'a str) -> Going<'a, ()> {
    Box::pin(async move {
      self
        .summaries
        .lock()
        .unwrap()
        .push((agent.to_owned(), date.to_owned(), summary.to_owned()));

      Ok(())
    })
  }

  fn tell<'a>(&'a self, agent: &'a str, line: Value) -> Going<'a, ()> {
    Box::pin(async move {
      self.told.lock().unwrap().push((agent.to_owned(), line));

      Ok(())
    })
  }
}

const ANSWER: &str = r#"{"summary": "Learned that the member keeps bees; merged two names; closed one promise.", "learned": 3, "merged": 1, "closed": 1}"#;

fn waiting() -> Option<Pending> {
  Some(Pending {
    unconsolidated: 4,
    written_since: 9,
  })
}

#[tokio::test]
async fn a_sleep_writes_one_summary_labelled_with_its_day_and_tells_the_owner_in_one_line() {
  let night = StandIn {
    pending: waiting(),
    answer: ANSWER.to_owned(),
    ..StandIn::default()
  };

  let came = sleep_one(&night, "agent-a", "2026-11-24").await;

  assert!(
    matches!(&came, Outcome::Slept { told: true, report } if report.learned == 3),
    "{came:?}"
  );
  assert_eq!(night.asked.lock().unwrap().len(), 1);
  assert_eq!(
    *night.summaries.lock().unwrap(),
    [(
      "agent-a".to_owned(),
      "2026-11-24".to_owned(),
      "Learned that the member keeps bees; merged two names; closed one promise.".to_owned()
    )]
  );
  assert_eq!(
    *night.told.lock().unwrap(),
    [(
      "agent-a".to_owned(),
      json!({ "code": "slept", "agent": "agent-a", "date": "2026-11-24", "learned": 3, "merged": 1, "closed": 1 })
    )]
  );
}

#[tokio::test]
async fn a_claim_puts_the_sleep_off_and_nothing_to_sleep_on_or_no_brain_runs_nothing() {
  let claimed = StandIn {
    claimed: true,
    pending: waiting(),
    answer: ANSWER.to_owned(),
    ..StandIn::default()
  };

  assert_eq!(sleep_one(&claimed, "agent-a", "2026-11-24").await, Outcome::Claimed);
  assert!(claimed.asked.lock().unwrap().is_empty(), "no run went out under a claim");

  let rested = StandIn {
    pending: Some(Pending::default()),
    answer: ANSWER.to_owned(),
    ..StandIn::default()
  };

  assert_eq!(sleep_one(&rested, "agent-a", "2026-11-24").await, Outcome::Rested);
  assert!(rested.asked.lock().unwrap().is_empty());

  let brainless = StandIn {
    answer: ANSWER.to_owned(),
    ..StandIn::default()
  };

  assert_eq!(sleep_one(&brainless, "agent-a", "2026-11-24").await, Outcome::NoBrain);

  let unread = StandIn {
    pending: waiting(),
    answer: "I tidied things up.".to_owned(),
    ..StandIn::default()
  };

  assert!(matches!(sleep_one(&unread, "agent-a", "2026-11-24").await, Outcome::Failed(_)));
  assert!(unread.summaries.lock().unwrap().is_empty() && unread.told.lock().unwrap().is_empty());
}

async fn supervisor(scratch: &Scratch, name: &str) {
  scratch
    .run(
      r#"INSERT INTO "supervisors" ("name", "session_scope", "turn_shape", "created_at", "updated_at")
         VALUES ($1, 'room', 'pieces', 0, 0)"#,
      vec![name.into()],
    )
    .await;
}

#[tokio::test]
async fn the_pass_records_each_try_and_each_day_slept_and_holds_a_supervisors_own_setting() {
  let scratch = Scratch::new().await;

  supervisor(&scratch, "agent-a").await;
  supervisor(&scratch, "agent-b").await;
  set(scratch.db(), "agent-b", false, None).await.unwrap();

  let rules = rules();
  let at_four = midnight() + 4 * HOUR + 60_000;

  // Under a claim, the try is recorded and the day stays unslept.
  let claimed = StandIn {
    claimed: true,
    pending: waiting(),
    answer: ANSWER.to_owned(),
    ..StandIn::default()
  };
  let came = pass(scratch.db(), &claimed, &rules, at_four).await.unwrap();

  assert_eq!(came, [("agent-a".to_owned(), Outcome::Claimed)], "agent-b sleeps not");
  assert_eq!(setting(scratch.db(), "agent-a").await.unwrap().tried_at, Some(at_four));
  assert_eq!(setting(scratch.db(), "agent-a").await.unwrap().slept_on, None);

  // Within the same hour nothing is tried; the next hour the cards are free and it sleeps.
  let night = StandIn {
    pending: waiting(),
    answer: ANSWER.to_owned(),
    ..StandIn::default()
  };

  assert!(pass(scratch.db(), &night, &rules, at_four + 10 * 60_000).await.unwrap().is_empty());

  let came = pass(scratch.db(), &night, &rules, at_four + HOUR).await.unwrap();

  assert!(
    matches!(came.as_slice(), [(agent, Outcome::Slept { .. })] if agent == "agent-a"),
    "{came:?}"
  );
  assert_eq!(setting(scratch.db(), "agent-a").await.unwrap().slept_on.as_deref(), Some("2026-11-24"));
  assert!(
    pass(scratch.db(), &night, &rules, at_four + 2 * HOUR).await.unwrap().is_empty(),
    "slept for the day"
  );

  // A supervisor's own hour and switch stand as written.
  set(scratch.db(), "agent-b", true, Some(23)).await.unwrap();
  assert_eq!(
    setting(scratch.db(), "agent-b").await.unwrap(),
    Setting {
      on: true,
      hour: Some(23),
      slept_on: None,
      tried_at: None,
    }
  );
}
