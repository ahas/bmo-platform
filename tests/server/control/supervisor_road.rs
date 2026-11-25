//! A supervisor's one road and its call names, asked of a running server over its control routes (docs/api.md,
//! 「supervisors」): the road route names the road and whether a turn would go out on it now, never on a Claude
//! login that was never signed in, no stand-in route stands, a body naming an order or a forced provider is
//! refused, call names are written whole and read back as one line or refused out of their bounds, and a session
//! is started at no scope the contract does not name — on a store the server settled itself.

use serde_json::{Value, json};

use crate::support::scratch::Scratch;
use crate::support::standing;

async fn get(port: u16, token: &str, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

async fn send(method: reqwest::Method, port: u16, token: &str, path: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .request(method, format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .json(&body)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_server_answers_a_supervisor_s_road_and_call_names_and_refuses_an_order_or_a_stand_in() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  crate::support::rows::supervisor(scratch.db(), "chat-agent", &[]).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  // Its road is Claude Code's, on the machine's own login, never signed in on this machine: the road is named
  // and cannot take a turn, the reason naming the sign-in command with that login's directory in front of it.
  let login = home.path().join(".claude");

  std::fs::remove_file(login.join(".credentials.json")).unwrap();

  let (status, unsigned) = get(port, token, "/supervisors/chat-agent/road").await;

  assert_eq!(status, 200, "{unsigned}");
  assert_eq!(
    (&unsigned["supervisor"], &unsigned["provider"], &unsigned["model"]),
    (&json!("chat-agent"), &json!("claude-code"), &json!("claude-opus-5"))
  );
  assert_eq!(unsigned["answers"], json!(false), "{unsigned}");
  assert!(
    unsigned["reason"].as_str().is_some_and(
      |reason| reason.contains(&format!("CLAUDE_CONFIG_DIR={} claude /login", login.display()))
    ),
    "{unsigned}"
  );

  // Signed in, its credentials file standing, the road answers.
  std::fs::write(login.join(".credentials.json"), "test-token").unwrap();

  let (_, road) = get(port, token, "/supervisors/chat-agent/road").await;

  assert_eq!((&road["answers"], &road["reason"]), (&json!(true), &Value::Null), "{road}");

  // Claude Code switched off, the road cannot take a turn: the reason says why, with no other road offered in its
  // place though every other provider stays on.
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('provider_switch:claude-code', 'off')
         ON CONFLICT ("key") DO UPDATE SET "value" = EXCLUDED."value""#,
      vec![],
    )
    .await;

  let (_, closed) = get(port, token, "/supervisors/chat-agent/road").await;

  assert_eq!(
    (&closed["provider"], &closed["answers"]),
    (&json!("claude-code"), &json!(false)),
    "{closed}"
  );
  assert!(closed["reason"].as_str().is_some_and(|reason| !reason.is_empty()), "{closed}");
  assert_eq!(get(port, token, "/supervisors/nobody/road").await.0, 404);

  // The supervisor's read names the same road and nothing of an order.
  let (_, held) = get(port, token, "/supervisors/chat-agent").await;

  assert_eq!((&held["provider"], &held["model"]), (&json!("claude-code"), &json!("claude-opus-5")));

  for gone in ["order", "quotaShare", "active", "waitingFor"] {
    assert!(held.get(gone).is_none(), "{gone}: {held}");
  }

  // No stand-in route stands, and a change naming an order or a forced provider is refused by its field.
  assert_eq!(get(port, token, "/stand-in?agent=chat-agent").await.0, 404);

  for (field, value) in [
    ("order", json!([{ "provider": "codex", "model": "gpt-5.6-sol" }])),
    ("quotaShare", json!(0.5)),
    ("active", json!({ "provider": "codex", "model": "gpt-5.6-sol" })),
  ] {
    let (status, refused) =
      send(reqwest::Method::PATCH, port, token, "/supervisors/chat-agent", json!({ field: value })).await;

    assert_eq!((status, &refused["field"]), (400, &json!(field)), "{refused}");
  }

  // Its road names an advisor its model is paired with, read back with the road; one ranking below its model is
  // refused by its own field and leaves the advisor standing (docs/provider-order.md, 「The advisor」).
  let (status, advised) = send(
    reqwest::Method::PATCH,
    port,
    token,
    "/supervisors/chat-agent",
    json!({ "advisor": "fable" }),
  )
  .await;

  assert_eq!((status, &advised["advisor"]), (200, &json!("fable")), "{advised}");

  let (status, refused) = send(
    reqwest::Method::PATCH,
    port,
    token,
    "/supervisors/chat-agent",
    json!({ "advisor": "claude-sonnet-5" }),
  )
  .await;

  assert_eq!((status, &refused["field"]), (400, &json!("advisor")), "{refused}");
  assert_eq!(get(port, token, "/supervisors/chat-agent").await.1["advisor"], json!("fable"));

  // Call names are written as a whole list and read back on both reads as the one line the receiver wakes it on;
  // a list out of the policy's bounds is refused by its field and leaves the names standing.
  let (status, named) = send(
    reqwest::Method::PATCH,
    port,
    token,
    "/supervisors/chat-agent",
    json!({ "aliases": ["Dear", " dear ", "\u{B514}\u{C5B4}"] }),
  )
  .await;
  let line = json!("Dear, dear, \u{B514}\u{C5B4}");

  assert_eq!((status, &named["aliases"]), (200, &line), "{named}");
  assert_eq!(get(port, token, "/supervisors/chat-agent").await.1["aliases"], line);
  let (_, every) = get(port, token, "/supervisors").await;
  let listed = every["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|one| one["name"] == "chat-agent")
    .unwrap();

  assert_eq!(listed["aliases"], line);

  for names in [
    json!(["a,b"]),
    json!([" "]),
    json!(["x".repeat(41)]),
    json!(vec!["n"; 11]),
    json!("Dear"),
  ] {
    let (status, refused) = send(
      reqwest::Method::PATCH,
      port,
      token,
      "/supervisors/chat-agent",
      json!({ "aliases": names }),
    )
    .await;

    assert_eq!((status, &refused["field"]), (400, &json!("aliases")), "{names}: {refused}");
  }

  assert_eq!(get(port, token, "/supervisors/chat-agent").await.1["aliases"], line);

  let (_, cleared) =
    send(reqwest::Method::PATCH, port, token, "/supervisors/chat-agent", json!({ "aliases": [] })).await;

  assert_eq!(cleared["aliases"], Value::Null, "{cleared}");

  let (status, refused) = send(
    reqwest::Method::POST,
    port,
    token,
    "/sessions",
    json!({ "agentName": "digest", "scope": "editor" }),
  )
  .await;

  assert_eq!(status, 400, "{refused}");
  assert!(refused["error"].as_str().unwrap().contains("not editor"), "{refused}");
  assert_ne!(get(port, token, "/sessions/digest@editor").await.0, 200);
}
