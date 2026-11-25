//! The skill put (docs/tools.md, 「The skill put」): the person is the record's own, a record naming none refused
//! before agency is asked; a put asks agency's `PUT /personal-skills/{account}/{name}` with the files whole and a
//! removal its `DELETE`; a refusal is answered as a failure with agency's words. Its manifest is held to the tool.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::extract::Path as Route;
use axum::http::{Method, StatusCode};
use axum::{Json, Router};
use serde_json::{Value, json};
use skill_put_tool::{PutInput, put_with};
use tool_call::{Control, Session};

type Asked = Arc<Mutex<Vec<(String, String, Value)>>>;

async fn agency(asked: Asked) -> Control {
  let app = Router::new().route(
    "/personal-skills/{account}/{name}",
    axum::routing::any(
      move |method: Method, Route((account, name)): Route<(String, String)>, body: Option<Json<Value>>| {
        let asked = asked.clone();

        async move {
          asked.lock().unwrap().push((
            method.to_string(),
            format!("{account}/{name}"),
            body.map(|Json(body)| body).unwrap_or(Value::Null),
          ));

          if name == "taken" {
            return (
              StatusCode::CONFLICT,
              Json(json!({ "error": "the skill taken is an installed copy", "code": "name-taken" })),
            );
          }

          if method == Method::DELETE {
            (StatusCode::OK, Json(json!({ "name": name })))
          } else {
            (StatusCode::OK, Json(json!({ "name": name, "version": "300-aaa" })))
          }
        }
      },
    ),
  );
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  Control { port, token: None }
}

fn input(name: &str, action: Option<&str>, files: &[(&str, &str)]) -> PutInput {
  PutInput {
    name: name.to_owned(),
    action: action.map(str::to_owned),
    files: files.iter().map(|(path, text)| ((*path).to_owned(), (*text).to_owned())).collect(),
  }
}

fn text(answer: &tool_call::Answer) -> String {
  serde_json::to_string(answer).unwrap()
}

#[tokio::test]
async fn a_put_and_a_removal_ask_agency_for_the_record_s_own_account() {
  let asked = Asked::default();
  let control = agency(asked.clone()).await;
  let owned = Session {
    owner_account: Some("8".to_owned()),
    ..Session::default()
  };
  let skill = [("SKILL.md", "---\nname: tidy\ndescription: Tidies notes.\n---\nTidy them.")];

  let nobody = text(&put_with(&control, &Session::default(), input("tidy", None, &skill)).await);

  assert!(nobody.contains("acts for no person"), "{nobody}");

  let empty = text(&put_with(&control, &owned, input("tidy", None, &[])).await);

  assert!(empty.contains("names the skill's files"), "{empty}");
  assert!(asked.lock().unwrap().is_empty(), "agency was not asked");

  let saved = text(&put_with(&control, &owned, input("tidy", None, &skill)).await);

  assert!(saved.contains("saved as version 300-aaa"), "{saved}");

  let removed = text(&put_with(&control, &owned, input("tidy", Some("remove"), &[])).await);

  assert!(removed.contains("is removed"), "{removed}");

  let taken = text(&put_with(&control, &owned, input("taken", None, &skill)).await);

  assert!(taken.contains("was not saved: the skill taken is an installed copy"), "{taken}");

  let asked = asked.lock().unwrap();

  assert_eq!(
    asked.iter().map(|(method, path, _)| format!("{method} {path}")).collect::<Vec<_>>(),
    ["PUT 8/tidy", "DELETE 8/tidy", "PUT 8/taken"]
  );
  assert_eq!(
    asked[0].2["files"],
    json!(skill.iter().map(|(path, text)| (*path, *text)).collect::<BTreeMap<_, _>>())
  );
}

#[test]
fn the_skill_put_entry_is_declared_as_its_tool_answers_it() {
  let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tools").join("skill_put");
  let manifest: toml::Value =
    toml::from_str(&std::fs::read_to_string(folder.join("tool.toml")).unwrap()).unwrap();
  let entry = &manifest["entry"].as_array().unwrap()[0];

  assert_eq!(entry["namespace"].as_str(), Some(skill_put_tool::NAMESPACE));
  assert_eq!(entry["name"].as_str(), Some(skill_put_tool::NAME));
  assert_eq!(entry["title"].as_str(), Some(skill_put_tool::TITLE));
  assert_eq!(entry["description"].as_str(), Some(skill_put_tool::DESCRIPTION));

  let schema: Value =
    serde_json::from_str(&std::fs::read_to_string(folder.join("skill_put.json")).unwrap()).unwrap();

  assert_eq!(schema["required"], json!(["name"]));
  assert_eq!(schema["properties"]["action"]["enum"], json!(skill_put_tool::ACTIONS));
  assert_eq!(schema["additionalProperties"], false);
}
