//! The hooks: a point answers its default until a consumer registers there, and again once it is let
//! go; a consumer across the boundary registers by its address, and an address that fails answers the
//! default; a point that outlives a restart keeps its address in the store and its token in a file, and
//! stands again as the next server starts.

use agency_server::hooks::{HookPoint, Hooks, Remote, kept_token_path, register_remote, wired};
use axum::Json;
use axum::http::StatusCode;
use serde_json::{Value, json};

struct Pick;

impl HookPoint for Pick {
  type Ask = u32;
  type Answer = String;
  const NAME: &'static str = "pick";

  fn default_answer(ask: u32) -> String {
    format!("default for {ask}")
  }
}

struct Other;

impl HookPoint for Other {
  type Ask = ();
  type Answer = bool;
  const NAME: &'static str = "other";

  fn default_answer(_ask: ()) -> bool {
    false
  }
}

#[tokio::test]
async fn a_point_nobody_registered_at_answers_its_default() {
  let hooks = Hooks::default();

  assert!(!hooks.registered::<Pick>());
  assert_eq!(hooks.ask::<Pick>(3).await, "default for 3");
}

#[tokio::test]
async fn a_registered_answer_is_asked_until_it_is_let_go() {
  let hooks = Hooks::default();

  hooks.clone().register::<Pick, _, _>(|ask| async move { format!("consumer's {ask}") });

  assert!(hooks.registered::<Pick>());
  assert_eq!(hooks.ask::<Pick>(4).await, "consumer's 4");
  assert!(!hooks.ask::<Other>(()).await, "another point keeps its default");

  hooks.register::<Pick, _, _>(|ask| async move { format!("replaced {ask}") });

  assert_eq!(hooks.ask::<Pick>(5).await, "replaced 5");

  hooks.unregister::<Pick>();

  assert_eq!(hooks.ask::<Pick>(6).await, "default for 6");
}

// A consumer standing on the loopback, answering each ask with what `answer` makes of it.
async fn consumer(answer: fn(Value) -> (StatusCode, Value)) -> String {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}/hook", listener.local_addr().unwrap());
  let app = axum::Router::new().fallback(move |Json(body): Json<Value>| async move {
    let (status, body) = answer(body);

    (status, Json(body))
  });

  tokio::spawn(async move { axum::serve(listener, app).await });

  address
}

#[tokio::test]
async fn a_remote_answer_is_the_consumer_s_and_a_failing_one_the_default() {
  let hooks = Hooks::default();
  let url =
    consumer(|body| (StatusCode::OK, json!({ "answer": format!("{} {}", body["point"], body["ask"]) })))
      .await;

  register_remote::<Pick>(&hooks, Remote { url, token: None });

  assert!(hooks.registered::<Pick>());
  assert_eq!(hooks.ask::<Pick>(3).await, "\"pick\" 3");

  let url = consumer(|_| (StatusCode::INTERNAL_SERVER_ERROR, json!({}))).await;

  register_remote::<Pick>(&hooks, Remote { url, token: None });

  assert_eq!(hooks.ask::<Pick>(4).await, "default for 4");
}

#[test]
fn the_points_a_consumer_registers_at_are_each_named_once() {
  let mut names: Vec<&str> = wired().map(|point| point.name).collect();

  names.sort_unstable();

  let count = names.len();

  names.dedup();

  assert_eq!(names.len(), count);

  for wanted in ["pane-claims", "road-verbs", "room-title", "typed-line"] {
    assert!(names.contains(&wanted), "{wanted} is not a point a consumer registers at");
  }
}

#[tokio::test]
async fn a_typed_line_registration_keeps_its_address_and_stands_again_as_the_server_starts() {
  use crate::support::scratch::Scratch;

  use agency_server::control::typed_line::{Typed, TypedLine};
  use agency_server::hooks::{keep, restore};

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data_root = home.path().to_path_buf();
  let url = consumer(|_| (StatusCode::OK, json!({ "answer": "<round of the owner's chat>" }))).await;
  let registered = Remote {
    url,
    token: Some("receiver-token".to_owned()),
  };
  let hooks = Hooks::default();

  register_remote::<TypedLine>(&hooks, registered.clone());

  assert_eq!(
    hooks
      .ask::<TypedLine>(Typed {
        text: "sort it out".to_owned(),
        session_id: "s-1".to_owned()
      })
      .await,
    Some("<round of the owner's chat>".to_owned()),
  );

  keep(scratch.db(), &data_root, "typed-line", &registered).await.unwrap();
  assert_eq!(
    std::fs::read_to_string(kept_token_path(&data_root, "typed-line")).unwrap(),
    "receiver-token"
  );

  // A fresh set of hooks, as the next server starts, is registered again from what the store keeps and
  // asks under the kept token.
  let fresh = Hooks::default();

  assert_eq!(restore(&fresh, scratch.db()).await.unwrap(), 1, "the kept registration stands again");
  assert_eq!(
    fresh
      .ask::<TypedLine>(Typed {
        text: "sort it out".to_owned(),
        session_id: "s-1".to_owned()
      })
      .await,
    Some("<round of the owner's chat>".to_owned()),
  );
}
