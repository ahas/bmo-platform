//! What every handler stands on: the refusal and how it answers, the store's place, the clock's now,
//! and the text measures.

use agency_server::Service;
use agency_server::config::Config;
use agency_server::peers::PeerFault;
use agency_server::places::Ground;
use agency_server::text::{grouped, length, within};
use agency_server::{Refusal, archive_place, now_ms};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use harness_providers::Machine;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use store::place::VARIABLE as STORE_ADDRESS;

use crate::support::scratch::Scratch;

async fn answered(refusal: Refusal) -> (StatusCode, Value) {
  let response = refusal.into_response();
  let status = response.status();
  let body = response.into_body().collect().await.unwrap().to_bytes();

  (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn a_refusal_answers_its_status_with_its_code_and_its_sentence() {
  let (status, body) = answered(Refusal::absent("no-such-place", "No place is named that.")).await;

  assert_eq!(status, StatusCode::NOT_FOUND);
  assert_eq!(body, json!({ "code": "no-such-place", "error": "No place is named that." }));

  let (status, body) = answered(Refusal::failed("The disk is full.")).await;

  assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
  assert_eq!(body["code"], "failed");
}

#[test]
fn each_refusal_kind_answers_its_own_status() {
  assert_eq!(Refusal::bad("x", "y").status, StatusCode::BAD_REQUEST);
  assert_eq!(Refusal::forbidden("x", "y").status, StatusCode::FORBIDDEN);
  assert_eq!(Refusal::absent("x", "y").to_string(), "y");
}

#[test]
fn a_peer_fault_is_refused_as_the_peer_answered() {
  let refused = Refusal::from(PeerFault::Refused {
    peer: "brain",
    code: "agent-writing".into(),
    message: "The folder is being written.".into(),
  });

  assert_eq!((refused.status, refused.code.as_str()), (StatusCode::CONFLICT, "peer-refused"));
  assert!(refused.detail.contains("The folder is being written."));

  let unreachable = Refusal::from(PeerFault::Unreachable {
    peer: "node service",
    detail: "connection refused".into(),
  });

  assert_eq!(
    (unreachable.status, unreachable.code.as_str()),
    (StatusCode::BAD_GATEWAY, "peer-unreachable")
  );

  let failed = Refusal::from(PeerFault::Malformed {
    peer: "brain",
    detail: "missing field".into(),
  });

  assert_eq!((failed.status, failed.code.as_str()), (StatusCode::BAD_GATEWAY, "peer-failed"));
}

#[test]
fn a_store_fault_is_refused_as_the_stores() {
  let refusal = Refusal::from(sea_orm::DbErr::Custom("gone".into()));

  assert_eq!(
    (refusal.status, refusal.code.as_str()),
    (StatusCode::INTERNAL_SERVER_ERROR, "store-failed")
  );
  assert!(refusal.detail.starts_with("the store: "));
}

// The variable is the process's, which a test never sets, since tests share the process: where the run
// names a store the variable is read first, then the server's environment file, and the address file only
// where neither names one.
#[test]
fn the_stores_place_is_the_variables_else_the_servers_environment_file_else_the_data_roots_file() {
  let named = std::env::var("BMO_ARCHIVE_URL").ok().filter(|value| !value.trim().is_empty());
  let root = tempfile::tempdir().unwrap();

  std::fs::create_dir_all(root.path().join("env")).unwrap();

  assert_eq!(archive_place(root.path()), named.as_deref().map(|value| value.trim().to_owned()));

  std::fs::write(root.path().join("env/archive-url"), "  postgres:///kept\n").unwrap();

  let expected = named.clone().map_or("postgres:///kept".to_owned(), |value| value.trim().to_owned());

  assert_eq!(archive_place(root.path()).as_deref(), Some(expected.as_str()));

  // The server's environment file names the store the server runs on, and outranks the address file.
  std::fs::write(
    root.path().join("env/agency.env"),
    "AGENCY_CONTROL_PORT=1\nBMO_ARCHIVE_URL=\"postgres:///agency\"\n",
  )
  .unwrap();

  let expected = named.map_or("postgres:///agency".to_owned(), |value| value.trim().to_owned());

  assert_eq!(archive_place(root.path()).as_deref(), Some(expected.as_str()));
}

// A program agency starts reads its own archive from the data root's setting: the store's address names
// agency's own database, and handing it on would point every archive read in that program at the wrong one
// (docs/store.md, 「Connecting」).
#[tokio::test]
async fn a_program_a_run_starts_is_not_handed_the_stores_address() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let at = |path: &std::path::Path| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
      (STORE_ADDRESS.to_owned(), scratch.address()),
    ],
    home.path().join("data"),
  );
  let service = Service::new(scratch.db().clone(), Config::read(|_| None).unwrap());
  let ground = Ground::new(&service, &machine, None);

  // Agency's own process still knows where its store is.
  assert_eq!(ground.vars().get(STORE_ADDRESS).map(String::as_str), Some(scratch.address().as_str()));
  // No program it starts is handed it, so that program's archive reads stay its own.
  assert!(!ground.env().contains_key(STORE_ADDRESS));
  assert!(!ground.run_env("bmo-a-run").contains_key(STORE_ADDRESS));
}

#[test]
fn now_is_in_epoch_milliseconds() {
  let now = now_ms();
  let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();

  assert!((now / 1000 - i64::try_from(seconds).unwrap()).abs() <= 1);
}

#[test]
fn a_text_is_measured_in_utf16_units() {
  assert_eq!(length("agency"), 6);
  assert_eq!(length("é"), 1);
  assert_eq!(length("𝄞"), 2);
  assert_eq!(length(""), 0);
}

#[test]
fn a_text_is_cut_within_its_utf16_units_leaving_no_character_split() {
  assert_eq!(within("agency", 3), "age");
  assert_eq!(within("agency", 30), "agency");
  assert_eq!(within("a𝄞b", 2), "a");
  assert_eq!(within("a𝄞b", 3), "a𝄞");
  assert_eq!(within("", 0), "");
}

#[test]
fn a_bound_is_written_with_its_thousands_apart() {
  assert_eq!(grouped(0), "0");
  assert_eq!(grouped(999), "999");
  assert_eq!(grouped(1000), "1,000");
  assert_eq!(grouped(123_456), "123,456");
  assert_eq!(grouped(1_234_567), "1,234,567");
}
