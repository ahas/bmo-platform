//! Words said to a run, a job or a repository's worker (docs/api.md, 「sessions」, 「assignments and jobs」 and
//! 「repositories, prefix-checks and room-holdings」): the run's own road, by its session's id; a job's, by its
//! id or any session a turn of it ran in, kept until a turn of it takes them; and the repository's, which finds
//! the job running in one of its checkouts or, where none runs there, carries the words in a turn of the
//! worker's own (docs/agent-control.md, 「Words that arrive while a turn is working」, 「Words for a running
//! job」 and 「Words said to a worker」).

use axum::http::{Method, StatusCode};
use axum::response::Response;

use super::{Call, Refused, Route, answer};
use crate::words;

use bmo_agency_client::WordsSent;

fn backend(call: &Call) -> Result<crate::herdr_backend::Shared, Refused> {
  call.sessions().ok_or_else(|| {
    Refused::new(StatusCode::SERVICE_UNAVAILABLE, "agency stands no places to carry sessions on")
  })
}

/// Words said to one run's session, answered with what became of them.
pub struct RunWords;

impl Route for RunWords {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/sessions/{sessionId}/words";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let id = call.param("sessionId").unwrap_or_default().to_owned();
    let ask = words::asked(call.body().await?)?;
    let ground = call.shared_ground();
    let sessions = backend(&call)?;
    let sent: WordsSent = words::to_session(ground, sessions, &id, ask).await?;

    Ok(answer(StatusCode::CREATED, serde_json::json!(sent)))
  }
}

crate::moved!(RunWords);

/// Words said to a job by its id, answered with what became of them (docs/api.md, 「assignments and jobs」).
pub struct JobWords;

impl Route for JobWords {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/jobs/{jobId}/words";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let id = call
      .param("jobId")
      .unwrap_or_default()
      .parse::<i64>()
      .map_err(|_| Refused::new(StatusCode::BAD_REQUEST, "jobId is a number").with_field("jobId"))?;
    let ask = words::asked(call.body().await?)?;
    let sent: WordsSent = words::to_job_named(call.shared_ground(), id, ask).await?;

    Ok(answer(StatusCode::CREATED, serde_json::json!(sent)))
  }
}

crate::moved!(JobWords);

/// Words said to a repository's worker, answered with what became of them.
pub struct RepositoryWords;

impl Route for RepositoryWords {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/repositories/{repositoryName}/words";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let repository = call.param("repositoryName").unwrap_or_default().to_owned();
    let ask = words::asked(call.body().await?)?;
    let sent: WordsSent = words::to_repository(call.shared_ground(), &repository, ask).await?;

    Ok(answer(StatusCode::CREATED, serde_json::json!(sent)))
  }
}

crate::moved!(RepositoryWords);
