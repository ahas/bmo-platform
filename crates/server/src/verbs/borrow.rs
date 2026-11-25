//! A checkout of another repository borrowed for the rest of a job (docs/worker-slots.md, 「A job writes only in
//! its own checkout」): `bmo job borrow`, run from the job's own shell, asks agency's server's `POST /borrowings`
//! for the job working in this process's run. It asks with the control token where this process holds one, and
//! with the job's session token otherwise, which is all a job's program is handed. With `--wait` it asks the
//! server to wait for a checkout, says what it waits for each time the server answers that it still waits, and
//! asks again, so the call is never quiet for long.

use bmo_agency_client::{BorrowAsk, Borrowing};
use reqwest::Method;

use super::{CONTROL_TOKEN, VerbError, control_call, control_token};
use crate::lines::token::TOKEN_VARIABLE;

fn refused(why: impl std::fmt::Display) -> VerbError {
  VerbError::Ended {
    code: 1,
    said: format!("borrow: refused — {why}"),
  }
}

/// The token this process asks agency's server with: the control token, else the session token of the run it
/// works in.
fn bearer() -> Option<String> {
  control_token().or_else(|| {
    std::env::var(TOKEN_VARIABLE)
      .ok()
      .map(|token| token.trim().to_owned())
      .filter(|token| !token.is_empty())
  })
}

/// The named repository's checkout lent to the job this process runs for, and the lines saying where it is;
/// waited for where `wait` says so.
pub async fn run(repository: &str, wait: bool) -> Result<Vec<String>, VerbError> {
  let Some(run_name) = std::env::var("BMO_RUN_NAME").ok().filter(|name| !name.is_empty()) else {
    return Err(refused("a checkout is lent to a job, and this is not run inside one"));
  };
  let Some(token) = bearer() else {
    return Err(refused(format!("neither {CONTROL_TOKEN} nor {TOKEN_VARIABLE} is set in this process")));
  };
  let ask = BorrowAsk {
    run_name,
    repository_name: repository.to_owned(),
    wait,
  };
  let body = serde_json::to_value(&ask).map_err(refused)?;
  let (status, answered) = loop {
    let (status, answered) = control_call(&token, Method::POST, "/borrowings", Some(&body))
      .await
      .map_err(|why| refused(format!("agency's server did not answer: {why}")))?;

    if status != 202 {
      break (status, answered);
    }

    let why = answered
      .as_ref()
      .and_then(|said| said["error"].as_str())
      .unwrap_or("every checkout is at work");

    println!("borrow: waiting for a checkout of {repository} to come free — {why}");
  };
  let lent = || -> Result<Borrowing, VerbError> {
    answered
      .clone()
      .and_then(|said| serde_json::from_value(said).ok())
      .ok_or_else(|| refused(format!("agency's server answered {status} with no checkout")))
  };

  match status {
    201 => {
      let lent = lent()?;

      Ok(vec![
        format!("borrow: {} is this job's until it ends — {}'s checkout, on this job's branch.", lent.cwd, lent.worker_name),
        "Work and commit there; one close closes it with this job's other checkouts, each named by --checkout.".to_owned(),
      ])
    }
    200 => {
      let lent = lent()?;

      Ok(vec![format!(
        "borrow: this job already has {}'s checkout: {}",
        lent.worker_name, lent.cwd
      )])
    }
    _ => {
      let error = answered.as_ref().and_then(|said| said["error"].as_str()).map(str::to_owned);

      Err(refused(error.unwrap_or_else(|| format!("agency's server answered {status}"))))
    }
  }
}
