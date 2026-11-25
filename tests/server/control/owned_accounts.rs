//! The Claude accounts a person owns (docs/api.md, 「Who a caller acts for」; docs/supply.md, 「What a person
//! brings」, 「The lendable accounts」), through the agency's own client: a member's call reads and changes
//! their own accounts alone, pins what it adds to them and is refused the platform's order and choice; the
//! platform's calls read the platform's accounts and each person's roads; an account is lent where the
//! administrator marks it, whoever owns it, and a person marks none; and a person's sign-in waits for their
//! node's link.

use std::sync::Arc;

use agency_server::control::claude_accounts::{accounts, lent_accounts};
use agency_server::control::{Front, router};
use bmo_agency_client::{AgencyClient, ClientFailure, FallbackChange, ListAccountsQuery};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::grounds::ground_on;
use crate::support::scratch::Scratch;

pub(super) const TOKEN: &str = "secret";

// The front on a port of its own, over the ground given.
pub(super) async fn front(ground: agency_server::places::Ground) -> u16 {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let app = router(Front::new(TOKEN.to_owned(), Arc::new(ground)).unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  port
}

// The agency's client sending the console's headers naming a person, or none for the deployment's own call.
pub(super) fn client(port: u16, person: Option<(&str, &str)>) -> AgencyClient {
  let mut headers = reqwest::header::HeaderMap::new();

  if let Some((key, role)) = person {
    headers.insert("Owner-Key", key.parse().unwrap());
    headers.insert("Owner-Role", role.parse().unwrap());
  }

  AgencyClient::new(&format!("http://127.0.0.1:{port}"), TOKEN)
    .unwrap()
    .with_http(reqwest::Client::builder().default_headers(headers).build().unwrap())
}

// The code a refusal came with, and its status.
pub(super) fn refused<T: std::fmt::Debug>(answer: Result<T, ClientFailure>) -> (u16, String) {
  match answer {
    Err(ClientFailure::Answered {
      status,
      answer: Some(answer),
    }) => (status, answer.code),
    other => panic!("a refusal was due: {other:?}"),
  }
}

fn change(body: Value) -> FallbackChange {
  serde_json::from_value(body).unwrap()
}

// The platform's account `main`, the administrator's `owner`, and two people's, `seven` and `eight`; the
// administrator user 1, and person 8 holding a key for DeepSeek's road beside their login.
async fn accounts_of_four(scratch: &Scratch, home: &std::path::Path) {
  let login = home.join("login");

  std::fs::create_dir_all(&login).unwrap();
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "claude_accounts"
           ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at", "owner_account")
         VALUES ('main', 'main', 1, 1, 'directory', $1, 0, NULL),
                ('owner', 'owner', 2, 1, 'directory', $1, 0, 1),
                ('seven', 'seven', 3, 1, 'directory', $1, 0, 7),
                ('eight', 'eight', 4, 1, 'directory', $1, 0, 8)"#,
      vec![login.to_string_lossy().into_owned().into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '1')
         ON CONFLICT ("key") DO UPDATE SET "value" = EXCLUDED."value""#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "personal_supply" ("owner_account", "road", "secret", "place", "added_at")
         VALUES (8, 'deepseek', 'ds', 1, 0)"#,
      vec![],
    )
    .await;
}

