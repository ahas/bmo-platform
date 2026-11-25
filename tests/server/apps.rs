//! The apps a person reaches (docs/tools.md, 「The apps a person reaches」): each app's two settings resolved on a
//! stand-in account service for the person a record acts for, the apps not both offered and on written into the
//! record's withheld-apps file before the record is issued, the file resolved again as a turn begins and a change
//! owed to the session, and none withheld for the deployment's own callers or the administrator's.
//!
//! The stand-in answers each resolution as the account service's delegation chain resolves it for the settings the
//! tool host's `put` declares: the company app `ledger` is offered by the platform's rule to one company alone, and
//! the `almanac` app's on setting is locked off by another company over what its person said.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use agency_server::apps::{self, Changed, Personal, Withheld};
use agency_server::person_nodes::Caller;
use axum::extract::{Path, Query, State};
use serde_json::{Value, json};
use tool_call::account_service::AccountService;

use crate::support::grounds::{ground_with, seeded_with};
use crate::support::rows;
use crate::support::scratch::Scratch;

// What the stand-in resolves each setting to for each person, by the setting's key and the person; `true` where
// the table says nothing.
type Table = Arc<Mutex<HashMap<(String, String), bool>>>;

/// The people of the stand-in: one of the company the platform offers its company app to, and one of a company
/// that locked the almanac off though its person had it on.
const COMPANY_PERSON: &str = "7";
const OTHER_PERSON: &str = "8";

fn table() -> Table {
  let held = HashMap::from([
    (("app.ledger.offered".to_owned(), COMPANY_PERSON.to_owned()), true),
    (("app.ledger.offered".to_owned(), OTHER_PERSON.to_owned()), false),
    (("app.almanac.on".to_owned(), OTHER_PERSON.to_owned()), false),
  ]);

  Arc::new(Mutex::new(held))
}

/// The account service standing in on a loopback port of its own, behind the token `chain-token`: the kinds it lists
/// are the two apps' settings and a setting that is no app's.
async fn account_service(table: Table) -> String {
  let kinds = json!({ "kinds": [
    { "key": "app.almanac.offered" }, { "key": "app.almanac.on" },
    { "key": "app.ledger.offered" }, { "key": "app.ledger.on" },
    { "key": "supply.own" },
  ]});
  let app = axum::Router::new()
    .route(
      "/chain/kinds",
      axum::routing::get(move |headers: axum::http::HeaderMap| {
        let kinds = kinds.clone();

        async move {
          match headers.get("authorization").and_then(|value| value.to_str().ok()) {
            Some("Bearer chain-token") => (axum::http::StatusCode::OK, axum::Json(kinds)),
            _ => (axum::http::StatusCode::UNAUTHORIZED, axum::Json(json!({ "code": "unauthorized" }))),
          }
        }
      }),
    )
    .route(
      "/chain/kinds/{key}/resolve",
      axum::routing::get(
        |State(table): State<Table>,
         Path(key): Path<String>,
         Query(asked): Query<HashMap<String, String>>| async move {
          let person = asked.get("person").cloned().unwrap_or_default();
          let value = table.lock().unwrap().get(&(key.clone(), person.clone())).copied().unwrap_or(true);

          axum::Json(json!({ "kind": key, "subject": { "person": person }, "value": value, "layers": [] }))
        },
      ),
    )
    .with_state(table);
  let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  address
}

fn reached(address: &str) -> AccountService {
  AccountService {
    address: address.to_owned(),
    token: "chain-token".to_owned(),
  }
}

fn keys(apps: &[&str]) -> BTreeSet<String> {
  apps.iter().map(|app| (*app).to_owned()).collect()
}

// "A company app visible to that company's person only" and "a lock at the company keeps a person's on from
// applying", as agency reads what the chain resolved: an app reaches a person where both its settings resolve
// true, and only the settings of an app's kinds are asked.
#[tokio::test]
async fn an_app_not_both_offered_and_on_for_the_person_is_withheld() {
  let address = account_service(table()).await;
  let service = reached(&address);

  assert_eq!(apps::resolve(&service, COMPANY_PERSON).await.unwrap(), keys(&[]));
  assert_eq!(apps::resolve(&service, OTHER_PERSON).await.unwrap(), keys(&["almanac", "ledger"]));

  let refused = apps::resolve(
    &AccountService {
      token: "wrong".to_owned(),
      ..service.clone()
    },
    COMPANY_PERSON,
  )
  .await
  .unwrap_err();

  assert!(refused.contains("401"), "{refused}");
  assert!(
    apps::resolve(&AccountService::default(), COMPANY_PERSON)
      .await
      .unwrap_err()
      .contains("WEB_ADDRESS")
  );
}

