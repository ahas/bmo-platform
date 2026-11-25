//! The base persona a chat launch ends its system prompt with: the base layer the agent's brain renders, asked
//! of a stand-in brain over HTTP, its placeholders filled from the values file, and the persona the turn
//! handed in wherever the brain's cannot be used, with one sentence saying why.

use std::path::Path;
use std::sync::{Arc, Mutex};

use agency_server::config::Config;
use agency_server::lines::persona::{BasePersona, base_persona, filled, values_of};
use agency_server::peers::Peers;
use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tokio::net::TcpListener;

type Asked = Arc<Mutex<Vec<String>>>;

const VALUES: &str = r#"
[owner]
name = "Owner Name"
id = "1000"

[rooms.lobby]
tone = "warm"
"#;

const BASE: &str = "You speak with {{owner.name}} (id {{owner.id}}) as a friend.";

fn brains(agent: &str, id: Option<i64>) -> Response {
  let items: Vec<_> = id
    .map(|id| json!({ "id": id, "accountKey": "1", "agentKey": agent, "name": agent, "createdAt": 1 }))
    .into_iter()
    .collect();

  axum::Json(json!({ "items": items, "more": false })).into_response()
}

// The stand-in brain: `held` keeps brain 5 with a base layer of placeholders, `plain` brain 6 with one of
// none, `bare` brain 7 with no base layer, and `nobody` keeps no brain.
async fn answer(State(asked): State<Asked>, uri: Uri) -> Response {
  let path = uri.path().to_owned();
  let query = uri.query().unwrap_or_default().to_owned();

  asked.lock().unwrap().push(format!("{path}?{query}"));

  match (path.as_str(), query.as_str()) {
    ("/brains", "kind=agent&agentKey=held&limit=1") => brains("held", Some(5)),
    ("/brains", "kind=agent&agentKey=plain&limit=1") => brains("plain", Some(6)),
    ("/brains", "kind=agent&agentKey=bare&limit=1") => brains("bare", Some(7)),
    ("/brains", _) => brains("nobody", None),
    ("/brains/5/persona", "layers=persona") => ([("content-type", "text/plain")], BASE).into_response(),
    ("/brains/6/persona", "layers=persona") => {
      ([("content-type", "text/plain")], "You speak plainly.").into_response()
    }
    _ => (
      StatusCode::NOT_FOUND,
      axum::Json(json!({ "code": "not-found", "error": "No such layer." })),
    )
      .into_response(),
  }
}

async fn stand_in() -> (Peers, Asked) {
  let asked = Asked::default();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}/", listener.local_addr().unwrap());
  let routes = axum::Router::new().fallback(answer).with_state(asked.clone());

  tokio::spawn(async move { axum::serve(listener, routes).await.unwrap() });

  let config = Config::read(|name| (name == "AGENCY_BRAIN_URL").then(|| address.clone())).unwrap();

  (Peers::of(&config.peers), asked)
}

fn values_file(dir: &Path, text: &str) -> std::path::PathBuf {
  let path = dir.join("persona-values.toml");

  std::fs::write(&path, text).unwrap();

  path
}

#[tokio::test]
async fn the_brain_s_base_layer_is_used_with_its_placeholders_filled() {
  let (peers, asked) = stand_in().await;
  let dir = tempfile::tempdir().unwrap();
  let values = values_file(dir.path(), VALUES);

  let persona = base_persona(peers.brain.as_ref(), "held", Some(&values), Some("handed in")).await;

  assert_eq!(
    persona,
    BasePersona {
      text: Some("You speak with Owner Name (id 1000) as a friend.".into()),
      unused: None,
    }
  );
  assert_eq!(
    *asked.lock().unwrap(),
    ["/brains?kind=agent&agentKey=held&limit=1", "/brains/5/persona?layers=persona"],
    "the base layer alone is asked for"
  );

  let plain = base_persona(peers.brain.as_ref(), "plain", None, None).await;

  assert_eq!(
    plain.text.as_deref(),
    Some("You speak plainly."),
    "a layer of no placeholder needs no values"
  );
  assert_eq!(plain.unused, None);
}

