//! What stops a change a route makes: a refusal a person reads about one field, answered 400 with the
//! field beside the error, or a failure.

use axum::http::StatusCode;
use axum::response::Response;
use serde_json::json;

use super::{Refused, answer};

pub enum Fault {
  Field(String, String),
  Failed(Refused),
}

impl From<Refused> for Fault {
  fn from(failed: Refused) -> Self {
    Self::Failed(failed)
  }
}

impl From<crate::Refusal> for Fault {
  fn from(refusal: crate::Refusal) -> Self {
    Self::Failed(refusal.into())
  }
}

impl From<store::Fault> for Fault {
  fn from(fault: store::Fault) -> Self {
    Self::Failed(fault.into())
  }
}

impl From<sea_orm::DbErr> for Fault {
  fn from(error: sea_orm::DbErr) -> Self {
    Self::Failed(error.into())
  }
}

impl From<harness_machine::Fault> for Fault {
  fn from(fault: harness_machine::Fault) -> Self {
    match fault {
      harness_machine::Fault::Refused(refusal) => Self::Field(refusal.field, refusal.error),
      harness_machine::Fault::Failed(error) => Self::Failed(Refused::failed(error)),
    }
  }
}

pub fn refused<T>(field: &str, error: impl Into<String>) -> Result<T, Fault> {
  Err(Fault::Field(field.to_owned(), error.into()))
}

impl Fault {
  /// The refusal answered 400 with its field, or the failure handed on.
  pub fn answer(self) -> Result<Response, Refused> {
    match self {
      Self::Field(field, error) => {
        Ok(answer(StatusCode::BAD_REQUEST, json!({ "error": error, "field": field })))
      }
      Self::Failed(failed) => Err(failed),
    }
  }
}
