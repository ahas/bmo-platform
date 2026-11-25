#![allow(
  dead_code,
  unused_imports,
  unused_variables,
  clippy::needless_lifetimes,
  clippy::let_and_return,
  clippy::unnecessary_to_owned
)]
//! Documents a session relies on, moved (docs/sessions.md, 「Document reload」): the paths named published
//! as `documents-moved` for the consumer that holds the rooms to tell the sessions concerned — the named
//! agent's in the named rooms where the reload names them — with whether the notice is due at once. Agency
//! tells no session itself. And the rulebooks more than one writer shares, read by a consumer that composes
//! a run's whole prompt itself (docs/api.md, 「Agents」).

use axum::http::{Method, StatusCode};
use axum::response::Response;
use serde::Serialize;
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::events::Event;
use crate::lines::Trees;

/// Documents moved: their paths, as the reload named them, whether the notice is due at once, and whose
/// documents they are and the rooms they concern, where the reload named them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DocumentsMoved {
  pub paths: Vec<String>,
  pub now: bool,
  pub agent: Option<String>,
  pub rooms: Vec<i64>,
}

impl Event for DocumentsMoved {
  const NAME: &'static str = "document-move.created";
}

/// The documents a reload names, published.
pub struct DocumentsReload;

impl Route for DocumentsReload {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/document-moves";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let paths: Vec<String> = body
      .get("paths")
      .and_then(Value::as_array)
      .map(|paths| paths.iter().filter_map(Value::as_str).map(str::to_owned).collect())
      .unwrap_or_default();

    if paths.is_empty() {
      return Err(Refused::new(StatusCode::BAD_REQUEST, "paths (strings) are required"));
    }

    let rooms = match body.get("roomIds").or_else(|| body.get("rooms")) {
      None | Some(Value::Null) => vec![],
      Some(Value::Array(rooms)) => rooms
        .iter()
        .map(Value::as_i64)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "rooms are chat ids (numbers)"))?,
      Some(_) => return Err(Refused::new(StatusCode::BAD_REQUEST, "rooms are chat ids (numbers)")),
    };
    let moved = DocumentsMoved {
      paths,
      now: body.get("now").and_then(Value::as_bool).unwrap_or(false),
      agent: body
        .get("agent")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|agent| !agent.is_empty())
        .map(str::to_owned),
      rooms,
    };

    call.events().publish(moved.clone());

    // A session whose prompt the moved documents are part of is replaced while it is quiet, not under its
    // next turn (docs/sessions.md, 「Session replacement」).
    if let Some(sessions) = call.sessions() {
      tokio::spawn(async move { sessions.settle_pass().await });
    }

    Ok(answer(
      StatusCode::CREATED,
      json!({
        "paths": moved.paths,
        "at": crate::now_ms(),
        "agentName": moved.agent,
        "roomIds": moved.rooms,
        "now": moved.now,
      }),
    ))
  }
}

crate::moved!(DocumentsReload);

/// The shared rulebooks named, as the deploy tree holds them.
pub struct Rulebooks;

// A rulebook's name: one plain word of letters, digits and dashes, naming a file under the rules folder.
fn plain(name: &str) -> bool {
  !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

impl Route for Rulebooks {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/rulebooks";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let names = call.query("names").unwrap_or_default();
    let names: Vec<&str> = names.split(',').map(str::trim).filter(|name| !name.is_empty()).collect();

    if names.is_empty() {
      return Err(Refused::new(StatusCode::BAD_REQUEST, "names (comma-separated) are required"));
    }

    let trees = Trees::of(call.ground());
    let mut rulebooks = Vec::with_capacity(names.len());

    for name in names {
      if !plain(name) {
        return Err(Refused::new(StatusCode::BAD_REQUEST, format!("{name} is not a rulebook's name")));
      }

      let Ok(text) = std::fs::read_to_string(trees.rulebook_path(name)) else {
        return Err(Refused::new(StatusCode::NOT_FOUND, format!("agency keeps no rulebook named {name}")));
      };

      rulebooks.push(json!({ "name": name, "text": text.trim() }));
    }

    Ok(answer(StatusCode::OK, json!({ "rulebooks": rulebooks })))
  }
}

crate::moved!(Rulebooks);
