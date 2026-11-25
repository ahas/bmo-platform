//! An owner and an organisation unit removed for good (docs/api.md, 「owners and organisation units removed for
//! good」), through the agency's own client on a test's ground, which reaches no user, unit or firewall of this
//! machine: the count says exactly what the removal takes and changes nothing, a second removal takes nothing,
//! another account's and the platform's rows stand as they stood, and only the administrator is let in, on numbers
//! that are positive whole numbers alone.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use agency_server::peers::Asking;
use agency_server::peers::node::{NodeRow, NodeService};
use bmo_agency_client::{AgencyClient, OrgUnitRemovalQuery};
use sea_orm::Value;

use super::owned_accounts::{TOKEN, client, front, refused};
use crate::support::grounds::ground_on;
use crate::support::rows;
use crate::support::scratch::Scratch;

fn administrator(port: u16) -> AgencyClient {
  client(port, Some(("1", "administrator")))
}

fn kinds(pairs: &[(&str, i64)]) -> BTreeMap<String, i64> {
  pairs.iter().map(|(kind, n)| ((*kind).to_owned(), *n)).collect()
}

fn zeros(of: &BTreeMap<String, i64>) -> BTreeMap<String, i64> {
  of.keys().map(|kind| (kind.clone(), 0)).collect()
}

/// A node service that lists nothing and keeps the names it was asked to take off its list.
#[derive(Default)]
struct Removals(Mutex<Vec<String>>);

impl NodeService for Removals {
  fn link_up<'a>(&'a self, _node: &'a str) -> Asking<'a, bool> {
    Box::pin(async { Ok(false) })
  }

  fn node_rows(&self) -> Asking<'_, Vec<NodeRow>> {
    Box::pin(async { Ok(vec![]) })
  }

  fn ssh_login<'a>(&'a self, _node: &'a str) -> Asking<'a, Option<Vec<String>>> {
    Box::pin(async { Ok(None) })
  }

  fn remove_node<'a>(&'a self, node: &'a str) -> Asking<'a, bool> {
    self.0.lock().unwrap().push(node.to_owned());

    Box::pin(async { Ok(true) })
  }
}

async fn rows_of(scratch: &Scratch, sql: &str) -> i64 {
  scratch.count(sql, vec![]).await
}

fn written(path: &Path, text: &str) {
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  std::fs::write(path, text).unwrap();
}

