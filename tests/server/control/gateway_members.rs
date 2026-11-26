//! The gateway's members as the owner reads them and sets their owners (docs/api.md, 「providers, models and
//! model files」; docs/own-llm.md, 「The gateway's members」), through the agency's own client: the deployment's
//! and the administrator's calls read every member with its owner, a member reads only the members owned by
//! their own account, an owner is written into the members file — an account's or a unit's, one at most — with
//! every other field and member left as they stood and the file's mode kept, a name the file holds none of, an
//! owner that is no whole number and a member naming both owners are refused, and a member's change is refused.

use std::os::unix::fs::PermissionsExt;

use bmo_agency_client::{GatewayMember, GatewayMemberChange};
use serde_json::{Value, json};

use super::owned_accounts::{client, front, refused};
use crate::support::grounds::ground_on;
use crate::support::scratch::Scratch;

// Two members, one of a road and one the gateway serves alone, with a field no route names.
fn members() -> Value {
  json!([
    {
      "name": "router-flash", "road": "openrouter", "label": "Router Flash", "provider": "openrouter",
      "model": "vendor/flash-1", "key_file": "openrouter.key", "context_tokens": 131072,
      "answer_tokens": 8192, "thinking": false, "free": true,
    },
    { "name": "local-chat", "provider": "openai", "model": "chat", "api_base": "http://127.0.0.1:9/v1",
      "key_file": "local.key" },
  ])
}

fn file_of(home: &std::path::Path) -> std::path::PathBuf {
  home.join("data/llm-gateway/members.json")
}

fn held(home: &std::path::Path) -> Value {
  serde_json::from_str(&std::fs::read_to_string(file_of(home)).unwrap()).unwrap()
}

// The change that makes the member an account's, or the platform's where none; the unit axis left standing.
fn owned(account: Option<i64>) -> GatewayMemberChange {
  GatewayMemberChange {
    account: Some(account),
    unit: None,
  }
}

// The change that files the member under a unit, or the platform's where none; the account axis left standing.
fn filed(unit: Option<i64>) -> GatewayMemberChange {
  GatewayMemberChange {
    account: None,
    // The named axis carries the null too: naming an owner as null gives it back to the platform.
    unit: Some(unit),
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_owner_reads_the_members_and_sets_one_s_owner_in_the_file_alone() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  std::fs::create_dir_all(file_of(home.path()).parent().unwrap()).unwrap();
  std::fs::write(file_of(home.path()), members().to_string()).unwrap();
  std::fs::set_permissions(file_of(home.path()), std::fs::Permissions::from_mode(0o640)).unwrap();

  let port = front(ground_on(scratch.db().clone(), home.path())).await;
  let deployment = client(port, None);
  let administrator = client(port, Some(("1", "administrator")));
  let member = client(port, Some(("7", "member")));
  let flash = |account: Option<i64>, unit: Option<i64>| GatewayMember {
    name: "router-flash".to_owned(),
    road: Some("openrouter".to_owned()),
    label: Some("Router Flash".to_owned()),
    provider: "openrouter".to_owned(),
    account,
    unit,
  };

  // The administrator's read is every member, whichever way each stands owned.
  let listed = administrator.list_gateway_members().await.unwrap().items;

  assert_eq!(listed[0], flash(None, None));
  assert_eq!(
    (listed[1].name.as_str(), &listed[1].road, listed[1].account, listed[1].unit),
    ("local-chat", &None, None, None)
  );

  // While the file's members are the platform's, a member reads none of them.
  assert!(member.list_gateway_members().await.unwrap().items.is_empty());

  // The owner written is the file's, the rest of the member and the other member as they stood.
  assert_eq!(
    deployment.change_gateway_member("router-flash", &owned(Some(7))).await.unwrap(),
    flash(Some(7), None)
  );

  let mut expected = members();

  expected[0]["account"] = json!(7);
  assert_eq!(held(home.path()), expected);
  assert_eq!(std::fs::metadata(file_of(home.path())).unwrap().permissions().mode() & 0o777, 0o640);
  assert_eq!(deployment.list_gateway_members().await.unwrap().items[0], flash(Some(7), None));

  // The member whose account owns a member reads that member and no other of the file.
  let seen = member.list_gateway_members().await.unwrap().items;

  assert_eq!(seen, vec![flash(Some(7), None)]);

  // The unit replaces the account in the same write: a member never stands under two owners.
  assert_eq!(
    deployment.change_gateway_member("router-flash", &filed(Some(4))).await.unwrap(),
    flash(None, Some(4))
  );

  expected[0].as_object_mut().unwrap().remove("account");
  expected[0]["unit"] = json!(4);
  assert_eq!(held(home.path()), expected);

  // Null gives the member back to the platform.
  assert_eq!(
    deployment.change_gateway_member("router-flash", &filed(None)).await.unwrap(),
    flash(None, None)
  );
  assert_eq!(held(home.path()), members());

  assert_eq!(
    refused(deployment.change_gateway_member("nobody", &owned(Some(7))).await),
    (404, "unknown-member".to_owned())
  );

  let post = |body: Value| {
    reqwest::Client::new()
      .patch(format!("http://127.0.0.1:{port}/gateway-members/router-flash"))
      .bearer_auth("secret")
      .json(&body)
      .send()
  };

  let bad = post(json!({ "account": "seven" })).await.unwrap();

  assert_eq!(bad.status().as_u16(), 400);
  assert_eq!(bad.json::<Value>().await.unwrap()["field"], "account");

  let bad = post(json!({ "unit": "four" })).await.unwrap();

  assert_eq!(bad.status().as_u16(), 400);
  assert_eq!(bad.json::<Value>().await.unwrap()["field"], "unit");

  // One member has one owner at most: the change naming both is refused, the file left as it stands.
  let bad = post(json!({ "account": 7, "unit": 4 })).await.unwrap();

  assert_eq!(bad.status().as_u16(), 400);
  assert_eq!(held(home.path()), members());

  // Who owns a member is not a member's to set.
  assert_eq!(
    refused(member.change_gateway_member("router-flash", &owned(Some(7))).await),
    (403, "forbidden".to_owned())
  );
  assert_eq!(held(home.path()), members());

  // A file that does not read as members is never written over.
  let unread = r#"[{ "name": "router-flash" }]"#;

  std::fs::write(file_of(home.path()), unread).unwrap();
  assert_eq!(
    refused(deployment.change_gateway_member("router-flash", &owned(Some(7))).await),
    (503, "members-unread".to_owned())
  );
  assert_eq!(std::fs::read_to_string(file_of(home.path())).unwrap(), unread);
}
