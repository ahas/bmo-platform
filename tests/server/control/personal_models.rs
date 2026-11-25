//! A person's own local AI (docs/supply.md, 「A person's local AI」 and 「Testing a source」; docs/api.md, 「accounts,
//! rest and usage」), through the agency's own client: a person with a PC of their own adds one, it stands as a
//! registration they own reached through their PC at a port of the policy's range, and its test is sent from the PC
//! over the link's runner socket; a person with no PC of their own, and one whose chain forbids it, are refused, and a
//! run on one their chain now forbids finds no personal supply and is told why.

use std::collections::BTreeMap;
use std::path::Path;

use agency_server::supply::{self, Shortfall};
use bmo_agency_client::{
  AgencyClient, LocalModelTried, PersonalModelAsk, PersonalModelChange, SupplyTestAsk,
};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use policy::supply::SUPPLY;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::owned_accounts::{client, front, refused};
use crate::support::grounds::ground_with;
use crate::support::scratch::Scratch;

const KEY: &str = "local-secret";

// The account service standing in: `supply.personal-local-ai` off for person 8, every other kind saying nothing.
async fn account_service() -> String {
  let app = axum::Router::new().route(
    "/chain/kinds/{key}/resolve",
    axum::routing::get(
      |axum::extract::Path(key): axum::extract::Path<String>,
       axum::extract::Query(query): axum::extract::Query<BTreeMap<String, String>>| async move {
        let off = key == "supply.personal-local-ai" && query.get("person").map(String::as_str) == Some("8");

        axum::Json(json!({ "value": if off { json!(false) } else { Value::Null } }))
      },
    ),
  );
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  address
}

// One request the local AI standing in answers: the model it serves under the key it takes, and a refusal of any
// other key.
async fn local_answer(
  request: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<Full<Bytes>>, String> {
  let keyed =
    request.headers().get("authorization").and_then(|value| value.to_str().ok()) == Some("Bearer a-key");
  let path = request.uri().path().to_owned();
  let body: Value =
    serde_json::from_slice(&request.into_body().collect().await.unwrap().to_bytes()).unwrap();
  let (status, answer) = match (path.as_str(), keyed, body["model"].as_str()) {
    ("/v1/chat/completions", true, Some("qwen3")) => {
      (200, json!({ "choices": [{ "message": { "content": "Hi" } }] }))
    }
    ("/v1/chat/completions", false, _) => (401, json!({ "error": { "message": "invalid api key" } })),
    _ => (404, json!({ "error": "model not found" })),
  };

  Ok(
    hyper::Response::builder()
      .status(status)
      .body(Full::new(Bytes::from(answer.to_string())))
      .unwrap(),
  )
}

// A person's PC's link standing in on its runner socket: an ask naming person 7 and the local AI's host is reached
// and the stream served as the local AI; any other host is not reached; any other person is refused. Every ask is
// kept.
fn pc_link(socket: &Path) -> std::sync::Arc<std::sync::Mutex<Vec<Value>>> {
  let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
  let kept = asked.clone();
  let listener = tokio::net::UnixListener::bind(socket).unwrap();

  tokio::spawn(async move {
    loop {
      let Ok((mut stream, _)) = listener.accept().await else { continue };
      let kept = kept.clone();

      tokio::spawn(async move {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];

        while stream.read(&mut byte).await.unwrap_or(0) == 1 && byte[0] != b'\n' {
          line.push(byte[0]);
        }

        let ask: Value = serde_json::from_slice(&line).unwrap();

        kept.lock().unwrap().push(ask.clone());

        let answer = match (ask["owner"].as_str(), ask["reach"].as_str()) {
          (Some("7"), Some("localhost:11434")) => json!({ "t": "reached" }),
          (Some("7"), Some(to)) => json!({ "t": "refused", "why": format!("{to}: connection refused") }),
          _ => json!({ "t": "refused", "why": "runs no personal tool of that account" }),
        };
        let reached = answer["t"] == "reached";

        stream.write_all(format!("{answer}\n").as_bytes()).await.unwrap();

        if reached {
          let _ = hyper::server::conn::http1::Builder::new()
            .serve_connection(
              hyper_util::rt::TokioIo::new(stream),
              hyper::service::service_fn(local_answer),
            )
            .await;
        }
      });
    }
  });

  asked
}

