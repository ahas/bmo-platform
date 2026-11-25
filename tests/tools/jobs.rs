//! The job tools on a node (docs/tools.md, 「The job tools on a node」): each entry asks its route of agency's server
//! for the supervisor the session's record is held by, whatever its arguments say, a record held by nobody refused
//! before agency is asked, and a refusal answered with agency's status, code and sentence. The manifest is held to
//! the tool, and each input schema to the type the supervisor's own server reads.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::extract::Path as Route;
use axum::http::StatusCode;
use axum::{Json, Router};
use bmo_agency_client::{FileItem, ReviewJob, StopJob};
use jobs_tool::{CAPABILITY, ENTRIES, NAMESPACE, call_with, full_name};
use schemars::JsonSchema;
use serde_json::{Value, json};
use tool_call::{Control, Session};

type Asked = Arc<Mutex<Vec<(String, Value)>>>;

async fn agency(asked: Asked) -> Control {
  let app = Router::new().route(
    "/{route}",
    axum::routing::post(move |Route(route): Route<String>, Json(body): Json<Value>| {
      let asked = asked.clone();

      async move {
        asked.lock().unwrap().push((route.clone(), body.clone()));

        match route.as_str() {
          "item-filings" if body["title"] == "refused" => (
            StatusCode::CONFLICT,
            Json(json!({ "code": "agent-unnamed", "error": "name the agent the item runs on" })),
          ),
          "item-filings" => (
            StatusCode::CREATED,
            Json(json!({ "id": 7, "state": "draft", "scope": "-9", "labels": {} })),
          ),
          _ => (StatusCode::NOT_FOUND, Json(json!({ "code": "not-found", "error": "no such job" }))),
        }
      }
    }),
  );
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  Control { port, token: None }
}

fn text(answer: &tool_call::Answer) -> String {
  serde_json::to_string(answer).unwrap()
}

// "each call asks agency's server for the supervisor the session's record is held by; a record held by nobody is
// refused before agency is asked; a refusal is answered with agency's status, code and sentence"
#[tokio::test]
async fn each_entry_asks_its_route_for_the_supervisor_the_record_is_held_by() {
  let asked = Asked::default();
  let control = agency(asked.clone()).await;
  let held = Session {
    holder_name: "member-supervisor".to_owned(),
    owner_account: Some("60".to_owned()),
    ..Session::default()
  };
  let item = json!({ "title": "Check the README", "brief": "Fix the typo.", "queueName": "todo",
    "repository": "bmo-agency" });

  let nobody = call_with(&control, &Session::default(), &full_name("file_item"), item.clone()).await;

  assert!(text(&nobody).contains("held by no supervisor"), "{}", text(&nobody));
  assert!(asked.lock().unwrap().is_empty(), "agency was not asked");

  let filed = text(&call_with(&control, &held, &full_name("file_item"), item).await);

  assert!(filed.contains("\\\"id\\\":7") && filed.contains("draft"), "{filed}");

  let refused = json!({ "title": "refused", "brief": "b", "queueName": "todo" });
  let refused = text(&call_with(&control, &held, &full_name("file_item"), refused).await);

  assert!(refused.contains("\"isError\":true") && refused.contains("agent-unnamed"), "{refused}");
  assert!(refused.contains("409"), "{refused}");

  let stopped = json!({ "jobId": 3, "reason": "the owner said stop" });
  let stopped = text(&call_with(&control, &held, &full_name("stop_job"), stopped).await);

  assert!(stopped.contains("\"isError\":true") && stopped.contains("no such job"), "{stopped}");

  let misfit = text(&call_with(&control, &held, &full_name("review_job"), json!({ "jobId": "x" })).await);

  assert!(misfit.contains("not the shape this tool takes"), "{misfit}");

  let other = text(&call_with(&control, &held, "bmo/file_item", json!({})).await);

  assert!(other.contains("holds no entry bmo/file_item"), "{other}");

  let asked = asked.lock().unwrap();

  assert_eq!(
    asked.iter().map(|(route, _)| route.as_str()).collect::<Vec<_>>(),
    ["item-filings", "item-filings", "job-stops"]
  );
  assert!(asked.iter().all(|(_, body)| body["agentName"] == "member-supervisor"), "{asked:?}");
  assert_eq!(asked[0].1["repository"], "bmo-agency");
}

// The property names a schema declares, and those it requires.
fn shape(schema: &Value) -> (BTreeSet<String>, BTreeSet<String>) {
  let names = |value: &Value| -> BTreeSet<String> {
    value.as_object().map(|object| object.keys().cloned().collect()).unwrap_or_default()
  };
  let required = schema["required"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|name| name.as_str().map(str::to_owned))
    .collect();

  (names(&schema["properties"]), required)
}

fn generated<T: JsonSchema>() -> Value {
  schemars::SchemaGenerator::default().into_root_schema_for::<T>().to_value()
}

#[test]
fn the_manifest_declares_each_entry_as_the_tool_answers_it_and_each_schema_is_its_type_s() {
  let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tools").join("jobs");
  let manifest: toml::Value =
    toml::from_str(&std::fs::read_to_string(folder.join("tool.toml")).unwrap()).unwrap();
  let entries = manifest["entry"].as_array().unwrap();

  assert_eq!(entries.len(), ENTRIES.len());

  for (declared, entry) in entries.iter().zip(ENTRIES) {
    assert_eq!(declared["namespace"].as_str(), Some(NAMESPACE));
    assert_eq!(declared["name"].as_str(), Some(entry.name));
    assert_eq!(declared["title"].as_str(), Some(entry.title));
    assert_eq!(declared["description"].as_str(), Some(entry.description));
    assert_eq!(declared["input"].as_str(), Some(entry.input));
    assert_eq!(
      declared["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|one| one.as_str().unwrap())
        .collect::<Vec<_>>(),
      [CAPABILITY]
    );
  }

  let file = |name: &str| -> Value {
    serde_json::from_str(&std::fs::read_to_string(folder.join(name)).unwrap()).unwrap()
  };

  for (name, schema) in [
    ("file_item.json", generated::<FileItem>()),
    ("stop_job.json", generated::<StopJob>()),
    ("review_job.json", generated::<ReviewJob>()),
  ] {
    let (properties, required) = shape(&file(name));
    let (typed, typed_required) = shape(&schema);

    assert_eq!(properties, typed, "{name} declares the properties its type reads");
    assert!(required.is_subset(&typed_required), "{name} requires only what its type requires");
    assert_eq!(file(name)["additionalProperties"], json!(false), "{name}");
  }
}
