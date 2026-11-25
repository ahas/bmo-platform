//! The Claude logins as the job unit and the console ask after them, of a running server: whether a user
//! holds a login that is on, each login's own two headline windows as the store keeps them, the record homes
//! each once, and the rest a done job's usage calls for begun on its login where a window reads full and
//! nothing begun where every window has any left; and the fallback's view answering the active slot's balance
//! and each account's windows and balance.

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

async fn patch(port: u16, token: &str, path: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .patch(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .json(&body)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_server_answers_a_user_s_login_the_record_homes_and_a_job_s_usage_rest() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  scratch
    .run(
      r#"INSERT INTO "claude_accounts"
           ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at", "user_id")
         VALUES ('login-a', 'one', 1, 1, 'directory', '/logins/one', 0, 7),
                ('login-b', 'two', 2, 0, 'directory', '/logins/two', 0, 8),
                ('login-c', 'three', 3, 1, 'directory', '/logins/one', 0, NULL)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "usage_windows" ("mode", "account", "name", "utilization", "resets_at", "seen_at")
         VALUES ('claude-code', 'login-a', 'five_hour', 0.25, 1800003600, 1800000000),
                ('claude-code', 'login-a', 'seven_day', 0.5, NULL, 1800000000),
                ('claude-code', 'login-c', 'five_hour', 0.75, NULL, 1800000000)"#,
      vec![],
    )
    .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  // A user holds a login only where one of their own is on.
  let held = |accounts: &serde_json::Value, id: &str| {
    accounts["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|account| account["id"] == id)
      .map(|account| account["held"].as_bool().unwrap())
      .unwrap_or(false)
  };
  let (status, seven) = get(port, token, "/accounts?userName=7").await;
  assert_eq!(status, 200);
  assert!(held(&seven, "login-a"));
  let (_, eight) = get(port, token, "/accounts?userName=8").await;
  assert!(!held(&eight, "login-b"));
  let (_, nine) = get(port, token, "/accounts?userName=9").await;
  assert!(
    nine["items"]
      .as_array()
      .unwrap()
      .iter()
      .all(|account| !account["held"].as_bool().unwrap())
  );
  let (_, someone) = get(port, token, "/accounts?userName=someone").await;
  assert!(someone["items"].as_array().unwrap().is_empty());

  // Every login, each with the home its records are kept under.
  let (status, accounts) = get(port, token, "/accounts").await;
  let homes: Vec<&str> = accounts["items"]
    .as_array()
    .unwrap()
    .iter()
    .map(|one| one["home"].as_str().unwrap())
    .collect();

  assert_eq!(status, 200);
  assert_eq!(homes.len(), 3, "{homes:?}");
  assert_eq!(&homes[1..], ["/logins/two", "/logins/one"]);

  // Each login answers its own two headline windows, and null where it holds no such window.
  let of = |id: &str| {
    accounts["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|account| account["id"] == id)
      .unwrap()
      .clone()
  };
  let one = of("login-a");

  assert_eq!(
    one["session"],
    json!({ "name": "five_hour", "account": "login-a", "mode": "claude-code", "utilization": 0.25,
            "resetsAt": 1800003600, "seenAt": 1800000000, "durationMinutes": null, "limitName": null })
  );
  assert_eq!(one["week"]["name"], "seven_day");
  assert_eq!(one["week"]["utilization"], 0.5);
  assert_eq!(of("login-b")["session"], Value::Null);
  assert_eq!(of("login-c")["session"]["utilization"], 0.75);
  assert_eq!(of("login-c")["week"], Value::Null);

  // A full session window rests its login until it resets; windows with half a percent left rest nothing.
  let until = 4_102_444_800_i64;
  let full = json!({ "usage": { "utilization": 1.0, "resetsAt": until, "weeklyUtilization": 0.2, "weeklyResetsAt": null } });
  let light = json!({ "usage": { "utilization": 0.995, "resetsAt": until, "weeklyUtilization": 0.995, "weeklyResetsAt": until } });

  assert_eq!(patch(port, token, "/accounts/login-a", full).await.0, 200);
  assert_eq!(patch(port, token, "/accounts/login-c", light).await.0, 200);
  assert_eq!(
    scratch
      .count(
        r#"SELECT count(*) FROM "settings" WHERE "key" = 'rest_until:login-a' AND "value"::numeric = $1"#,
        vec![until.into()],
      )
      .await,
    1
  );
  assert_eq!(
    scratch
      .count(r#"SELECT count(*) FROM "settings" WHERE "key" = 'rest_until:login-c'"#, vec![])
      .await,
    0
  );
  assert_eq!(patch(port, token, "/accounts/login-a", json!({ "usage": {} })).await.0, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fallback_s_view_answers_the_active_slot_s_balance_and_each_account_s_windows_and_balance() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  scratch
    .run(
      r#"INSERT INTO "claude_accounts"
           ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at",
            "endpoint_url", "key_file")
         VALUES ('login-a', 'one', 1, 1, 'directory', '/logins/one', 0, NULL, NULL),
                ('paid', 'two', 2, 1, 'endpoint', '/logins/paid', 0, 'https://endpoint.invalid', '/keys/paid')"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "usage_windows" ("mode", "account", "name", "utilization", "resets_at", "seen_at")
         VALUES ('claude-code', 'login-a', 'five_hour', 0.25, NULL, 1800000000),
                ('claude-code', 'login-a', 'seven_day', 0.5, NULL, 1800000000)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "usage_balances"
           ("mode", "account", "amount", "currency", "seen_at", "requests_used", "requests_limit", "requests_left",
            "requests_scope")
         VALUES ('deepseek', '', 12.5, 'CNY', 1800000000, 12, 50, 38, 'some-requests'),
                ('deepseek', 'paid', 3, 'CNY', 1800000060, NULL, NULL, NULL, NULL)"#,
      vec![],
    )
    .await;
  // The supervisor stands on DeepSeek now.
  crate::support::rows::supervisor(scratch.db(), "chat-agent", &[("last_mode", "deepseek".into())]).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let (status, fallback) = get(server.port, token, "/fallback").await;

  assert_eq!(status, 200, "{fallback}");
  assert_eq!(fallback["active"]["provider"], "deepseek");

  // A provider that cannot answer here says why twice over: the code a screen words, and the sentence.
  let (_, providers) = get(server.port, token, "/providers").await;

  for road in providers["items"].as_array().unwrap() {
    assert_eq!(road["reason"].is_null(), road["reasonCode"].is_null(), "{road}");

    if road["name"] == "openrouter" && !road["reason"].is_null() {
      assert_eq!(road["reasonCode"], "openrouter-key-unusable", "{road}");
    }
  }
  // A provider's quota carries its balance whole: the amount, its currency, when it was seen and the request
  // allowance read with it.
  let quota = &providers["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|road| road["name"] == "deepseek")
    .unwrap()["quota"];

  assert_eq!(
    (&quota["balance"], &quota["currency"], &quota["balanceSeenAt"], &quota["requests"]),
    (
      &json!(12.5),
      &json!("CNY"),
      &json!(1800000000),
      &json!({ "used": 12, "limit": 50, "left": 38, "scope": "some-requests" })
    )
  );
  assert_eq!(fallback["active"]["session"], Value::Null);
  assert_eq!(
    fallback["active"]["balance"],
    json!({
      "amount": 12.5, "currency": "CNY", "seenAt": 1800000000,
      "requests": { "used": 12, "limit": 50, "left": 38, "scope": "some-requests" }
    })
  );

  let of = |listed: &Value, id: &str| {
    listed.as_array().unwrap().iter().find(|account| account["id"] == id).unwrap().clone()
  };
  let accounts = &fallback["accounts"];

  assert_eq!(of(accounts, "login-a")["session"]["utilization"], 0.25);
  assert_eq!(of(accounts, "login-a")["week"]["utilization"], 0.5);
  assert_eq!(of(accounts, "login-a")["balance"], Value::Null, "a login is paid by its subscription");
  assert_eq!(of(accounts, "paid")["session"], Value::Null);
  assert_eq!(of(accounts, "paid")["balance"]["amount"], 3.0);

  let (_, listed) = get(server.port, token, "/accounts").await;

  assert_eq!(
    of(&listed["items"], "paid")["balance"],
    json!({ "amount": 3.0, "currency": "CNY", "seenAt": 1800000060, "requests": null })
  );
  assert_eq!(of(&listed["items"], "login-a")["balance"], Value::Null);
}