fn local_ask(url: &str, key: Option<&str>) -> PersonalModelAsk {
  PersonalModelAsk {
    url: url.to_owned(),
    model_name: "qwen3".to_owned(),
    key_name: key.map(str::to_owned),
    context_tokens: None,
  }
}

async fn tested(
  client: &AgencyClient,
  model: Option<&str>,
  local: Option<LocalModelTried>,
) -> Option<String> {
  let answer = client
    .test_supply(&SupplyTestAsk {
      road: "registered".to_owned(),
      model: model.map(str::to_owned),
      local,
    })
    .await
    .unwrap();

  assert_eq!(answer.ok, answer.reason_code.is_none(), "{answer:?}");
  answer.reason_code
}

fn tried(url: &str, key: Option<&str>) -> Option<LocalModelTried> {
  Some(LocalModelTried {
    url: url.to_owned(),
    model_name: "qwen3".to_owned(),
    key_name: key.map(str::to_owned),
  })
}

const ADDRESS: &str = "http://localhost:11434/v1/chat/completions";

// "a person with a PC of their own adds a local AI, which stands as a registration they own reached through their PC
// at a port of the policy's range, is tried from the PC before and after it is kept, and is carried by their PC's
// link; one with no PC and one whose chain forbids it are refused; a run on one the chain forbids is withheld"
#[tokio::test(flavor = "multi_thread")]
async fn a_person_adds_a_local_ai_tried_from_their_pc_under_their_chain() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let address = account_service().await;
  let variables = BTreeMap::from([
    ("WEB_ADDRESS".to_owned(), address),
    ("WEB_TOKEN".to_owned(), "chain-token".to_owned()),
  ]);
  let ground = ground_with(scratch.db().clone(), home.path(), &variables);
  let roots = ground.roots.clone();
  let env = roots.data.join("env").join("users").join("7");

  std::fs::create_dir_all(&env).unwrap();
  std::fs::write(env.join(KEY), "a-key\n").unwrap();
  std::fs::write(env.join("wrong"), "not-the-key\n").unwrap();
  scratch
    .run(r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '1')"#, vec![])
    .await;
  // Persons 7 and 8 each hold a PC of their own whose link runs their personal tools; person 9 holds none.
  for (owner, node) in [("7", "u7pc"), ("8", "u8pc")] {
    scratch
      .run(
        r#"INSERT INTO "person_nodes" ("owner_account", "node", "shared", "set_at") VALUES ($1, $2, false, 1)"#,
        vec![owner.into(), node.into()],
      )
      .await;

    let spec_path = roots.spec_path(node);

    std::fs::create_dir_all(spec_path.parent().unwrap()).unwrap();
    std::fs::write(
      &spec_path,
      json!({
        "node": node, "ssh": [], "root": "/pc", "binary": "/pc/deploy/bin/node-link",
        "herdr": { "local": "/h.sock", "remote": "/rh.sock" }, "bridge": "/b.sock", "deploy": "/d",
        "control": "/c.sock", "proc": "/proc-here", "state": "/state.json",
        "runner": roots.runner_socket(node), "runner_owner": owner,
      })
      .to_string(),
    )
    .unwrap();
  }

  let asked = pc_link(&roots.runner_socket("u7pc"));
  let db = scratch.db().clone();
  let stood = ground_with(db.clone(), home.path(), &variables);
  let port = front(ground).await;
  let seven = client(port, Some(("7", "member")));
  let eight = client(port, Some(("8", "member")));
  let nine = client(port, Some(("9", "member")));

  // Tried before it is kept: from the PC, with the key the person's secret holds; a wrong key is refused by the
  // server, and an address the PC does not reach says so.
  assert_eq!(tested(&seven, None, tried(ADDRESS, Some(KEY))).await, None);
  assert_eq!(tested(&seven, None, tried(ADDRESS, Some("wrong"))).await.as_deref(), Some("refused"));
  assert_eq!(
    tested(&seven, None, tried("http://192.0.2.1:8080/v1/chat/completions", Some(KEY)))
      .await
      .as_deref(),
    Some("unreachable")
  );
  assert_eq!(asked.lock().unwrap()[0], json!({ "owner": "7", "reach": "localhost:11434" }));

  // Kept: a registration person 7 owns, its one address on the PC at the lowest port of the range, its key their
  // secret by name.
  let made = seven.put_personal_model("ollama", &local_ask(ADDRESS, Some(KEY))).await.unwrap();

  assert_eq!(made.model, "registered::u7-ollama");
  assert_eq!((made.key_name.as_deref(), made.enabled), (Some(KEY), true));
  assert_eq!(made.context_tokens, SUPPLY.local_ai.context_tokens);

  let held = agency_server::control::registry::registration(&db, "u7-ollama").await.unwrap().unwrap();

  assert_eq!(held.owner_account, Some(7));
  assert_eq!(held.addresses.len(), 1);
  assert_eq!(held.addresses[0].node, SUPPLY.local_ai.node);
  assert_eq!(held.addresses[0].key_file.as_deref(), Some("users/7/local-secret"));
  assert_eq!(held.addresses[0].reach_port, Some(i64::from(SUPPLY.local_ai.first_port)));

  // Listed as theirs, tried by its key from the PC, and carried by their PC's link.
  let listed = seven.list_personal_models().await.unwrap();

  assert!(listed.allowed);
  assert_eq!(listed.items, vec![made.clone()]);
  assert_eq!(tested(&seven, Some("registered::u7-ollama"), None).await, None);
  assert_eq!(
    agency_server::local_ai::reaches(&db, 7).await.unwrap(),
    vec![node_link::spec::Reach {
      port: SUPPLY.local_ai.first_port,
      to: "localhost:11434".to_owned(),
    }]
  );

  // It carries their runs as personal supply; switched off, it carries none and no link carries it.
  let personal = supply::personal_source(&stood, 7, "registered", "u7-ollama").await.unwrap();

  assert_eq!(personal, Some(supply::Source::Owned));
  seven
    .change_personal_model("ollama", &PersonalModelChange { enabled: false })
    .await
    .unwrap();
  assert_eq!(supply::personal_source(&stood, 7, "registered", "u7-ollama").await.unwrap(), None);
  assert!(agency_server::local_ai::reaches(&db, 7).await.unwrap().is_empty());
  seven
    .change_personal_model("ollama", &PersonalModelChange { enabled: true })
    .await
    .unwrap();

  // Another's local AI is none of theirs; a name out of its rule and an address that is not plain http are refused
  // with their field.
  assert_eq!(tested(&nine, Some("registered::u7-ollama"), None).await.as_deref(), Some("no-source"));
  assert_eq!(refused(nine.delete_personal_model("ollama").await).0, 404);
  assert_eq!(refused(seven.put_personal_model("Bad_Name", &local_ask(ADDRESS, None)).await).0, 400);
  assert_eq!(
    refused(
      seven
        .put_personal_model("other", &local_ask("https://localhost:11434/v1/chat/completions", None))
        .await
    )
    .0,
    400
  );

  // A person with no PC of their own is refused, and their test says so.
  assert_eq!(
    refused(nine.put_personal_model("ollama", &local_ask(ADDRESS, None)).await),
    (409, "no-pc".to_owned())
  );
  assert_eq!(tested(&nine, None, tried(ADDRESS, None)).await.as_deref(), Some("no-pc"));

  // A person whose chain forbids it is refused, their test is withheld, and a local AI of theirs made before the
  // chain closed carries none of their runs, which are told why.
  assert_eq!(
    refused(eight.put_personal_model("ollama", &local_ask(ADDRESS, None)).await),
    (403, "personal-local-ai-withheld".to_owned())
  );
  assert_eq!(tested(&eight, None, tried(ADDRESS, None)).await.as_deref(), Some("withheld"));
  assert!(!eight.list_personal_models().await.unwrap().allowed);
  scratch
    .run(
      r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens", "thinking",
           "walked", "keeps_research", "added_at", "owner_account", "enabled")
         VALUES ('u8-ollama', 'ollama', 'qwen3', 32768, 4096, 0, 0, 0, 1, 8, 1)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "registered_model_addresses" ("registration", "node", "position", "url", "key_owned",
           "role_files", "reach_port")
         VALUES ('u8-ollama', 'pc', 1, $1, 0, 0, $2)"#,
      vec![ADDRESS.into(), (i64::from(SUPPLY.local_ai.first_port) + 1).into()],
    )
    .await;
  assert_eq!(supply::personal_source(&stood, 8, "registered", "u8-ollama").await.unwrap(), None);

  let mut chosen = supply::Chosen {
    run: "turn-1-1".to_owned(),
    account: Some(8),
    provider: "registered".to_owned(),
    model: "u8-ollama".to_owned(),
    source: supply::Source::Personal,
    order: vec![supply::PERSONAL.to_owned()],
    units: Vec::new(),
    step: 1,
  };
  let closed = supply::next_source(&stood, &mut chosen, &[supply::PERSONAL.to_owned()])
    .await
    .unwrap()
    .unwrap_err();

  assert_eq!(closed.why, Shortfall::Withheld);

  // The person's own sources say they hold one that is on, and whether they may add one now: a person with no PC of
  // their own may not.
  let sources = |person: &'static str| async move {
    reqwest::Client::new()
      .get(format!("http://127.0.0.1:{port}/supply-sources"))
      .bearer_auth("secret")
      .header("Owner-Key", person)
      .header("Owner-Role", "member")
      .send()
      .await
      .unwrap()
      .json::<Value>()
      .await
      .unwrap()
  };
  let seven_sources = sources("7").await;

  assert_eq!(
    (seven_sources["ownSupply"].clone(), seven_sources["localAi"].clone()),
    (json!(true), json!(true))
  );
  assert_eq!(sources("9").await["localAi"], json!(false));

  // Off-boarding switches it off; removing it takes its registration away.
  let off = agency_server::control::personal_supply::off_board(&db, &roots.data, 7).await.unwrap();

  assert_eq!(off.local_ais, 1);
  assert!(!seven.list_personal_models().await.unwrap().items[0].enabled);
  seven.delete_personal_model("ollama").await.unwrap();
  assert!(
    agency_server::control::registry::registration(&db, "u7-ollama")
      .await
      .unwrap()
      .is_none()
  );
}

#[test]
fn a_local_ai_s_answer_is_read_into_what_a_person_is_told() {
  use agency_server::control::supply_test::Outcome;
  use agency_server::local_ai::local_outcome;

  assert_eq!(local_outcome(200, "{}"), Outcome::Answers);
  assert!(
    matches!(local_outcome(401, r#"{"error":{"message":"bad key"}}"#), Outcome::Refused(why) if why.contains("bad key"))
  );
  assert!(matches!(local_outcome(403, "forbidden"), Outcome::Refused(_)));
  assert!(
    matches!(local_outcome(404, r#"{"error":"model not found"}"#), Outcome::Failed(why) if why.contains("model not found"))
  );
  assert!(matches!(local_outcome(500, ""), Outcome::Failed(why) if why.contains("500")));
  assert_eq!(Outcome::Withheld.code(), Some("withheld"));
  assert_eq!(Outcome::NoPc.code(), Some("no-pc"));
  assert_eq!(Outcome::Unreachable("x".to_owned()).code(), Some("unreachable"));
}
