//! What the chat agent tells the owner of a question a job put to them, and what a confirmation a job's pane stopped at
//! is asked as, each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobQuestions {
  pub wait: Wait,
  pub asked: Asked,
  pub pane: Pane,
}

/// A wait as the room is told it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wait {
  pub minutes: String,
  pub seconds: String,
}

/// The question put, talked over, and settled.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asked {
  pub lead: String,
  pub removed_room: String,
  pub option: String,
  pub recommended: String,
  pub why: String,
  pub answer: String,
  pub answer_talk: String,
  pub talk_held: String,
  pub job_says: String,
  pub revised: String,
  pub answer_more: String,
  pub chosen: String,
  pub typed: String,
  pub talk_lapsed: String,
  pub stop_lapsed: String,
  pub lapsed: String,
}

/// A confirmation a job's pane stopped at, put to the owner as a question.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pane {
  pub allow: String,
  pub refuse: String,
  pub close: String,
  pub leave: String,
  pub permission: String,
  pub unknown: String,
  pub unknown_text: String,
  pub why: String,
  pub lead: String,
  pub withdrawn: String,
  pub own_work: String,
  pub answered_in_pane: String,
}

crate::part!(pub JOB_QUESTIONS: JobQuestions = "job-questions");