#[tokio::test]
async fn the_handed_persona_stands_where_the_brain_gives_none() {
  let (peers, _) = stand_in().await;

  for agent in ["nobody", "bare"] {
    let persona = base_persona(peers.brain.as_ref(), agent, None, Some("  handed in  ")).await;

    assert_eq!(persona.text.as_deref(), Some("handed in"), "{agent}");

    let why = persona.unused.unwrap();

    assert!(why.starts_with("The base persona is the one the turn handed in, since "), "{why}");
    assert!(why.ends_with('.') && why.matches(". ").count() == 0, "one sentence: {why}");
  }

  let none = base_persona(peers.brain.as_ref(), "nobody", None, Some("   ")).await;

  assert_eq!(none.text, None);
  assert!(none.unused.unwrap().contains("the agent has no brain and the turn handed in none"));

  let undeployed =
    base_persona(Peers::of(&Default::default()).brain.as_ref(), "held", None, Some("handed")).await;

  assert_eq!(undeployed.text.as_deref(), Some("handed"));
  assert!(undeployed.unused.unwrap().contains("no brain is deployed"));
}

#[tokio::test]
async fn the_handed_persona_stands_where_a_brain_does_not_answer() {
  let listener = TcpListener::bind("[::1]:0").await.unwrap();
  let address = format!("http://{}/", listener.local_addr().unwrap());

  drop(listener);

  let config = Config::read(|name| (name == "AGENCY_BRAIN_URL").then(|| address.clone())).unwrap();
  let persona = base_persona(Peers::of(&config.peers).brain.as_ref(), "held", None, Some("handed")).await;

  assert_eq!(persona.text.as_deref(), Some("handed"));
  assert!(persona.unused.unwrap().contains("could not be reached"));
}

#[tokio::test]
async fn the_handed_persona_stands_where_a_placeholder_stays_unfilled() {
  let (peers, _) = stand_in().await;
  let dir = tempfile::tempdir().unwrap();

  let unnamed = base_persona(peers.brain.as_ref(), "held", None, Some("handed")).await;

  assert_eq!(unnamed.text.as_deref(), Some("handed"));
  assert!(unnamed.unused.unwrap().contains("AGENCY_PERSONA_VALUES names no values file"));

  let short = values_file(dir.path(), "[owner]\nname = \"Owner Name\"\n");
  let missing = base_persona(peers.brain.as_ref(), "held", Some(&short), Some("handed")).await;

  assert_eq!(missing.text.as_deref(), Some("handed"));
  assert!(missing.unused.unwrap().contains("holds no value for owner.id"));

  let gone = base_persona(peers.brain.as_ref(), "held", Some(&dir.path().join("absent.toml")), None).await;

  assert_eq!(gone.text, None);
  assert!(gone.unused.unwrap().contains("could not be read"));
}

#[test]
fn every_placeholder_is_filled_or_the_text_is_refused() {
  let dir = tempfile::tempdir().unwrap();
  let values = values_of(Some(&values_file(dir.path(), VALUES)));

  assert_eq!(
    filled(
      "{{owner.name}} and {{owner.name}}; {{ owner.name }} and {{Owner}} are no placeholders",
      &values
    ),
    Ok("Owner Name and Owner Name; {{ owner.name }} and {{Owner}} are no placeholders".into())
  );
  assert_eq!(
    filled("{{owner.handle}} {{room-slug.tone}}", &values),
    Err("the values file holds no value for owner.handle, room-slug.tone".into())
  );
  assert_eq!(
    filled("{{rooms}}", &values),
    Err("the values file holds no value for rooms".into()),
    "a table is no value"
  );
  assert_eq!(filled("no placeholder", &Err("unread".into())), Ok("no placeholder".into()));
  assert!(
    values_of(Some(&values_file(dir.path(), "owner = [")))
      .unwrap_err()
      .contains("is not TOML")
  );
}
