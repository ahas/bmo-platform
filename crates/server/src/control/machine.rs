//! This machine's room across the boundary (docs/api.md, 「Workers」, the machine's room): what holds a
//! job's launch now, for the launcher to ask before it launches, and the launcher's notice that it held
//! launches, which agency keeps until the room returns.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::now_ms;
use crate::watches::machine::{LaunchHeld, launch_room, tell_launch_held};

/// What holds a job's launch on this machine now, with the counts the job-count limits are judged on.
pub struct MachineRoom;

impl Route for MachineRoom {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/launch-room";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let ground = call.ground();
    let room = launch_room(&ground.db, ground.peers.node.as_ref()).await?;

    Ok(answer(StatusCode::OK, json!(room)))
  }
}

crate::moved!(MachineRoom);

/// The launcher's notice that launches are held and on what, or its withdrawal where it holds nothing.
pub struct LaunchHeldTold;

impl Route for LaunchHeldTold {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/launch-room";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let bad = |error: &str| Refused::new(StatusCode::BAD_REQUEST, error);
    let held = body
      .get("held")
      .and_then(Value::as_bool)
      .ok_or_else(|| bad("held (a boolean) is required"))?;
    let reasons: Vec<String> = body
      .get("reasons")
      .filter(|_| held)
      .and_then(Value::as_array)
      .map(|reasons| reasons.iter().filter_map(Value::as_str).map(str::to_owned).collect())
      .unwrap_or_default();

    if held && reasons.is_empty() {
      return Err(bad("reasons (a list of strings) is required while launches are held"));
    }

    tell_launch_held(
      call.store(),
      &LaunchHeld {
        at: now_ms(),
        held: reasons,
      },
    )
    .await?;

    let ground = call.ground();
    let room = launch_room(&ground.db, ground.peers.node.as_ref()).await?;

    Ok(answer(StatusCode::OK, json!(room)))
  }
}

crate::moved!(LaunchHeldTold);