// "Turning off takes effect from the next turn": the place's file resolved again as a turn begins names the apps
// turned off and on, and a turn that finds nothing changed changes nothing; where the account service cannot be
// asked, the file stands as it was.
#[tokio::test]
async fn a_turn_resolves_its_place_s_apps_again_and_says_what_changed() {
  let table = table();
  let address = account_service(table.clone()).await;
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let more = BTreeMap::from([
    ("WEB_ADDRESS".to_owned(), address.clone()),
    ("WEB_TOKEN".to_owned(), "chain-token".to_owned()),
    (agency_server::personal_apps::POCKET_VARIABLE.to_owned(), empty_registry(home.path())),
  ]);
  let ground = ground_with(scratch.db().clone(), home.path(), &more);
  let place = home.path().join("place");
  let file = apps::place_file(&place);

  apps::write_for(&ground, &file, COMPANY_PERSON, &Default::default()).await;
  assert_eq!(
    apps::read(&file),
    Some(Withheld {
      apps: Some(keys(&[])),
      account: COMPANY_PERSON.to_owned(),
      personal: Personal {
        network: true,
        secrets: true,
      },
      ..Withheld::default()
    })
  );
  assert_eq!(apps::refresh(&ground, &place).await, None, "nothing changed");

  // The company takes the network from its person's personal apps: the file says so from the next turn, and no app
  // changed, so nothing is told.
  table
    .lock()
    .unwrap()
    .insert(("personal-apps.network".to_owned(), COMPANY_PERSON.to_owned()), false);
  assert_eq!(apps::refresh(&ground, &place).await, None);
  assert_eq!(
    apps::read(&file).unwrap().personal,
    Personal {
      network: false,
      secrets: true,
    }
  );

  // The company turns the almanac off for its person between two turns.
  table
    .lock()
    .unwrap()
    .insert(("app.almanac.on".to_owned(), COMPANY_PERSON.to_owned()), false);

  let changed = apps::refresh(&ground, &place).await.unwrap();

  assert_eq!(
    changed,
    Changed {
      off: vec!["almanac".to_owned()],
      on: vec![],
    }
  );
  assert_eq!(apps::read(&file).unwrap().apps, Some(keys(&["almanac"])));
  assert!(apps::changed_fact(&changed).contains("turned off: almanac; turned on: none"));

  let instruction = apps::changed_instruction("u7", 42, &changed);

  assert_eq!(instruction["agentName"], "u7");
  assert_eq!(instruction["room"], 42);
  assert_eq!(instruction["sort"], "notice");
  assert!(instruction["said"]["fact"].as_str().unwrap().contains("almanac"));

  // Back on.
  table
    .lock()
    .unwrap()
    .insert(("app.almanac.on".to_owned(), COMPANY_PERSON.to_owned()), true);
  assert_eq!(apps::refresh(&ground, &place).await.unwrap().on, ["almanac"]);

  // An account service that cannot be asked leaves the file as it stood.
  let gone = ground_with(
    sea_orm::DatabaseConnection::Disconnected,
    home.path(),
    &BTreeMap::from([("WEB_ADDRESS".to_owned(), "http://127.0.0.1:1".to_owned())]),
  );

  table
    .lock()
    .unwrap()
    .insert(("app.almanac.on".to_owned(), COMPANY_PERSON.to_owned()), false);
  assert_eq!(apps::refresh(&gone, &place).await, None);
  assert_eq!(apps::read(&file).unwrap().apps, Some(keys(&[])));

  // A place no record of a person's named has no file, and nothing is resolved for it.
  assert_eq!(apps::refresh(&ground, &home.path().join("elsewhere")).await, None);
}

// A run's file of its own is named by when its record lapses, and the files of lapsed records go as another is
// named.
#[test]
fn a_run_s_own_file_goes_once_its_record_has_lapsed() {
  let data = tempfile::tempdir().unwrap();
  let lapsed = apps::run_file(data.path(), 1_000, 500);

  std::fs::create_dir_all(lapsed.parent().unwrap()).unwrap();
  std::fs::write(&lapsed, "{}").unwrap();

  let standing = apps::run_file(data.path(), 9_000, 2_000);

  std::fs::write(&standing, "{}").unwrap();
  assert!(!lapsed.exists(), "a lapsed record's file is taken away");
  assert_ne!(standing, apps::run_file(data.path(), 9_000, 2_000));
  assert!(standing.exists(), "a standing record's file stays");
}

// A tool host keeping the record each token is issued for, answering it as issued.
fn issuing_host(socket: &std::path::Path) -> Arc<Mutex<Vec<Value>>> {
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
        record["createdAt"] = json!(1);
        record["expiresAt"] = Value::Null;

        (axum::http::StatusCode::CREATED, axum::Json(record))
      }
    }),
  );

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  issued
}