fn ids(accounts: &[bmo_agency_client::FallbackAccount]) -> Vec<&str> {
  accounts.iter().map(|account| account.id.as_str()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_reads_their_own_accounts_and_the_platform_reads_its_own_and_each_person_s_roads() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  accounts_of_four(&scratch, home.path()).await;

  let port = front(ground_on(scratch.db().clone(), home.path())).await;
  let member = client(port, Some(("7", "member")));
  let administrator = client(port, Some(("1", "administrator")));
  let deployment = client(port, None);

  let own = member.get_fallback().await.unwrap();

  assert_eq!(ids(&own.accounts), ["seven"]);
  assert_eq!(own.accounts[0].user_id, Some(7));
  assert_eq!((own.active, own.personal_supply), (None, None));

  let platform = administrator.get_fallback().await.unwrap();
  let people: Vec<(i64, Vec<String>)> = platform
    .personal_supply
    .unwrap()
    .into_iter()
    .map(|person| (person.user, person.roads))
    .collect();

  assert_eq!(ids(&platform.accounts), ["main", "owner"]);
  assert_eq!(
    people,
    [
      (7, vec!["claude-code".to_owned()]),
      (8, vec!["claude-code".to_owned(), "deepseek".to_owned()])
    ]
  );

  // The accounts list answers the same: the person's own, and the platform's with each person's roads.
  let listed = |page: &bmo_agency_client::AccountPage| -> Vec<String> {
    page.items.iter().map(|account| account.id.clone()).collect()
  };
  let own = member.list_accounts(&ListAccountsQuery::default()).await.unwrap();
  let platform = deployment.list_accounts(&ListAccountsQuery::default()).await.unwrap();

  assert_eq!((listed(&own), own.personal_supply), (vec!["seven".to_owned()], None));
  assert_eq!(listed(&platform), ["main", "owner"]);
  assert_eq!(platform.personal_supply.map(|people| people.len()), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_changes_their_own_accounts_alone_and_a_platform_account_alone_is_lent() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  accounts_of_four(&scratch, home.path()).await;

  let key = home.path().join("data/env/users/7/anthropic");

  std::fs::create_dir_all(key.parent().unwrap()).unwrap();
  std::fs::write(&key, "sk-ant-api-test\n").unwrap();

  let port = front(ground_on(scratch.db().clone(), home.path())).await;
  let member = client(port, Some(("7", "member")));
  let deployment = client(port, None);
  let login = home.path().join("login").to_string_lossy().into_owned();

  // An account a member adds is pinned to them, its secret named in their own space.
  let added = member
    .change_fallback(&change(json!({
      "action": "add-account", "name": "keyed", "kind": "api-key", "path": login, "keyFile": "anthropic"
    })))
    .await
    .unwrap();
  let keyed = added.accounts.iter().find(|account| account.name == "keyed").unwrap();

  assert_eq!(ids(&added.accounts), ["seven", keyed.id.as_str()]);
  assert_eq!((keyed.user_id, keyed.credential_kind.as_str()), (Some(7), "api-key"));
  assert_eq!(keyed.key_file.as_deref(), key.to_str());

  // Another's account stands nowhere for them, and the platform's order and choice are not theirs.
  for asked in [
    json!({ "action": "update-account", "account": "eight", "name": "taken" }),
    json!({ "action": "set-account-enabled", "account": "main", "enabled": false }),
    json!({ "action": "remove-account", "account": "eight" }),
  ] {
    assert_eq!(
      refused(member.change_fallback(&change(asked)).await),
      (404, "unknown-account".to_owned())
    );
  }

  for asked in [
    json!({ "action": "set-account-order", "order": ["main", "owner", "seven", "eight"] }),
    json!({ "action": "choose-account", "account": "seven" }),
    json!({ "action": "clear-account-choice" }),
  ] {
    assert_eq!(refused(member.change_fallback(&change(asked)).await), (403, "forbidden".to_owned()));
  }

  // Any account is lent once the administrator marks it, the administrator's own among them; a person may mark
  // none, their own neither.
  let lent = deployment
    .change_fallback(&change(json!({ "action": "update-account", "account": "main", "lendable": true })))
    .await
    .unwrap();

  assert!(lent.accounts.iter().find(|account| account.id == "main").unwrap().lendable);
  assert_eq!(
    refused(
      member
        .change_fallback(&change(
          json!({ "action": "update-account", "account": "seven", "lendable": true })
        ))
        .await
    ),
    (403, "forbidden".to_owned())
  );
  deployment
    .change_fallback(&change(json!({ "action": "update-account", "account": "owner", "lendable": true })))
    .await
    .unwrap();

  let lendable: Vec<String> = lent_accounts(scratch.db(), 8)
    .await
    .unwrap()
    .into_iter()
    .map(|account| account.id)
    .collect();

  assert_eq!(lendable, ["main", "owner"]);
  assert!(
    accounts(scratch.db())
      .await
      .unwrap()
      .iter()
      .all(|account| ["main", "owner"].contains(&account.id.as_str()) || !account.lendable)
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_s_sign_in_says_each_refusal_and_signs_their_own_login_in_again_on_their_node() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  accounts_of_four(&scratch, home.path()).await;

  let port = front(ground_on(scratch.db().clone(), home.path())).await;
  let started = |body: Value| {
    reqwest::Client::new()
      .post(format!("http://127.0.0.1:{port}/claude-logins"))
      .bearer_auth(TOKEN)
      .header("Owner-Key", "7")
      .header("Owner-Role", "member")
      .json(&body)
      .send()
  };
  let refusal = |answer: reqwest::Response| async move {
    let status = answer.status().as_u16();
    let body: Value = answer.json().await.unwrap();

    (status, body["code"].as_str().unwrap_or_default().to_owned())
  };

  // A person names a login to sign in again or a new one's name, not both, and only their own login.
  assert_eq!(
    refusal(started(json!({ "account": "seven", "name": "mine" })).await.unwrap()).await,
    (400, "account-or-name".to_owned())
  );
  assert_eq!(
    refusal(started(json!({ "account": "eight" })).await.unwrap()).await,
    (404, "unknown-account".to_owned())
  );
  assert_eq!(
    refusal(started(json!({ "account": "main" })).await.unwrap()).await,
    (404, "unknown-account".to_owned())
  );

  // A name another account holds, the platform's here, is refused under its field, though the person sees none
  // of the platform's accounts.
  let taken = started(json!({ "name": "main" })).await.unwrap();
  assert_eq!(taken.status().as_u16(), 409);
  let taken: Value = taken.json().await.unwrap();
  assert_eq!((taken["code"].as_str(), taken["field"].as_str()), (Some("name-taken"), Some("name")));

  // Their isolation node's link was never heard here, whether they name the login or agency names it.
  assert_eq!(
    refusal(started(json!({ "name": "mine" })).await.unwrap()).await,
    (409, "node-not-attached".to_owned())
  );
  assert_eq!(refusal(started(json!({})).await.unwrap()).await, (409, "node-not-attached".to_owned()));
  assert_eq!(
    refusal(started(json!({ "account": "seven" })).await.unwrap()).await,
    (409, "node-not-attached".to_owned())
  );

  // Its link heard now, with no ready workplace yet, as while it is first set up.
  let node_dir = home.path().join("data/nodes/account-7");

  std::fs::create_dir_all(&node_dir).unwrap();
  std::fs::write(
    node_dir.join("link-state.json"),
    json!({ "state": "connected", "heard": agency_server::now_ms() }).to_string(),
  )
  .unwrap();
  assert_eq!(refusal(started(json!({})).await.unwrap()).await, (409, "node-not-ready".to_owned()));

  // Ready, a login of theirs that stands elsewhere is not signed in again there.
  scratch
    .run(
      r#"INSERT INTO "node_workplaces" ("node", "root", "home", "state", "created_at", "updated_at")
         VALUES ('account-7', '~/node-root', '/home/tester', 'ready', 0, 0)"#,
      vec![],
    )
    .await;
  assert_eq!(
    refusal(started(json!({ "account": "seven" })).await.unwrap()).await,
    (409, "login-elsewhere".to_owned())
  );
  scratch
    .run(r#"UPDATE "claude_accounts" SET "node" = 'n9' WHERE "id" = 'seven'"#, vec![])
    .await;
  assert_eq!(
    refusal(started(json!({ "account": "seven" })).await.unwrap()).await,
    (409, "login-elsewhere".to_owned())
  );

  // One standing on their node now, whose workplace holds no Claude Code of its own, hears so and the node's
  // setup is started to lay it, so a start a few minutes later finds it.
  scratch
    .run(r#"UPDATE "claude_accounts" SET "node" = 'account-7' WHERE "id" = 'seven'"#, vec![])
    .await;
  assert_eq!(
    refusal(started(json!({ "account": "seven" })).await.unwrap()).await,
    (409, "no-cli".to_owned())
  );

  // That Claude Code laid, the sign-in goes past every refusal to the node's herdr, which no test stands.
  std::fs::write(
    node_dir.join("setup.json"),
    json!({ "claude": "~/.bmo/workplace/bin/bmo-claude" }).to_string(),
  )
  .unwrap();
  assert_eq!(started(json!({ "account": "seven" })).await.unwrap().status().as_u16(), 502);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_owner_s_sign_in_says_each_refusal_by_its_own_code() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  accounts_of_four(&scratch, home.path()).await;

  let port = front(ground_on(scratch.db().clone(), home.path())).await;
  let started = |body: Value| {
    reqwest::Client::new()
      .post(format!("http://127.0.0.1:{port}/claude-logins"))
      .bearer_auth(TOKEN)
      .json(&body)
      .send()
  };
  let refusal = |answer: reqwest::Response| async move {
    let status = answer.status().as_u16();
    let body: Value = answer.json().await.unwrap();

    (status, body["code"].as_str().unwrap_or_default().to_owned())
  };

  assert_eq!(
    refusal(started(json!({ "account": "main", "name": "mine" })).await.unwrap()).await,
    (400, "account-or-name".to_owned())
  );
  assert_eq!(
    refusal(started(json!({ "account": "nine" })).await.unwrap()).await,
    (404, "unknown-account".to_owned())
  );
  // A person's login is named in the one namespace every account's names share.
  assert_eq!(
    refusal(started(json!({ "name": "seven" })).await.unwrap()).await,
    (409, "name-taken".to_owned())
  );
}
