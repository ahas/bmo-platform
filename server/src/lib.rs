//! The account service (docs/accounts.md): sumida's users, sessions, API keys and permission layers with
//! the deployment's rules beside sumida's, and the routes the console and the other units ask it on. It
//! stands on its own database and holds the decisions; what a screen or a room does with an answer is its
//! caller's.

pub mod checks;
pub mod data_move;
pub mod grants;
pub mod marks;
pub mod migration;
pub mod phones;
pub mod receiver;
pub mod routes;
pub mod rules;
pub mod settings;
pub mod store;
pub mod users;

use std::sync::Arc;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use sea_orm::{ConnectionTrait, DatabaseConnection, QueryResult, Value};
use serde_json::json;
use sumida::api::SumidaState;
use sumida::error::SumidaApiError;

use crate::receiver::Receiver;

/// The server's version, the repository's.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What every handler stands on: the service's database, where sumida's tables stand beside the service's
/// own, and the receiver a message's sender is asked of.
#[derive(Clone)]
pub struct Service {
  pub db: DatabaseConnection,
  pub receiver: Arc<dyn Receiver>,
}

impl SumidaState for Service {
  fn db(&self) -> &DatabaseConnection {
    &self.db
  }
}

/// A refusal, said with its status and a sentence naming why.
#[derive(Debug, thiserror::Error)]
#[error("{detail}")]
pub struct Refusal {
  pub status: StatusCode,
  pub detail: String,
}

impl Refusal {
  pub fn new(status: StatusCode, detail: impl Into<String>) -> Self {
    Self {
      status,
      detail: detail.into(),
    }
  }

  pub fn bad(detail: impl Into<String>) -> Self {
    Self::new(StatusCode::BAD_REQUEST, detail)
  }

  pub fn forbidden(detail: impl Into<String>) -> Self {
    Self::new(StatusCode::FORBIDDEN, detail)
  }

  pub fn absent(detail: impl Into<String>) -> Self {
    Self::new(StatusCode::NOT_FOUND, detail)
  }
}

impl From<sea_orm::DbErr> for Refusal {
  fn from(error: sea_orm::DbErr) -> Self {
    Self::new(StatusCode::INTERNAL_SERVER_ERROR, format!("the service's database: {error}"))
  }
}

impl From<sumida::api::authz::AuthzError> for Refusal {
  fn from(error: sumida::api::authz::AuthzError) -> Self {
    Self::new(StatusCode::INTERNAL_SERVER_ERROR, format!("the rules: {error}"))
  }
}

impl From<sumida::error::SumidaError> for Refusal {
  fn from(error: sumida::error::SumidaError) -> Self {
    let status = error.status_code();

    Self::new(status, error.to_string())
  }
}

impl IntoResponse for Refusal {
  fn into_response(self) -> Response {
    (self.status, Json(json!({ "detail": self.detail }))).into_response()
  }
}

/// The one row a statement answers, or none.
pub async fn one<C: ConnectionTrait>(
  db: &C,
  sql: &str,
  values: Vec<Value>,
) -> Result<Option<QueryResult>, Refusal> {
  Ok(store::one(db, sql, values).await?)
}

/// Every row a statement answers.
pub async fn all<C: ConnectionTrait>(
  db: &C,
  sql: &str,
  values: Vec<Value>,
) -> Result<Vec<QueryResult>, Refusal> {
  Ok(store::all(db, sql, values).await?)
}

/// A statement run for what it writes.
pub async fn run<C: ConnectionTrait>(db: &C, sql: &str, values: Vec<Value>) -> Result<u64, Refusal> {
  Ok(store::run(db, sql, values).await?)
}

/// Now, in the epoch milliseconds the service's own columns keep.
pub fn now_ms() -> i64 {
  chrono::Utc::now().timestamp_millis()
}