// "A person not offered an app does not see its tools": the record of a person's supervisor names the file its
// withheld apps were written into before it was issued; the deployment's own supervisor and a supervisor of the
// administrator's account name the administrator's file, which withholds their unattached MCP servers alone and
// keeps the network and secrets (docs/tools.md, 「The apps a person reaches」).
#[tokio::test]
async fn a_person_s_record_names_its_withheld_apps_and_the_deployment_s_own_none() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let issued = issuing_host(&socket);
  let address = account_service(table()).await;
  let more = BTreeMap::from([
    ("WEB_ADDRESS".to_owned(), address),
    ("WEB_TOKEN".to_owned(), "chain-token".to_owned()),
    ("BMO_MCP_HOST_SOCKET".to_owned(), socket.to_string_lossy().into_owned()),
    ("BMO_MCP_HOST_ISSUER_TOKEN".to_owned(), "issuer".to_owned()),
    (agency_server::personal_apps::POCKET_VARIABLE.to_owned(), empty_registry(home.path())),
  ]);
  let ground = seeded_with(db, home.path(), &more).await;

  rows::supervisor(db, "u8", &[("owner_account", 8i64.into())]).await;
  rows::supervisor(db, "u9", &[("owner_account", 9i64.into())]).await;
  rows::supervisor(db, "keeper", &[]).await;
  scratch
    .run(r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '9')"#, vec![])
    .await;

  let person = |agent: &'static str| {
    let ground = &ground;

    async move {
      apps::person_of(
        ground,
        Caller {
          agent,
          role: None,
          checkout: None,
        },
      )
      .await
      .unwrap()
    }
  };

  assert_eq!(person("u8").await.as_deref(), Some(OTHER_PERSON));
  assert_eq!(person("u9").await, None, "the administrator's account");
  assert_eq!(person("keeper").await, None, "the deployment's own");

  let place = home.path().join("places").join("u8");
  let issue = |agent: &'static str| {
    let (ground, place) = (&ground, place.clone());

    async move {
      let env = BTreeMap::from([
        ("BMO_TURN_AGENT".to_owned(), agent.to_owned()),
        ("BMO_AGENT_PLACE".to_owned(), place.to_string_lossy().into_owned()),
      ]);

      agency_server::lines::token::issue_for_run(ground, "chat", agent, "/tmp", &env, None, u64::MAX)
        .await
        .unwrap();
    }
  };

  issue("u8").await;

  let record = issued.lock().unwrap().pop().expect("a record was issued");
  let file = apps::place_file(&place);

  assert_eq!(record["withheldAppsFile"], json!(file));
  assert_eq!(apps::read(&file).unwrap().apps, Some(keys(&["almanac", "ledger"])));
  assert_eq!(apps::read(&file).unwrap().account, OTHER_PERSON);

  for agent in ["u9", "keeper"] {
    issue(agent).await;

    let record = issued.lock().unwrap().pop().expect("a record was issued");
    let written = apps::read(&file).unwrap();

    assert_eq!(record["withheldAppsFile"], json!(file), "{agent}: {record}");
    assert_eq!(written.apps, Some(keys(&[])), "{agent}: no MCP server stands");
    assert_eq!(written.account, "9");
    assert!(written.personal.network && written.personal.secrets, "{agent}");
  }
}

// A tool host's registry holding no personal tool of anybody's.
fn empty_registry(home: &std::path::Path) -> String {
  use std::os::unix::fs::PermissionsExt;

  let pocket = home.join("empty-pocket");

  std::fs::write(&pocket, "#!/bin/sh\necho '[]'\n").unwrap();
  std::fs::set_permissions(&pocket, std::fs::Permissions::from_mode(0o755)).unwrap();
  pocket.to_string_lossy().into_owned()
}

