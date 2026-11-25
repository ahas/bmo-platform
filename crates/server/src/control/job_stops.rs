//! A job a supervisor stops (docs/api.md, 「assignments and jobs」): by its id or by its item, only for a turn
//! the receiver marked privileged, so whether the owner asked for the stop is agency's reading of the turn and
//! never the model's word.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use bmo_agency_client::JobStopAsk;
use store::one;

use super::{Call, Refused, Route, answer};
use crate::turn_marks;

/// One job stopped for the turn an agent carries now.
pub struct StopJob;

impl Route for StopJob {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/job-stops";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let bad = |why: String| Refused::new(StatusCode::BAD_REQUEST, why).with_code("bad-request");
    let ask: JobStopAsk =
      serde_json::from_value(call.body().await?).map_err(|why| bad(format!("a job's stop: {why}")))?;
    let reason = ask.reason.trim();

    if reason.is_empty() {
      return Err(bad("a job's stop: reason says why, in a short line".to_owned()));
    }

    let Some(mark) = turn_marks::of(&ask.agent_name) else {
      return Err(
        Refused::new(
          StatusCode::CONFLICT,
          format!("{} is carrying no turn to stop a job for", ask.agent_name),
        )
        .with_code("no-turn"),
      );
    };

    if !mark.privileged {
      return Err(
        Refused::new(
          StatusCode::CONFLICT,
          format!("{}'s turn is not the owner's alone, and only the owner stops a job", ask.agent_name),
        )
        .with_code("not-owner"),
      );
    }

    let id = match (ask.job_id, ask.item_id) {
      (Some(job), None) => job,
      (None, Some(item)) => running_job_of(call.store(), item).await?,
      _ => return Err(bad("a job's stop names exactly one of jobId and itemId".to_owned())),
    };
    let stopped = super::work::stop_job(call.ground(), &call.stops(), id, reason).await?;

    Ok(answer(StatusCode::OK, stopped))
  }
}

// The newest job of an item that is starting or running.
async fn running_job_of(db: &sea_orm::DatabaseConnection, item: i64) -> Result<i64, Refused> {
  let row = one(
    db,
    r#"SELECT "id" FROM "jobs" WHERE "item_id" = $1 AND "state" IN ('starting', 'running', 'reviewing')
       ORDER BY "id" DESC LIMIT 1"#,
    vec![item.into()],
  )
  .await?;

  match row.and_then(|row| row.try_get::<i64>("", "id").ok()) {
    Some(id) => Ok(id),
    None => Err(
      Refused::new(StatusCode::NOT_FOUND, format!("no job of item {item} is starting or running"))
        .with_code("no-job"),
    ),
  }
}

crate::moved!(StopJob);
