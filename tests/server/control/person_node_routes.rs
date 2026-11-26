//! Who reaches a node and whether an account's node is linked, read over the control routes of a running server
//! (docs/api.md, 「person-nodes」): the answer a node service's tool reads as agency's client types it, the root of a
//! caller's account sharing the node among it, a caller left unnamed refused, and each record with its link's state.

use agency_server::person_nodes;
use bmo_agency_client::{NodeAccess, PersonNode, PersonNodeList};

use crate::support::rows::supervisor;
use crate::support::scratch::Scratch;
use crate::support::standing;

#[tokio::test(flavor = "multi_thread")]
async fn a_running_server_answers_who_reaches_a_node_and_whether_its_link_is_up() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  supervisor(scratch.db(), "u7", &[("owner_account", 7i64.into())]).await;
  person_nodes::set(scratch.db(), "7", "pc-7", true, false).await.unwrap().unwrap();
  scratch
    .run(
      r#"INSERT INTO "node_workplaces" ("node", "root", "home", "state", "created_at", "updated_at")
         VALUES ('pc-7', '~/node-root', '/home/tester', 'ready', 0, 0)"#,
      vec![],
    )
    .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let client = reqwest::Client::new();
  let ask = |query: &'static str| {
    client
      .get(format!("http://127.0.0.1:{}/nodes/pc-7/access{query}", server.port))
      .bearer_auth(token)
      .send()
  };

  let answer = ask("?agent=u7&checkout=%2Fwork").await.unwrap();

  assert_eq!(answer.status().as_u16(), 200);
  assert_eq!(
    answer.json::<NodeAccess>().await.unwrap(),
    NodeAccess {
      node: "pc-7".into(),
      owners: vec!["7".into()],
      agent: "u7".into(),
      account: Some("7".into()),
      administrator: false,
      reaches: true,
      root: Some("/home/tester/node-root/accounts/7".into()),
    }
  );
  let blank = ask("?agent=%20").await.unwrap();

  assert_eq!(blank.status().as_u16(), 200);
  assert_eq!(
    blank.json::<NodeAccess>().await.unwrap(),
    NodeAccess {
      node: "pc-7".into(),
      owners: vec!["7".into()],
      agent: String::new(),
      account: None,
      administrator: false,
      reaches: false,
      root: None,
    },
    "a blank agent names no caller: the owners alone are answered, as the node service's read asks them"
  );

  std::fs::create_dir_all(data.join("nodes/pc-7")).unwrap();
  std::fs::write(
    data.join("nodes/pc-7/link-state.json"),
    format!(r#"{{"state": "connected", "heard": {}}}"#, chrono::Utc::now().timestamp_millis()),
  )
  .unwrap();
  let list = client
    .get(format!("http://127.0.0.1:{}/person-nodes", server.port))
    .bearer_auth(token)
    .send()
    .await
    .unwrap()
    .json::<PersonNodeList>()
    .await
    .unwrap();
  let read: Vec<(&str, &str, bool)> = list
    .items
    .iter()
    .map(|one: &PersonNode| (one.owner_account.as_str(), one.node.as_str(), one.linked))
    .collect();

  assert_eq!(read, vec![("7", "pc-7", true)], "a node connected and heard from just now is linked");
}