// "A record names the account it acts for and the runner its personal tools run on" (docs/tools.md, 「The owner and
// the runner a record names」): a person's on the runner socket of the PC whose link runs that person's personal tools,
// the administrator's and the deployment's own supervisors' as the administrator's on this machine's node link, and a
// caller of no account none.
#[tokio::test]
async fn a_record_names_the_account_it_acts_for_and_the_runner_its_personal_tools_run_on() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let issued = issuing_host(&socket);
  let more = BTreeMap::from([
    ("BMO_MCP_HOST_SOCKET".to_owned(), socket.to_string_lossy().into_owned()),
    ("BMO_MCP_HOST_ISSUER_TOKEN".to_owned(), "issuer".to_owned()),
    (agency_server::personal_apps::POCKET_VARIABLE.to_owned(), empty_registry(home.path())),
  ]);
  let ground = seeded_with(db, home.path(), &more).await;

  rows::supervisor(db, "u8", &[("owner_account", 8i64.into())]).await;
  rows::supervisor(db, "u9", &[("owner_account", 9i64.into())]).await;
  rows::supervisor(db, "keeper", &[]).await;
  scratch
    .run(r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '9')"#, vec![])
    .await;
  scratch
    .run(
      r#"INSERT INTO "person_nodes" ("owner_account", "node", "shared", "set_at") VALUES ('8', 'u8pc', false, 1)"#,
      vec![],
    )
    .await;

  let spec_path = ground.roots.spec_path("u8pc");
  let runner_socket = ground.roots.runner_socket("u8pc");
  let spec = |owner: Option<&str>| {
    json!({
      "node": "u8pc", "ssh": [], "root": "/pc", "binary": "/pc/deploy/bin/node-link",
      "herdr": { "local": "/h.sock", "remote": "/rh.sock" }, "bridge": "/b.sock", "deploy": "/d",
      "control": "/c.sock", "proc": "/proc-here", "state": "/state.json",
      "runner": runner_socket, "runner_owner": owner,
    })
    .to_string()
  };

  std::fs::create_dir_all(spec_path.parent().unwrap()).unwrap();
  std::fs::write(&spec_path, spec(Some("8"))).unwrap();

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

  let person = issue("u8").await;

  assert_eq!(person["ownerAccount"], "8", "{person}");
  assert_eq!(person["runner"], json!({ "socket": runner_socket }), "{person}");
  // Its secrets are read from the person's own space of the secret store.
  assert_eq!(
    person["secretSpace"],
    json!(ground.roots.data.join("env").join("users").join("8")),
    "{person}"
  );

  let administrators = json!({ "command": [
    ground.roots.deploy.join("bin").join("node-link"),
    "runner",
    "--root",
    ground.roots.data.join("personal-tools"),
    "--owner",
    "9",
    "--kit",
    ground.roots.deploy.join("screen-kit"),
  ] });

  for agent in ["u9", "keeper"] {
    let record = issue(agent).await;

    assert_eq!(record["ownerAccount"], "9", "{agent}: {record}");
    assert_eq!(record["runner"], administrators, "{agent}: {record}");
    assert_eq!(record["secretSpace"], json!(ground.roots.data.join("env")), "{agent}: {record}");
  }

  let stray = issue("stray").await;

  assert!(
    stray.get("ownerAccount").is_none()
      && stray.get("runner").is_none()
      && stray.get("secretSpace").is_none(),
    "{stray}"
  );

  // A PC whose link runs no single account's personal tools names its person no runner.
  std::fs::write(&spec_path, spec(None)).unwrap();

  let shared = issue("u8").await;

  assert_eq!(shared["ownerAccount"], "8", "{shared}");
  assert!(shared.get("runner").is_none(), "{shared}");
}

// "A company may choose not to read the platform's base": a record issued for a person whose chain resolves
// `brain.global` false names `BMO_BRAIN_BASE` `off` for the brain's tool (docs/tools.md, 「A tool's folder」); one whose
// chain reads it, and the deployment's own, name none.
#[tokio::test]
async fn a_person_whose_chain_turns_the_base_off_has_their_record_say_so() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let home = tempfile::tempdir().unwrap();
  let socket = home.path().join("mcp-host.sock");
  let issued = issuing_host(&socket);
  let held = table();

  held.lock().unwrap().insert(("brain.global".to_owned(), OTHER_PERSON.to_owned()), false);

  let address = account_service(held).await;
  let more = BTreeMap::from([
    ("WEB_ADDRESS".to_owned(), address),
    ("WEB_TOKEN".to_owned(), "chain-token".to_owned()),
    ("BMO_MCP_HOST_SOCKET".to_owned(), socket.to_string_lossy().into_owned()),
    ("BMO_MCP_HOST_ISSUER_TOKEN".to_owned(), "issuer".to_owned()),
  ]);
  let ground = seeded_with(db, home.path(), &more).await;

  rows::supervisor(db, "u7", &[("owner_account", 7i64.into())]).await;
  rows::supervisor(db, "u8", &[("owner_account", 8i64.into())]).await;
  rows::supervisor(db, "keeper", &[]).await;

  for (agent, said) in [("u8", Some("off")), ("u7", None), ("keeper", None)] {
    let env = BTreeMap::from([("BMO_TURN_AGENT".to_owned(), agent.to_owned())]);

    agency_server::lines::token::issue_for_run(&ground, "chat", agent, "/tmp", &env, None, u64::MAX)
      .await
      .unwrap();

    let record = issued.lock().unwrap().pop().expect("a record was issued");

    assert_eq!(record["variables"]["BMO_BRAIN_BASE"].as_str(), said, "{agent}: {record}");
  }
}
