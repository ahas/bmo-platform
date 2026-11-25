#![allow(
  dead_code,
  unused_imports,
  unused_variables,
  clippy::needless_lifetimes,
  clippy::let_and_return,
  clippy::unnecessary_to_owned
)]
//! What a job's run asks of its repository across the boundary (docs/api.md, 「repositories, prefix-checks and
//! room-holdings」): what its ended runs left taken down, the documents a job is composed with, and which of a
//! change's files a session's prefix is built from.

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use super::{Call, Refused, Route, answer};
use crate::lines::prefix::touches_prefix;
use crate::lines::standing::session_ttl_ms;
use crate::lines::worker::{rules_of, worker_run};
use crate::lines::{Trees, read_required, render};
use crate::places::{AgentRow, dev};
use crate::worker_session::held::{self, SessionRun};
use crate::worker_session::{PROVIDER, records};

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

// The repository the path names.
fn worker_of(call: &Call) -> Result<String, Refused> {
  call
    .param("repositoryName")
    .filter(|repository| !repository.is_empty())
    .map(str::to_owned)
    .ok_or_else(|| bad("a repository's name is required"))
}

async fn read_body<T: DeserializeOwned>(call: &mut Call, what: &str) -> Result<T, Refused> {
  serde_json::from_value(call.body().await?).map_err(|error| bad(format!("{what}: {error}")))
}

/// A handover as the job's runner asks it.
#[derive(Deserialize)]
struct Handover {
  model: String,
  cwd: String,
  session: Option<String>,
}

/// What the ended runs in the checkout left running taken down (docs/run-groups.md).
pub struct SweepServers;

impl Route for SweepServers {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/repositories/{repositoryName}/servers";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let worker = worker_of(&call)?;
    let ground = call.ground();
    let row = AgentRow::named(&ground.db, &worker)
      .await?
      .ok_or_else(|| Refused::new(StatusCode::NOT_FOUND, format!("no checkout is named {worker}")))?;
    let checkout = ground.checkout_of(&row);
    // A copy of the data root takes down nothing of the machine's.
    let lines = if ground.roots.own_data_root() {
      crate::run_groups::sweep(&ground.roots.data, Some(std::path::Path::new(&checkout))).await
    } else {
      vec![]
    };

    Ok(answer(StatusCode::OK, json!({ "lines": lines })))
  }
}

crate::moved!(SweepServers);

/// What a worker's agent definition gives a job: the trailer with its paths filled in as the
/// deployment shows them, and the worker's own handoff instruction where its folder holds one.
pub struct JobDocuments;

impl Route for JobDocuments {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/repositories/{repositoryName}/definition";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let worker = worker_of(&call)?;
    let ground = call.ground();
    let trees = Trees::of(ground);
    let documents = trees.agent_dir("worker");
    // The trailer names the paths by the deployment's link, the documents being read out of the job's tree.
    let shown = Trees {
      code: ground.shown_code_root(),
      ..trees.clone()
    };
    let trailer = crate::lines::worker::job_trailer(&trees, &shown)?;
    let handoff = rules_of(ground, &worker)
      .await?
      .and_then(|folder| std::fs::read_to_string(trees.agent_dir(&folder).join("handoff.md")).ok())
      .map(|text| text.trim().to_owned());

    Ok(answer(StatusCode::OK, json!({ "trailer": trailer, "handoff": handoff })))
  }
}

crate::moved!(JobDocuments);

/// The files a change moved, as the job's runner names them.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Moved {
  #[serde(default)]
  worker_name: Option<String>,
  files: Vec<String>,
}

/// Which of a change's files a worker session's prefix is built from.
pub struct PrefixTouched;

impl Route for PrefixTouched {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/prefix-checks";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let moved: Moved = read_body(&mut call, "a change's files").await?;
    let worker = moved.worker_name.clone().unwrap_or_default();

    if !AgentRow::named(call.store(), &worker).await?.is_some_and(|row| row.kind == "worker") {
      return Err(bad(format!("no worker is named {worker}")));
    }

    Ok(answer(StatusCode::OK, json!({ "touched": touches_prefix(&moved.files) })))
  }
}

crate::moved!(PrefixTouched);
