//! What the console reads of a running server over its control routes (docs/api.md): each provider with its
//! default compaction threshold and typed quota; a supervisor's reset point alone and the rooms its turns went
//! into; a supervisor added on its one road and that road changed; a repository's typed checkouts and sampling settings, an add with no path
//! refused, a node checkout on a node that takes no jobs refused, a change refused for its sampling writing
//! none of its node checkouts; the nodes with whether they take jobs; a seeded supervisor's removal refused as
//! seeded; a slot asked for a model alone, its runs on `/slots`, an owner's release and the admission's state
//! after it; and a slot carrying role sessions in turn, its tile naming its session and run, and the forms
//! that are gone refused — on a store the server settled itself.

use serde_json::{Value, json};

use crate::support::rows::{repository, supervisor};
use crate::support::scratch::Scratch;
use crate::support::standing;

async fn call(
  port: u16,
  token: &str,
  method: reqwest::Method,
  path: &str,
  body: Option<Value>,
) -> (u16, Value) {
  let mut request = reqwest::Client::new()
    .request(method, format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token);

  if let Some(body) = body {
    request = request.json(&body);
  }

  let answer = request.send().await.unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

async fn get(port: u16, token: &str, path: &str) -> (u16, Value) {
  call(port, token, reqwest::Method::GET, path, None).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_server_answers_the_fields_the_console_reads() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  supervisor(scratch.db(), "chat-agent", &[]).await;
  supervisor(
    scratch.db(),
    "seeded-one",
    &[
      ("builtin", 1i64.into()),
      ("session_scope", "room".into()),
      ("turn_shape", "pieces".into()),
    ],
  )
  .await;
  repository(scratch.db(), "kept", "/checkouts/kept", &[]).await;
  scratch
    .run(
      r#"INSERT INTO "runs" ("agent", "chat_id", "work_unit", "depth", "started_at", "finished_at")
         VALUES ('chat-agent', 41, 'chat-agent-1', 0, 1000, 2000), ('chat-agent', 41, 'chat-agent-2', 0, 3000, NULL),
                ('chat-agent', 42, 'chat-agent-3', 0, 2500, 2600)"#,
      vec![],
    )
    .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  // Each provider names its default compaction threshold and what it has left, typed.
  let (status, providers) = get(port, token, "/providers").await;

  assert_eq!(status, 200, "{providers}");
  for provider in providers["items"].as_array().unwrap() {
    assert!(
      provider["foldRatioDefault"].as_f64().is_some_and(|ratio| ratio > 0.0 && ratio <= 1.0),
      "{provider}"
    );
    assert!(provider["quota"]["has"].is_boolean(), "{provider}");
    assert!(provider["lines"].is_array(), "{provider}");
  }

  let deepseek = providers["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|one| one["name"] == "deepseek")
    .unwrap();

  assert_eq!(deepseek["foldRatioDefault"], json!(0.5), "DeepSeek declares 0.5");

  // A provider reading its own key from a file names the file, and no other names one.
  let key_files: Vec<(String, Value)> = providers["items"]
    .as_array()
    .unwrap()
    .iter()
    .map(|one| (one["name"].as_str().unwrap().to_owned(), one["keyFile"].clone()))
    .collect();

  for (name, file) in &key_files {
    match name.as_str() {
      "openrouter" | "deepseek" => {
        assert!(file.as_str().is_some_and(|file| !file.is_empty()), "{name}: {file}")
      }
      _ => assert!(file.is_null(), "{name}: {file}"),
    }
  }

  // Each provider answers its own declaration of whether its engine is the owner's own: the registry's engines
  // and Claude Code on the owner's logins are, the outside roads are not.
  let owners_engines: Vec<(String, Value)> = providers["items"]
    .as_array()
    .unwrap()
    .iter()
    .map(|one| (one["name"].as_str().unwrap().to_owned(), one["ownersEngine"].clone()))
    .collect();

  for (name, own) in &owners_engines {
    let expected = matches!(name.as_str(), "claude-code" | "registered");

    assert_eq!(own, &json!(expected), "{name}: {own}");
  }

  // The reset point alone, and the rooms the supervisor's turns went into, the latest first.
  assert_eq!(
    get(port, token, "/supervisors/chat-agent/reset-point").await,
    (200, json!({ "supervisor": "chat-agent", "resetPoint": null }))
  );
  assert_eq!(get(port, token, "/supervisors/nobody/reset-point").await.0, 404);

  let (status, rooms) = get(port, token, "/supervisors/chat-agent/rooms").await;

  assert_eq!(status, 200, "{rooms}");
  assert_eq!(rooms["items"][0]["roomId"], 41, "{rooms}");
  assert_eq!(rooms["items"][0]["runs"], 2);
  assert_eq!(rooms["items"][0]["lastRunUnit"], "chat-agent-2");
  assert_eq!(rooms["items"][0]["lastFinishedAt"], Value::Null);
  assert_eq!(rooms["items"][1]["roomId"], 42);

  // A repository's checkouts are typed and its sampling's settings named with their bounds.
  let (status, kept) = get(port, token, "/repositories/kept").await;

  assert_eq!(status, 200, "{kept}");
  assert_eq!(kept["checkouts"][0]["kind"], "base", "{kept}");
  assert_eq!(kept["checkouts"][0]["job"], Value::Null);
  assert!(
    kept["samplingSettings"]
      .as_array()
      .unwrap()
      .iter()
      .any(|setting| setting["name"] == "temperature"),
    "{kept}"
  );
  assert!(
    !kept["samplingSettings"]
      .as_array()
      .unwrap()
      .iter()
      .any(|setting| setting["name"] == "max_tokens"),
    "the answer ceiling stays the registration's"
  );

  let (status, refused) =
    call(port, token, reqwest::Method::POST, "/repositories", Some(json!({ "name": "pathless" }))).await;

  assert_eq!((status, refused["field"].clone()), (400, json!("path")), "{refused}");

  // The node service keeps no node here, so none takes jobs and a checkout on one is refused.
  assert_eq!(get(port, token, "/nodes").await, (200, json!({ "items": [], "more": false })));

  let (status, refused) = call(
    port,
    token,
    reqwest::Method::PATCH,
    "/repositories/kept",
    Some(json!({ "nodeCheckouts": [{ "nodeName": "far", "path": null }] })),
  )
  .await;

  assert_eq!((status, refused["field"].clone()), (400, json!("nodeCheckouts")), "{refused}");
  assert_eq!(get(port, token, "/repositories/kept").await.1["checkouts"].as_array().unwrap().len(), 1);

  // A change is judged whole before anything of it is written: a sampling refused leaves the node checkouts
  // the same change would have taken out standing.
  scratch
    .run(
      r#"INSERT INTO "repository_node_checkouts" ("repository", "node", "path", "created_at")
         VALUES ('kept', 'far', '/checkouts/kept-far', 1)"#,
      vec![],
    )
    .await;

  let (status, refused) = call(
    port,
    token,
    reqwest::Method::PATCH,
    "/repositories/kept",
    Some(json!({ "nodeCheckouts": [], "sampling": { "no_such_setting": 1 } })),
  )
  .await;
  let standing = r#"SELECT COUNT(*) FROM "repository_node_checkouts" WHERE "repository" = 'kept'"#;

  assert_eq!(status, 400, "{refused}");
  assert_eq!(scratch.count(standing, vec![]).await, 1, "the node checkout stands");
  scratch
    .run(r#"DELETE FROM "repository_node_checkouts" WHERE "repository" = 'kept'"#, vec![])
    .await;

  // A supervisor is added on its one road, and its road is changed like an agent's: a changed model keeps the
  // provider and drops the old model's effort, a provider agency does not carry is refused by its field, and a
  // body naming an order is refused for the field it names.
  let (status, added) = call(
    port,
    token,
    reqwest::Method::POST,
    "/supervisors",
    Some(json!({
      "name": "pal",
      "label": "Pal",
      "repository": "kept",
      "provider": "claude-code",
      "model": "claude-opus-5",
      "effort": "high",
    })),
  )
  .await;

  assert_eq!(status, 201, "{added}");

  let (_, pal) = get(port, token, "/supervisors/pal").await;

  assert_eq!(
    (&pal["provider"], &pal["model"], &pal["effort"]),
    (&json!("claude-code"), &json!("claude-opus-5"), &json!("high")),
    "{pal}"
  );

  let (status, moved) = call(
    port,
    token,
    reqwest::Method::PATCH,
    "/supervisors/pal",
    Some(json!({ "model": "claude-sonnet-4-6" })),
  )
  .await;

  assert_eq!(status, 200, "{moved}");
  assert_eq!(
    (&moved["provider"], &moved["model"], &moved["effort"]),
    (&json!("claude-code"), &json!("claude-sonnet-4-6"), &Value::Null),
    "{moved}"
  );

  let (status, refused) = call(
    port,
    token,
    reqwest::Method::PATCH,
    "/supervisors/pal",
    Some(json!({ "provider": "nowhere", "model": "x" })),
  )
  .await;

  assert_eq!((status, refused["field"].clone()), (400, json!("provider")), "{refused}");

  let (status, refused) = call(
    port,
    token,
    reqwest::Method::POST,
    "/supervisors",
    Some(json!({
      "name": "pal-two",
      "label": "Pal",
      "repository": "kept",
      "provider": "deepseek",
      "model": "deepseek-flash",
      "order": [{ "provider": "claude-code", "model": "claude-opus-5" }],
    })),
  )
  .await;

  assert_eq!((status, refused["field"].clone()), (400, json!("order")), "{refused}");

  // A seeded supervisor's removal is refused as seeded.
  let (status, refused) = call(port, token, reqwest::Method::DELETE, "/supervisors/seeded-one", None).await;

  assert_eq!((status, refused["code"].clone()), (409, json!("seeded")), "{refused}");

  // A slot asked for a model alone goes out on the provider the role's agent of that model is on.
  let (status, roles) = get(port, token, "/roles/helper").await;

  assert_eq!(status, 200, "{roles}");

  let entry = roles["agents"][0].clone();
  let (status, asked) = call(
    port,
    token,
    reqwest::Method::POST,
    "/slot-requests",
    Some(
      json!({ "role": "helper", "requestKey": "console:one", "owner": "console", "model": entry["model"] }),
    ),
  )
  .await;

  assert_eq!(status, 201, "{asked}");
  assert_eq!(asked["provider"], entry["provider"], "{asked}");

  let (status, refused) = call(
    port,
    token,
    reqwest::Method::POST,
    "/slot-requests",
    Some(
      json!({ "role": "helper", "requestKey": "console:two", "owner": "console", "provider": "deepseek" }),
    ),
  )
  .await;

  assert_eq!(status, 400, "{refused}");

  // The owner gives back everything it holds in one ask, and an admission then says who gave it back.
  let token_of = asked["token"].as_str().unwrap().to_owned();
  let (status, released) =
    call(port, token, reqwest::Method::DELETE, "/slot-requests?owner=console", None).await;

  assert_eq!(status, 200, "{released}");
  assert_eq!(
    released["cancelled"].as_i64().unwrap() + released["released"].as_i64().unwrap(),
    1,
    "{released}"
  );
  assert_eq!(released["sessionsEnded"], json!([]));

  let (status, admission) = call(
    port,
    token,
    reqwest::Method::PATCH,
    &format!("/slot-requests/{token_of}"),
    Some(json!({})),
  )
  .await;

  assert_eq!(status, 200, "{admission}");
  assert_eq!(admission["request"], Value::Null);
  assert!(["released", "cancelled"].contains(&admission["state"].as_str().unwrap()), "{admission}");
  assert_eq!(admission["endedBy"], "owner");
  assert_eq!(call(port, token, reqwest::Method::DELETE, "/slot-requests", None).await.0, 400);

  // Every role's slots are typed, each run let in naming its token.
  let (status, slots) = get(port, token, "/slots").await;

  assert_eq!(status, 200, "{slots}");
  assert!(
    slots["items"].as_array().unwrap().iter().any(|role| role["role"] == "helper"),
    "{slots}"
  );

  sessions_on_one_slot(port, token, &entry).await;
}

// The tile `/slots` answers for the slot its token names.
async fn tile(port: u16, token: &str, slot: &str) -> Value {
  let (status, slots) = get(port, token, "/slots").await;

  assert_eq!(status, 200, "{slots}");
  slots["items"]
    .as_array()
    .unwrap()
    .iter()
    .flat_map(|role| role["runs"].as_array().unwrap().clone())
    .find(|run| run["token"] == slot)
    .unwrap_or_else(|| panic!("no tile for {slot}: {slots}"))
}

// A slot carrying role sessions in turn: each start moves its lease onto the session it opens, a session the
// lease moved off ends without giving the slot back, a tile names its session and a run only once one is
// named, and the owner's release ends the session the slot is bound to with it. A turn sent to a
// repository's worker address and a start naming no slot are refused.
async fn sessions_on_one_slot(port: u16, token: &str, entry: &Value) {
  let (status, asked) = call(
    port,
    token,
    reqwest::Method::POST,
    "/slot-requests",
    Some(json!({
      "role": "helper", "requestKey": "console:sessions", "owner": "console-sessions",
      "provider": entry["provider"], "model": entry["model"],
    })),
  )
  .await;

  assert_eq!(status, 201, "{asked}");

  let slot = asked["token"].as_str().unwrap().to_owned();
  let (status, admission) =
    call(port, token, reqwest::Method::PATCH, &format!("/slot-requests/{slot}"), Some(json!({}))).await;

  assert_eq!((status, admission["state"].as_str()), (200, Some("active")), "{admission}");

  let start = |slot: &str| json!({ "scope": "role", "slot": slot });
  let (status, first) = call(port, token, reqwest::Method::POST, "/sessions", Some(start(&slot))).await;

  assert_eq!(status, 201, "{first}");

  let first = first["id"].as_str().unwrap().to_owned();
  let read = tile(port, token, &slot).await;

  assert_eq!(read["session"], json!(first), "{read}");
  assert_eq!(read["runName"], Value::Null, "no turn runs and no run is named: {read}");
  // The run names the agent of its role it was let in on, which a screen tells the run's seat by.
  assert!(
    read["agent"]
      .as_str()
      .is_some_and(|agent| agent.starts_with(read["role"].as_str().unwrap())),
    "{read}"
  );

  // The next session starts on the slot before the first ends.
  let (status, second) = call(port, token, reqwest::Method::POST, "/sessions", Some(start(&slot))).await;

  assert_eq!(status, 201, "{second}");

  let second = second["id"].as_str().unwrap().to_owned();

  assert_ne!(first, second);
  assert_eq!(tile(port, token, &slot).await["session"], json!(second));

  let (status, _) = call(port, token, reqwest::Method::DELETE, &format!("/sessions/{first}"), None).await;

  assert!((200..300).contains(&status), "the first session ends: {status}");

  let (_, standing) = get(port, token, &format!("/slot-requests/{slot}")).await;

  assert_eq!(standing["state"], "active", "the slot moved off the first session stands: {standing}");

  // A run a binding names stands on the tile.
  let (status, bound) = call(
    port,
    token,
    reqwest::Method::PATCH,
    &format!("/slot-requests/{slot}"),
    Some(json!({ "state": "bound", "childId": "console-review" })),
  )
  .await;

  assert_eq!(status, 200, "{bound}");

  let read = tile(port, token, &slot).await;

  assert_eq!(
    (read["session"].clone(), read["runName"].clone()),
    (json!(second), json!("console-review"))
  );

  // The owner's release ends the session the slot is bound to.
  let (status, released) =
    call(port, token, reqwest::Method::DELETE, "/slot-requests?owner=console-sessions", None).await;

  assert_eq!(status, 200, "{released}");
  assert_eq!(
    (released["released"].clone(), released["sessionsEnded"].clone()),
    (json!(1), json!([second]))
  );
  // The session's ending is carried on after the answer, so it is gone once that work has run.
  while get(port, token, &format!("/sessions/{second}")).await.0 != 404 {
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  }

  // The forms that are gone.
  let (status, refused) = call(
    port,
    token,
    reqwest::Method::POST,
    "/sessions",
    Some(
      json!({ "scope": "role", "role": "helper", "provider": entry["provider"], "model": entry["model"] }),
    ),
  )
  .await;

  assert_eq!(status, 400, "a start naming no slot: {refused}");

  let (status, refused) = call(
    port,
    token,
    reqwest::Method::POST,
    "/sessions/kept@worker/turns",
    Some(json!({ "prompt": "do it" })),
  )
  .await;

  assert_eq!(status, 404, "a repository's worker address: {refused}");
}
