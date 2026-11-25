//! What a reset cuts off, as it begins and again once the store holds nothing of what it took
//! (docs/supervisors.md, 「What the reset cuts off」): the supervisor's reset point moved to now under the
//! hold its turns keep under, so every turn begun until then is overtaken; a cancellation stood over every
//! run of its own going, so an overtaken turn stops where it stands; and the move published with the
//! point, so the rooms' owner lets go what waited for its turns. The point moves only while no turn of the
//! supervisor is keeping what it came to.

use crate::events::Bus;
use crate::queue::running::EXECUTION_CANCEL_PREFIX;
use crate::queue::settings;
use crate::supervisor_turn::going::GoingRuns;
use crate::supervisor_turn::overtaken::{ResetHold, reset_point_key};
use crate::supervisor_turn::reset::SupervisorCutOff;
use sea_orm::DatabaseConnection;

use super::super::Refused;

/// The supervisor cut off at this moment.
pub async fn cut_off(
  db: &DatabaseConnection,
  events: &Bus,
  hold: &ResetHold,
  going: &GoingRuns,
  agent: &str,
) -> Result<(), Refused> {
  // A turn keeping what it came to keeps it whole before the point moves, so the reset takes it after; one
  // that would keep from now reads the point under the same hold and finds itself overtaken.
  let _cutting = hold.cutting().await;
  let at = crate::now_ms();

  // The point first, so a round taking words once they are let go already reads it.
  settings::set(db, &reset_point_key(agent), &at.to_string()).await?;

  for run in going.of(agent) {
    settings::set(db, &format!("{EXECUTION_CANCEL_PREFIX}{run}"), &at.to_string()).await?;
  }

  events.publish(SupervisorCutOff {
    agent: agent.to_owned(),
    point: at,
  });

  Ok(())
}