// Everything agency keeps of person 7, of person 8 beside them, and of no account, a row or a file of each kind.
async fn three_owners(scratch: &Scratch, data: &Path) {
  let db = scratch.db();

  for (name, owner) in [("seven-bmo", Some(7i64)), ("eight-bmo", Some(8)), ("platform-bmo", None)] {
    let with: Vec<(&str, Value)> = owner.map(|owner| ("owner_account", owner.into())).into_iter().collect();

    rows::supervisor(db, name, &with).await;
  }

  for (name, owner) in [("worker-u7", Some("7")), ("worker-u8", Some("8")), ("worker", None)] {
    scratch
      .run(
        r#"INSERT INTO "role_agents" ("name", "role", "owner_account") VALUES ($1, 'worker', $2)"#,
        vec![name.into(), owner.map(str::to_owned).into()],
      )
      .await;
  }

  for (token, state, account) in [
    ("t7a", "waiting", Some("7")),
    ("t7b", "released", Some("7")),
    ("t8", "waiting", Some("8")),
    ("tp", "waiting", None),
  ] {
    scratch
      .run(
        r#"INSERT INTO "role_slot_requests" ("token", "request_key", "owner", "source", "role", "provider", "model",
             "effort", "capacity", "state", "enqueued_at", "account")
           VALUES ($1, $1, 'someone', 'job', 'worker', 'claude-code', 'claude-opus-5', 'high', 1, $2, 1, $3)"#,
        vec![token.into(), state.into(), account.map(str::to_owned).into()],
      )
      .await;
  }

  for account in ["7", "8"] {
    scratch
      .run(
        r#"INSERT INTO "agent_attachments" VALUES ($1, 'supervisor', 'some-bmo', 'skill', 'tidy', 1)"#,
        vec![account.into()],
      )
      .await;
    scratch
      .run(
        r#"INSERT INTO "mcp_servers" ("account", "name", "key", "recorded_at") VALUES ($1, 'notes', $2, 1)"#,
        vec![account.into(), format!("own-{account}-notes").into()],
      )
      .await;
    written(&data.join(format!("agency/skills/{account}/tidy/SKILL.md")), "---\nname: tidy\n---\n");
    written(&data.join(format!("env/llm-gateway-calls/{account}.key")), "a key\n");
  }

  for (name, column, owner) in [
    ("seven-repo", "owner_account", Some(7i64)),
    ("eight-repo", "owner_account", Some(8)),
    ("platform-repo", "owner_account", None),
    ("unit-repo", "owner_unit", Some(40)),
    ("other-unit-repo", "owner_unit", Some(41)),
  ] {
    let with: Vec<(&str, Value)> = owner.map(|owner| (column, owner.into())).into_iter().collect();

    rows::repository(db, name, &format!("/nowhere/{name}"), &with).await;
  }

  scratch
    .run(
      r#"INSERT INTO "repository_node_checkouts" ("repository", "node", "path", "created_at")
         VALUES ('seven-repo', 'n1', '/nowhere/n1', 1), ('eight-repo', 'n1', '/nowhere/n1-8', 1)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "support_checkouts" ("repository", "slot", "made_at") VALUES ('seven-repo', 2, 1)"#,
      vec![],
    )
    .await;

  for (name, account) in [("seven-ai", Some(7i64)), ("eight-ai", Some(8)), ("platform-ai", None)] {
    scratch
      .run(
        r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
             "thinking", "walked", "added_at", "keeps_research", "account")
           VALUES ($1, $1, 'served', 200000, 8000, 1, 1, 1, 0, $2)"#,
        vec![name.into(), account.into()],
      )
      .await;
  }

  for (id, user, unit) in [
    ("seven", Some(7i64), None),
    ("eight", Some(8), None),
    ("forty", None, Some(40i64)),
    ("forty-one", None, Some(41)),
  ] {
    scratch
      .run(
        r#"INSERT INTO "claude_accounts" ("id", "name", "place", "enabled", "credential_kind", "credential_path",
             "added_at", "user_id", "unit_id")
           VALUES ($1, $1, 9, 1, 'directory', '/nowhere', 0, $2, $3)"#,
        vec![id.into(), user.into(), unit.into()],
      )
      .await;
  }

  for key in [
    "rest_until:seven",
    "model_rest:seven:opus",
    "model_rest:seventh:opus",
    "rest_until:eight",
  ] {
    scratch
      .run(r#"INSERT INTO "settings" ("key", "value") VALUES ($1, '1')"#, vec![key.into()])
      .await;
  }

  for (user, unit, secret) in [
    (Some(7i64), None, "ds7"),
    (Some(8), None, "ds8"),
    (None, Some(40i64), "ds40"),
    (None, Some(41), "ds41"),
  ] {
    scratch
      .run(
        r#"INSERT INTO "personal_supply" ("user_id", "unit_id", "road", "secret", "place", "added_at")
           VALUES ($1, $2, 'deepseek', $3, 1, 0)"#,
        vec![user.into(), unit.into(), secret.into()],
      )
      .await;
  }

  for (run, account, unit, state) in [
    ("job-71", 7i64, None, "open"),
    ("job-72", 7, None, "captured"),
    ("job-81", 8, None, "open"),
    ("job-91", 9, Some(40i64), "open"),
    ("job-92", 9, Some(41), "open"),
  ] {
    scratch
      .run(
        r#"INSERT INTO "supply_reservations"
             ("run", "opened_step", "last_step", "account", "unit_id", "source", "reservation", "amount_micros",
              "unit_micros", "covered_micros", "expires_at", "state")
           VALUES ($1, 1, 1, $2, $3, 'plan', $1, 1000000, 1000000, 10, NULL, $4)"#,
        vec![run.into(), account.into(), unit.into(), state.into()],
      )
      .await;
  }

  scratch
    .run(
      r#"INSERT INTO "gateway_call_keys" ("account", "key_hash", "issued_at") VALUES (7, 'h7', 1), (8, 'h8', 1)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "gateway_requests" ("at", "account", "model") VALUES (1, 7, 'm'), (2, 7, 'm'), (3, 8, 'm'), (4, NULL, 'm')"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "person_nodes" ("owner_account", "node", "shared", "set_at") VALUES ('7', 'u7pc', false, 1), ('8', 'u8pc', false, 1)"#,
      vec![],
    )
    .await;

  for node in ["account-7", "u7pc", "account-8", "u8pc"] {
    scratch
      .run(
        r#"INSERT INTO "node_workplaces" ("node", "root", "runs", "state", "created_at", "updated_at") VALUES ($1, '~/w', 0, 'ready', 1, 1)"#,
        vec![node.into()],
      )
      .await;
    written(&data.join(format!("nodes/{node}/setup.json")), "{}\n");
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_owner_s_count_is_what_its_removal_takes_and_another_s_and_the_platform_s_stand() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");

  three_owners(&scratch, &data).await;

  let removals = Arc::new(Removals::default());
  let mut ground = ground_on(scratch.db().clone(), home.path());

  ground.peers.node = removals.clone();

  let port = front(ground).await;
  let owner = administrator(port);
  let expected = kinds(&[
    ("supervisors", 1),
    ("slot-requests", 2),
    ("role-agents", 1),
    ("attachments", 1),
    ("mcp-servers", 1),
    ("skills", 1),
    ("repositories", 1),
    ("repository-checkouts", 2),
    ("registered-models", 1),
    ("claude-accounts", 1),
    ("personal-supply", 1),
    ("supply-reservations", 2),
    ("call-keys", 1),
    ("gateway-requests", 2),
    ("person-nodes", 1),
    ("workplaces", 2),
    ("isolation-users", 0),
  ]);

  assert_eq!(owner.count_owner_removal(7).await.unwrap().counts, expected);
  assert_eq!(owner.count_owner_removal(7).await.unwrap().counts, expected, "a count changes nothing");
  assert!(
    removals.0.lock().unwrap().is_empty(),
    "a count takes no node off the node service's list"
  );
  assert_eq!(owner.remove_owner(7).await.unwrap().removed, expected);
  // The PC the account's record named alone is off the node service's list, so its herdr pane ends.
  assert_eq!(*removals.0.lock().unwrap(), ["u7pc"]);
  assert_eq!(owner.count_owner_removal(7).await.unwrap().counts, zeros(&expected));
  assert_eq!(
    owner.remove_owner(7).await.unwrap().removed,
    zeros(&expected),
    "a second removal takes nothing"
  );

  // Nothing of person 7's is left.
  for (table, column) in [
    ("supervisors", r#""owner_account" = 7"#),
    ("role_agents", r#""owner_account" = '7'"#),
    ("role_slot_requests", r#""account" = '7'"#),
    ("agent_attachments", r#""account" = '7'"#),
    ("mcp_servers", r#""account" = '7'"#),
    ("repositories", r#""owner_account" = 7"#),
    ("repository_node_checkouts", r#""repository" = 'seven-repo'"#),
    ("support_checkouts", r#""repository" = 'seven-repo'"#),
    ("registered_models", r#""account" = 7"#),
    ("claude_accounts", r#""user_id" = 7"#),
    ("personal_supply", r#""user_id" = 7"#),
    ("supply_reservations", r#""account" = 7"#),
    ("gateway_call_keys", r#""account" = 7"#),
    ("gateway_requests", r#""account" = 7"#),
    ("person_nodes", r#""owner_account" = '7'"#),
    ("node_workplaces", r#""node" IN ('account-7', 'u7pc')"#),
    ("settings", r#""key" IN ('rest_until:seven', 'model_rest:seven:opus')"#),
  ] {
    assert_eq!(
      rows_of(&scratch, &format!(r#"SELECT COUNT(*) FROM "{table}" WHERE {column}"#)).await,
      0,
      "{table}"
    );
  }

  for gone in ["agency/skills/7", "env/llm-gateway-calls/7.key", "nodes/account-7", "nodes/u7pc"] {
    assert!(!data.join(gone).exists(), "{gone} is gone");
  }

  // The open reservation was captured through the outbox before its row went.
  assert_eq!(
    rows_of(&scratch, r#"SELECT COUNT(*) FROM "supply_settlements" WHERE "key" LIKE 'job-71:%'"#).await,
    2
  );

  // Person 8's and the platform's stand as they stood.
  for (table, column, left) in [
    ("supervisors", r#""owner_account" = 8 OR "owner_account" IS NULL"#, 2),
    ("role_agents", r#""owner_account" = '8' OR "owner_account" IS NULL"#, 2),
    (
      "role_slot_requests",
      r#"("account" = '8' OR "account" IS NULL) AND "state" = 'waiting'"#,
      2,
    ),
    ("agent_attachments", r#""account" = '8'"#, 1),
    ("mcp_servers", r#""account" = '8'"#, 1),
    (
      "repositories",
      r#""owner_account" = 8 OR ("owner_account" IS NULL AND "owner_unit" IS NULL)"#,
      2,
    ),
    ("repository_node_checkouts", r#""repository" = 'eight-repo'"#, 1),
    ("registered_models", r#""account" = 8 OR "account" IS NULL"#, 2),
    ("claude_accounts", r#""user_id" = 8"#, 1),
    ("personal_supply", r#""user_id" = 8"#, 1),
    ("supply_reservations", r#""account" IN (8, 9)"#, 3),
    ("gateway_call_keys", r#""account" = 8"#, 1),
    ("gateway_requests", r#""account" = 8 OR "account" IS NULL"#, 2),
    ("person_nodes", r#""owner_account" = '8'"#, 1),
    ("node_workplaces", r#""node" IN ('account-8', 'u8pc')"#, 2),
    ("settings", r#""key" IN ('model_rest:seventh:opus', 'rest_until:eight')"#, 2),
  ] {
    assert_eq!(
      rows_of(&scratch, &format!(r#"SELECT COUNT(*) FROM "{table}" WHERE {column}"#)).await,
      left,
      "{table}"
    );
  }

  for kept in [
    "agency/skills/8/tidy/SKILL.md",
    "env/llm-gateway-calls/8.key",
    "nodes/account-8",
    "nodes/u8pc",
  ] {
    assert!(data.join(kept).exists(), "{kept} stands");
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_unit_s_count_is_what_its_removal_takes_and_another_unit_s_stand() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  three_owners(&scratch, &home.path().join("data")).await;

  let port = front(ground_on(scratch.db().clone(), home.path())).await;
  let owner = administrator(port);
  let company = OrgUnitRemovalQuery { company: Some(40) };
  let expected = kinds(&[
    ("repositories", 1),
    ("repository-checkouts", 0),
    ("organisation-supply", 1),
    ("claude-accounts", 1),
    ("supply-reservations", 1),
  ]);

  assert_eq!(owner.count_org_unit_removal(40, &company).await.unwrap().counts, expected);
  assert_eq!(owner.remove_org_unit(40, &company).await.unwrap().removed, expected);
  assert_eq!(owner.count_org_unit_removal(40, &company).await.unwrap().counts, zeros(&expected));
  assert_eq!(owner.remove_org_unit(40, &company).await.unwrap().removed, zeros(&expected));

  for (table, column, left) in [
    ("repositories", r#""owner_unit" = 40"#, 0),
    ("personal_supply", r#""unit_id" = 40"#, 0),
    ("claude_accounts", r#""unit_id" = 40"#, 0),
    ("supply_reservations", r#""unit_id" = 40"#, 0),
    ("repositories", r#""owner_unit" = 41"#, 1),
    ("personal_supply", r#""unit_id" = 41"#, 1),
    ("claude_accounts", r#""unit_id" = 41"#, 1),
    ("supply_reservations", r#""unit_id" = 41"#, 1),
    // The person's own reservation stands, taken off the unit and captured.
    (
      "supply_reservations",
      r#""run" = 'job-91' AND "unit_id" IS NULL AND "state" = 'captured'"#,
      1,
    ),
    ("personal_supply", r#""user_id" IS NOT NULL"#, 2),
    ("claude_accounts", r#""user_id" IS NOT NULL"#, 2),
  ] {
    assert_eq!(
      rows_of(&scratch, &format!(r#"SELECT COUNT(*) FROM "{table}" WHERE {column}"#)).await,
      left,
      "{table} where {column}"
    );
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_administrator_removes_and_only_on_positive_whole_numbers() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  three_owners(&scratch, &home.path().join("data")).await;

  let port = front(ground_on(scratch.db().clone(), home.path())).await;
  let company = OrgUnitRemovalQuery { company: Some(40) };

  for caller in [client(port, Some(("7", "member"))), client(port, None)] {
    assert_eq!(refused(caller.count_owner_removal(7).await), (403, "forbidden".to_owned()));
    assert_eq!(refused(caller.remove_owner(7).await), (403, "forbidden".to_owned()));
    assert_eq!(
      refused(caller.count_org_unit_removal(40, &company).await),
      (403, "forbidden".to_owned())
    );
    assert_eq!(refused(caller.remove_org_unit(40, &company).await), (403, "forbidden".to_owned()));
  }

  assert_eq!(
    rows_of(&scratch, r#"SELECT COUNT(*) FROM "supervisors" WHERE "owner_account" = 7"#).await,
    1
  );

  let http = reqwest::Client::new();
  let asked = |method: reqwest::Method, path: &str| {
    http
      .request(method, format!("http://127.0.0.1:{port}{path}"))
      .bearer_auth(TOKEN)
      .header("Owner-Key", "1")
      .header("Owner-Role", "administrator")
  };

  for path in [
    "/owners/seven/removal",
    "/owners/0/removal",
    "/owners/-7/removal",
    "/owners/7.0/removal",
    "/org-units/x/removal",
    "/org-units/40/removal?company=forty",
    "/org-units/40/removal?company=0",
  ] {
    let answer = asked(reqwest::Method::GET, path).send().await.unwrap();

    assert_eq!(answer.status().as_u16(), 400, "{path}");
  }

  for path in ["/owners/seven", "/owners/0", "/org-units/-40", "/org-units/40?company=x"] {
    let answer = asked(reqwest::Method::DELETE, path).send().await.unwrap();

    assert_eq!(answer.status().as_u16(), 400, "{path}");
  }

  assert_eq!(
    rows_of(&scratch, r#"SELECT COUNT(*) FROM "supervisors" WHERE "owner_account" = 7"#).await,
    1
  );
  assert_eq!(
    rows_of(&scratch, r#"SELECT COUNT(*) FROM "repositories" WHERE "owner_unit" = 40"#).await,
    1
  );
}
