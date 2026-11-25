//! A checkout of another repository lent to a running job (docs/api.md, 「assignments and jobs」;
//! docs/worker-slots.md, 「A job writes only in its own checkout」, 「A checkout lent to a job」): chosen, held in
//! the job's name, made and put on the job's branch as every loan is, so everything that reads what is held reads
//! it and nothing is launched into it. It stands until the job gives its checkout up. The job's own program asks
//! for it with its session token, since it holds no control token, and is lent only for the job running in its
//! own checkout. An ask that waits holds until a checkout comes free, for at most the policy's seconds, and is
//! then answered that it still waits, for the verb to ask again.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use bmo_agency_client::{BorrowAsk, Borrowing};
use policy::workers::WORKERS;
use store::one;

use super::loans::{Borrower, Lending, loan_watch, next_look};
use super::{Bearer, Call, Refused, Route, answer};
use crate::queue::holds::release_ended;
use crate::queue::workers::workers;

/// The variable a deployment names, in milliseconds, how long a borrowing that waits holds its ask, in place of
/// the policy's seconds.
pub const BORROW_WAIT: &str = "BMO_BORROW_WAIT_MS";

fn refused(status: StatusCode, code: &str, error: String) -> Refused {
  Refused::new(status, error).with_code(code)
}

/// A checkout lent to the job working in the run the ask names.
pub struct Borrow;

impl Route for Borrow {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/borrowings";
  const TAKES_RUN_TOKEN: bool = true;

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let bad = |why: String| refused(StatusCode::BAD_REQUEST, "bad-request", why);
    let ask: BorrowAsk =
      serde_json::from_value(call.body().await?).map_err(|why| bad(format!("a borrowing: {why}")))?;
    let run_name = ask.run_name.trim();
    let repository = ask.repository_name.trim();

    if run_name.is_empty() || repository.is_empty() {
      return Err(bad("a borrowing names its runName and its repositoryName".to_owned()));
    }

    let ground = call.ground();
    let job = one(
      &ground.db,
      r#"SELECT "id", "worker_name", "holder_name" FROM "jobs"
         WHERE "run_name" = $1 AND "state" IN ('starting', 'running')
         ORDER BY "id" DESC LIMIT 1"#,
      vec![run_name.into()],
    )
    .await?
    .ok_or_else(|| refused(StatusCode::NOT_FOUND, "no-job", format!("no job runs under {run_name}")))?;
    let job_id: i64 = job.try_get("", "id")?;
    let own: String = job.try_get("", "worker_name")?;

    if let Bearer::Run(holder) = call.bearer()
      && *holder != own
    {
      return Err(refused(
        StatusCode::FORBIDDEN,
        "not-this-job",
        format!("the session token is {holder}'s, and the job working in {run_name} runs in {own}"),
      ));
    }

    // A job borrows only a repository its holder's work may stand on (docs/api.md, 「assignments and jobs」).
    let holder: String = job.try_get("", "holder_name")?;
    let account = crate::supply::account_of(ground, &holder).await?;

    crate::access::works_on(ground, account, repository).await?;

    let borrower = Borrower::of_job(ground, job_id)
      .await?
      .ok_or_else(|| refused(StatusCode::NOT_FOUND, "no-job", format!("no job runs under {run_name}")))?;
    let bound = ground.vars().get(BORROW_WAIT).and_then(|ms| ms.trim().parse().ok()).map_or(
      std::time::Duration::from_secs(WORKERS.borrow_wait_seconds),
      std::time::Duration::from_millis,
    );
    let until = std::time::Instant::now() + bound;
    let mut moved = loan_watch(ground);

    loop {
      release_ended(&ground.db).await?;

      let known = workers(&ground.db, &ground.source).await?;
      let busy = match borrower.lend(ground, &known, repository).await? {
        Lending::Lent(lent) => {
          // One the job holds already stands where the job left it.
          if lent.fresh {
            borrower.place(ground, &lent).await?;
          }

          let borrowing = Borrowing {
            worker_name: lent.checkout.name.clone(),
            cwd: lent.checkout.cwd.clone(),
            assignment_id: lent.assignment_id,
          };
          let status = if lent.fresh { StatusCode::CREATED } else { StatusCode::OK };

          return Ok(answer(status, serde_json::to_value(borrowing).map_err(Refused::failed)?));
        }
        Lending::Busy(why) => why,
      };

      if !ask.wait {
        return Err(refused(
          StatusCode::UNPROCESSABLE_ENTITY,
          "no-worker-free",
          format!("{busy}; read which with bmo run list, and borrow once one is free, or wait with --wait"),
        ));
      }

      if std::time::Instant::now() >= until {
        return Err(refused(StatusCode::ACCEPTED, "still-waiting", busy));
      }

      // The wait never outlasts its bound, whatever look comes round after it.
      let left = until.saturating_duration_since(std::time::Instant::now());
      let _ = tokio::time::timeout(left, next_look(&mut moved)).await;
    }
  }
}

crate::moved!(Borrow);
