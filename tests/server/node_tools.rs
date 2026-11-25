//! A supervisor's tools on a node and the capabilities that operate the platform (docs/person-nodes.md, 「A
//! supervisor's tools on a node」 and 「Who reaches a node」): a person's supervisor, whose program stands on a node,
//! is handed the tool host's namespaces carrying the work of the servers that run with this machine's addresses and
//! secrets, and holds neither `deploy` nor `research.owner`; the administrator's supervisors and the deployment's
//! own keep their servers and hold both.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use agency_server::lines::Trees;
use agency_server::lines::chat::mcp_config;
use agency_server::supervisor_limits::Limits;
use serde_json::{Value, json};

use crate::support::grounds::seeded_with;
use crate::support::rows;
use crate::support::scratch::Scratch;

// A tool host standing in on `socket` that answers every record it is asked to issue and keeps it.
fn issuing_host(socket: &Path) -> Arc<Mutex<Vec<Value>>> {
  let issued = Arc::new(Mutex::new(Vec::new()));
  let kept = issued.clone();
  let listener = tokio::net::UnixListener::bind(socket).unwrap();
  let app = axum::Router::new().route(
    "/tool-sessions",
    axum::routing::post(move |axum::Json(mut record): axum::Json<Value>| {
      let kept = kept.clone();

      async move {
        kept.lock().unwrap().push(record.clone());
        record["token"] = json!("a-token");
        for (field, empty) in [("shown", json!([])), ("variables", json!({})), ("labels", json!({}))] {
          if record.get(field).is_none() {
            record[field] = empty;
          }
        }
        record["createdAt"] = json!(1);
        record["expiresAt"] = Value::Null;

        (axum::http::StatusCode::CREATED, axum::Json(record))
      }
    }),
  );

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  issued
}

fn words(record: &Value, field: &str) -> Vec<String> {
  record[field]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|word| word.as_str().map(str::to_owned))
    .collect()
}

// "a person's supervisor holds no capability that operates the platform, and its program on a node is handed the
// tool host's namespaces in place of the servers it could not start there; the administrator's and the deployment's
// own supervisors hold both capabilities and keep their servers"
#[tokio::test]
async fn a_member_s_record_reaches_the_hosted_tools_and_holds_no_capability_that_operates_the_platform() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let issued = issuing_host(&socket);
  let at = |path: &Path| path.to_string_lossy().into_owned();
  let more = BTreeMap::from([
    ("BMO_MCP_HOST_SOCKET".to_owned(), at(&socket)),
    ("BMO_MCP_HOST_ISSUER_TOKEN".to_owned(), "issuer".to_owned()),
    ("BMO_JOB_PLUGIN".to_owned(), "/opt/bmo-job".to_owned()),
    ("BMO_JOB_URL".to_owned(), "http://[::1]:1".to_owned()),
    ("BMO_AGENCY_URL".to_owned(), "http://[::1]:2".to_owned()),
    ("BMO_EXPLORER_PLUGIN".to_owned(), "/opt/bmo-explorer".to_owned()),
    ("BMO_DATA_DIR".to_owned(), at(&home.path().join("data"))),
  ]);
  let ground = seeded_with(db, home.path(), &more).await;

  rows::supervisor(db, "u60", &[("owner_account", 60i64.into())]).await;
  rows::supervisor(db, "u9", &[("owner_account", 9i64.into())]).await;
  rows::supervisor(db, "keeper", &[]).await;
  scratch
    .run(r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '9')"#, vec![])
    .await;
  scratch
    .run(
      r#"INSERT INTO "person_nodes" ("owner_account", "node", "shared", "set_at") VALUES ('60', 'u60', false, 1)"#,
      vec![],
    )
    .await;

  let issue = |agent: &'static str| {
    let (ground, issued) = (&ground, issued.clone());

    async move {
      let env = BTreeMap::from([("BMO_TURN_AGENT".to_owned(), agent.to_owned())]);

      agency_server::lines::token::issue_for_run(ground, "chat", agent, "/tmp", &env, None, u64::MAX)
        .await
        .unwrap();

      issued.lock().unwrap().pop().expect("a record was issued")
    }
  };

  let member = issue("u60").await;
  let capabilities = words(&member, "capabilities");
  let namespaces = words(&member, "namespaces");
  let shown = words(&member, "shown");

  for operating in ["deploy", "research.owner"] {
    assert!(!capabilities.contains(&operating.to_owned()), "{operating}: {member}");
  }
  for hosted in ["jobs.file", "documents.file", "web.read", "room.tools"] {
    assert!(capabilities.contains(&hosted.to_owned()), "{hosted}: {member}");
  }
  for namespace in ["agency", "artifact", "web", "bmo"] {
    assert!(namespaces.contains(&namespace.to_owned()), "{namespace}: {member}");
  }
  for entry in ["agency/file_item", "artifact/artifact_publish", "web/web_fetch", "bmo/say"] {
    assert!(shown.contains(&entry.to_owned()), "{entry}: {member}");
  }
  assert!(member["grants"].as_array().is_none_or(Vec::is_empty), "{member}");

  for agent in ["u9", "keeper"] {
    let record = issue(agent).await;
    let capabilities = words(&record, "capabilities");
    let namespaces = words(&record, "namespaces");

    for operating in ["deploy", "research.owner"] {
      assert!(capabilities.contains(&operating.to_owned()), "{agent} {operating}: {record}");
    }
    assert!(!capabilities.contains(&"jobs.file".to_owned()), "{agent}: {record}");
    for namespace in ["agency", "artifact", "web"] {
      assert!(!namespaces.contains(&namespace.to_owned()), "{agent} {namespace}: {record}");
    }
  }

  // The configuration of the member's program, which stands on its node, names the tool host's namespaces in place
  // of the servers that run with this machine's addresses and secrets; the deployment's own keeps those servers.
  let config = |agent: &'static str| {
    let ground = &ground;

    async move {
      let limits = Limits::of(&ground.db, agent).await.unwrap();
      let path = mcp_config(ground, &Trees::of(ground), agent, &limits).unwrap();
      let config: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();

      config["mcpServers"].clone()
    }
  };
  let on_node = config("u60").await;

  for server in ["job", "explorer"] {
    assert!(on_node[server].is_null(), "{server}: {on_node}");
  }
  for namespace in ["agency", "web", "artifact"] {
    assert_eq!(on_node[namespace]["type"], "http", "{namespace}: {on_node}");
    assert!(
      on_node[namespace]["url"].as_str().unwrap().ends_with(&format!("/mcp/{namespace}")),
      "{on_node}"
    );
  }

  let at_home = config("keeper").await;

  assert_eq!(at_home["agency"]["type"], "stdio", "{at_home}");
  assert!(at_home["job"].is_object() && at_home["explorer"].is_object(), "{at_home}");
  assert!(at_home["web"].is_null(), "{at_home}");

  // A tool limit closes what stands in on a node as it closes the servers at home.
  let limited = Limits::new(true, true, Some(vec!["web.pages".to_owned()])).standing_on_node(true);

  assert_eq!(limited.namespaces("chat").iter().filter(|name| *name == "web").count(), 1);
  assert!(!limited.namespaces("chat").contains(&"agency".to_owned()));
  assert!(!limited.keeps_server("explorer") && !limited.keeps_server("agency"));
}
