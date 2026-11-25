//! What a supervisor's reset tells the units that hold the rest of it (docs/api.md, 「Events」;
//! docs/supervisors.md, 「Starting it afresh」): each move of its reset point, so the rooms' owner lets go
//! what waited for its turns, and the reset once agency's part is taken, so each unit takes its own rows.

use serde::Serialize;

use crate::events::Event;

/// A supervisor cut off at its reset point: a turn begun at or before it is overtaken.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorCutOff {
  pub agent: String,
  /// The point, in epoch milliseconds.
  pub point: i64,
}

impl Event for SupervisorCutOff {
  const NAME: &'static str = "supervisor-reset.created";
}

/// A supervisor's reset with agency's part taken: the ranges taken, the rooms it answers alone and those it
/// answered with others, the runs its own log named before its lines went, who asked and when, in epoch
/// seconds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorReset {
  pub agent: String,
  pub ranges: Vec<String>,
  pub alone: Vec<i64>,
  pub shared: Vec<i64>,
  /// The runs the supervisor's own log lines went under, so the rooms' owner can take the records written
  /// under them: a chat run's name says its room but not the agent that ran it.
  pub runs: Vec<String>,
  pub by: String,
  pub at: i64,
}

impl Event for SupervisorReset {
  const NAME: &'static str = "supervisor-reset.created";
}
