//! The app put (docs/tools.md, 「The app put」): the person is the record's own, a record naming none refused before
//! agency is asked; each action asks its route of agency's server for that account and app, and its answer says the
//! version in place and the entries reached from the next turn; a refusal is answered as a failure with agency's
//! words. Its manifest is held to the tool.

use std::path::Path;
use std::sync::{Arc, Mutex};

use app_put_tool::{PutInput, put_with};
use axum::extract::Path as Route;
use axum::http::{Method, StatusCode};
use axum::{Json, Router};
use serde_json::{Value, json};
use tool_call::{Control, Session};

type Asked = Arc<Mutex<Vec<(String, String, Value)>>>;

async fn agency(asked: Asked) -> Control {
  let app = Router::new().route(
    "/personal-apps/{account}/{*rest}",
    axum::routing::any(
      move |method: Method, Route((account, rest)): Route<(String, String)>, body: Option<Json<Value>>| {
        let asked = asked.clone();

        async move {
          let body = body.map(|Json(body)| body).unwrap_or(Value::Null);

          asked.lock().unwrap().push((method.to_string(), format!("{account}/{rest}"), body));

          if rest.starts_with("broken") {
            return (
              StatusCode::UNPROCESSABLE_ENTITY,
              Json(json!({ "error": "broken: its build failed: no main.py", "code": "app-refused" })),
            );
          }

          let version = if method == Method::DELETE { Value::Null } else { json!("200-def") };

          (
            StatusCode::OK,
            Json(json!({
              "name": rest.split('/').next(),
              "version": version,
              "entries": [{ "name": "my/weekly", "description": "Sums up the week." }],
              "said": ["put 8's weekly 200-def"],
            })),
          )
        }
      },
    ),
  );
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  Control { port, token: None }
}

fn input(name: &str, action: Option<&str>, version: Option<&str>) -> PutInput {
  PutInput {
    name: name.to_owned(),
    action: action.map(str::to_owned),
    version: version.map(str::to_owned),
  }
}

fn text(answer: &tool_call::Answer) -> String {
  serde_json::to_string(answer).unwrap()
}

// "the person is the record's own: a record naming none is refused before agency is asked; each action asks its
// route for that account and app, and a refusal is answered with agency's words"
#[tokio::test]
async fn each_action_asks_its_route_for_the_record_s_own_account() {
  let asked = Asked::default();
  let control = agency(asked.clone()).await;
  let owned = Session {
    owner_account: Some("8".to_owned()),
    ..Session::default()
  };

  let nobody = put_with(&control, &Session::default(), input("weekly", None, None)).await;

  assert!(text(&nobody).contains("acts for no person"), "{}", text(&nobody));
  assert!(asked.lock().unwrap().is_empty(), "agency was not asked");

  let put = text(&put_with(&control, &owned, input("weekly", None, None)).await);

  assert!(put.contains("put as version 200-def"), "{put}");
  assert!(put.contains("From the next turn its sessions reach my/weekly"), "{put}");

  let back = text(&put_with(&control, &owned, input("weekly", Some("back"), Some("100-abc"))).await);

  assert!(back.contains("back on version 200-def"), "{back}");

  let removed = text(&put_with(&control, &owned, input("weekly", Some("remove"), None)).await);

  assert!(removed.contains("is removed"), "{removed}");

  let broken = text(&put_with(&control, &owned, input("broken", None, None)).await);

  assert!(broken.contains("\"isError\":true"), "{broken}");
  assert!(broken.contains("was not put: broken: its build failed"), "{broken}");

  let odd = text(&put_with(&control, &owned, input("weekly", Some("share"), None)).await);

  assert!(odd.contains("share is no action"), "{odd}");

  let asked = asked.lock().unwrap();

  assert_eq!(
    asked.iter().map(|(method, path, _)| format!("{method} {path}")).collect::<Vec<_>>(),
    ["POST 8/weekly/put", "POST 8/weekly/back", "DELETE 8/weekly", "POST 8/broken/put"]
  );
  assert_eq!(asked[1].2, json!({ "version": "100-abc" }));
}

#[test]
fn the_app_put_entry_is_declared_as_its_tool_answers_it() {
  let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tools").join("app_put");
  let manifest: toml::Value =
    toml::from_str(&std::fs::read_to_string(folder.join("tool.toml")).unwrap()).unwrap();
  let entry = &manifest["entry"].as_array().unwrap()[0];

  assert_eq!(entry["namespace"].as_str(), Some(app_put_tool::NAMESPACE));
  assert_eq!(entry["name"].as_str(), Some(app_put_tool::NAME));
  assert_eq!(entry["title"].as_str(), Some(app_put_tool::TITLE));
  assert_eq!(entry["description"].as_str(), Some(app_put_tool::DESCRIPTION));

  let schema: Value =
    serde_json::from_str(&std::fs::read_to_string(folder.join("app_put.json")).unwrap()).unwrap();

  assert_eq!(schema["required"], json!(["name"]));
  assert_eq!(schema["properties"]["action"]["enum"], json!(app_put_tool::ACTIONS));
  assert_eq!(schema["additionalProperties"], false);
}
