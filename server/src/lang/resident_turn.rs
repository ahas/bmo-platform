//! What a turn put into a standing program writes as it goes (`lang/<language>/resident-turn.yaml`), each line with the `{name}`
//! marks its file names.

use serde::Deserialize;

/// What a turn put into a standing program writes as it goes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnWords {
  pub notes: Notes,
  pub screens: Screens,
  pub confirm: Confirm,
}

/// The `confirm` lines.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Confirm {
  pub mark: String,
  pub left_to_person: String,
  pub no_allow: String,
  pub allowed: String,
  pub refused_with_reason: String,
  pub refused: String,
  pub closed: String,
  pub tries: String,
  pub again: String,
  pub answered: String,
  pub changed: String,
  pub repeated: String,
  pub closed_early: String,
  pub stopped_at: String,
  pub unasked: String,
}

/// The `screens` lines.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Screens {
  pub vanished: String,
  pub not_ready: String,
  pub not_taken: String,
  pub unsubmitted: String,
  pub stopped: String,
  pub overran: String,
  pub silent: String,
  pub unknown_window: String,
}

/// The `notes` lines.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notes {
  pub title: String,
  pub trust_written: String,
  pub trust_unwritten: String,
  pub trusted_folder: String,
  pub person_turn: String,
  pub stand_again: String,
  pub untrusted: String,
  pub not_ready: String,
  pub not_taken: String,
  pub herdr_silent_ready: String,
  pub enter_pressed: String,
  pub unsubmitted: String,
  pub overrun_unasked: String,
  pub herdr_silent: String,
  pub herdr_gave_up: String,
  pub herdr_back: String,
  pub quiet: String,
  pub error_menu: String,
}

crate::part!(pub TURN_WORDS: TurnWords = "resident-turn");
