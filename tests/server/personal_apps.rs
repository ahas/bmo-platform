//! A person's personal apps put, sent back, removed and listed through agency's server (docs/person-nodes.md,
//! 「Personal apps」): the administrator's app built by the real runner of this machine's node link, its description
//! put through a stand-in of `bmo-pocket` under the version the runner placed, the runner pointed at it, an update
//! moving both, going back to the version before, and a removal taking it out at home and on the machine; a person
//! with no runner, one whose chain withholds personal apps, and one whose app asks for a permission their chain leaves
//! off refused.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use agency_server::personal_apps::{self, Fault};
use agency_server::places::Ground;
use serde_json::{Value, json};

use crate::support::grounds::seeded_with;
use crate::support::rows;
use crate::support::scratch::Scratch;

// A stand-in of `bmo-pocket`'s owner verbs: `put --owner --version` copies the folder into the owner's registry and
// points at it, refusing a manifest that says `refuse-me`; `list --owner --json` and `take --owner` read and take the
// pointers, as the tool host's own verbs do.
const POCKET: &str = r#"#!/usr/bin/env python3
import json, os, shutil, sys, tomllib
args = sys.argv[1:]
root = args[args.index("--data-root") + 1]
verb = args[2]
owner = args[args.index("--owner") + 1]
owned = os.path.join(root, "tool-host", "owners", owner)
pointers = os.path.join(owned, "registry")
if verb == "put":
    version = args[args.index("--version") + 1]
    folder = args[-1]
    text = open(os.path.join(folder, "tool.toml")).read()
    manifest = tomllib.loads(text)
    name = manifest["tool"]["name"]
    if "refuse-me" in text:
        sys.stderr.write("bmo-pocket: %s: it asks for what a personal tool may not\n" % name)
        sys.exit(1)
    placed = os.path.join(owned, "tools", name, version)
    if not os.path.isdir(placed):
        shutil.copytree(folder, placed)
    os.makedirs(pointers, exist_ok=True)
    json.dump({"name": name, "version": version, "dir": placed, "origin": "machine:" + owner, "owner": owner, "putAt": 5}, open(os.path.join(pointers, name + ".json"), "w"))
    print("put %s's %s %s" % (owner, name, version))
elif verb == "list":
    rows = []
    for file in sorted(os.listdir(pointers)) if os.path.isdir(pointers) else []:
        pointer = json.load(open(os.path.join(pointers, file)))
        manifest = tomllib.loads(open(os.path.join(pointer["dir"], "tool.toml")).read())
        entries = [{"name": "my/" + entry["name"], "description": entry["description"]} for entry in manifest.get("entry", [])]
        permissions = manifest.get("permissions", {})
        rows.append(dict(pointer, entries=entries, permissions={"network": permissions.get("network", []), "secrets": permissions.get("secrets", [])}))
    print(json.dumps(rows))
elif verb == "take":
    name = args[-1]
    pointer = os.path.join(pointers, name + ".json")
    if not os.path.exists(pointer):
        sys.stderr.write("bmo-pocket: %s holds no personal tool %s\n" % (owner, name))
        sys.exit(1)
    os.remove(pointer)
    print("took %s's %s" % (owner, name))
"#;

const ADMINISTRATOR: &str = "9";

struct Stood {
  _scratch: Scratch,
  _home: tempfile::TempDir,
  ground: Ground,
}

impl Stood {
  // The app's folder of 'my apps' on this machine, its manifest's description `said` and what `more` adds.
  fn app(&self, name: &str, said: &str, more: &str) -> PathBuf {
    let dir = self.ground.roots.data.join("my-apps").join(name);

    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
      dir.join("tool.toml"),
      format!(
        "[tool]\nname = \"{name}\"\nprotocol = 1\nrun = [\"python3\", \"main.py\"]\n\n[[entry]]\nnamespace = \"my\"\nname = \"{name}\"\ndescription = \"{said}\"\ninput = \"input.json\"\ncapabilities = [\"my.own\"]\n{more}"
      ),
    )
    .unwrap();
    std::fs::write(dir.join("input.json"), r#"{ "type": "object" }"#).unwrap();
    std::fs::write(dir.join("main.py"), "print('{}')").unwrap();
    dir
  }

  fn home_pointer(&self, account: &str, name: &str) -> Option<Value> {
    let file = self
      .ground
      .roots
      .data
      .join("tool-host/owners")
      .join(account)
      .join("registry")
      .join(format!("{name}.json"));

    serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()
  }

  fn pc_pointer(&self, account: &str, name: &str) -> Option<Value> {
    let file = self
      .ground
      .roots
      .data
      .join("personal-tools/owners")
      .join(account)
      .join("registry")
      .join(format!("{name}.json"));

    serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()
  }
}

async fn stood(more: &[(&str, &str)]) -> Stood {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let pocket = home.path().join("fake-pocket");

  std::fs::write(&pocket, POCKET).unwrap();
  std::fs::set_permissions(&pocket, std::fs::Permissions::from_mode(0o755)).unwrap();

  let mut variables =
    BTreeMap::from([(personal_apps::POCKET_VARIABLE.to_owned(), pocket.to_string_lossy().into_owned())]);

  variables.extend(more.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())));

  let ground = seeded_with(scratch.db(), home.path(), &variables).await;

  scratch
    .run(
      &format!(
        r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '{ADMINISTRATOR}')"#
      ),
      vec![],
    )
    .await;

  // The administrator's runner is this deployment's node link, which the test's own build stands in for.
  let bin = ground.roots.deploy.join("bin");

  std::fs::create_dir_all(&bin).unwrap();
  std::os::unix::fs::symlink(crate::support::binary::path("node-link"), bin.join("node-link")).unwrap();

  Stood {
    _scratch: scratch,
    _home: home,
    ground,
  }
}

fn entry_names(entries: &[Value]) -> Vec<&str> {
  entries.iter().filter_map(|entry| entry["name"].as_str()).collect()
}

// "a put builds the app's folder on the runner into a new version, puts its description at home under that version
// and points the runner at it; an update moves both, going back moves both to the version before, the listing names
// what each keeps, and a removal takes it out at home and on the machine"
#[tokio::test]
async fn an_app_is_put_updated_sent_back_listed_and_removed_on_both_sides() {
  let stood = stood(&[]).await;
  let ground = &stood.ground;

  stood.app("weekly", "Sums up the week.", "");

  let first = personal_apps::put(ground, ADMINISTRATOR, "weekly").await.unwrap();
  let version = first.version.clone().unwrap();

  assert_eq!(entry_names(&first.entries), ["my/weekly"]);
  assert!(
    first.said.iter().any(|line| line.contains(&format!("put 9's weekly {version}"))),
    "{first:?}"
  );
  assert_eq!(stood.home_pointer(ADMINISTRATOR, "weekly").unwrap()["version"], version);
  assert_eq!(stood.pc_pointer(ADMINISTRATOR, "weekly").unwrap()["version"], version);

  let copy = ground.roots.data.join("tool-host/owners/9/tools/weekly").join(&version);

  assert!(copy.join("tool.toml").exists());
  assert!(!copy.join("main.py").exists(), "no code reaches home");

  // An update is a new version on both sides.
  tokio::time::sleep(std::time::Duration::from_millis(3)).await;
  stood.app("weekly", "Sums up the week, shorter.", "");

  let second = personal_apps::put(ground, ADMINISTRATOR, "weekly").await.unwrap();
  let newer = second.version.clone().unwrap();

  assert_ne!(newer, version);
  assert_eq!(stood.home_pointer(ADMINISTRATOR, "weekly").unwrap()["version"], newer);
  assert_eq!(stood.pc_pointer(ADMINISTRATOR, "weekly").unwrap()["version"], newer);

  let listed = personal_apps::list(ground, ADMINISTRATOR).await.unwrap();

  assert_eq!(listed["reached"], true, "{listed}");
  assert_eq!(listed["items"][0]["name"], "weekly");
  assert_eq!(listed["items"][0]["kind"], "app");
  assert_eq!(listed["items"][0]["version"], newer);
  assert_eq!(listed["items"][0]["kept"], json!([version]));
  assert_eq!(listed["items"][0]["entries"][0]["description"], "Sums up the week, shorter.");
  assert_eq!(listed["items"][0]["permissions"], json!({ "network": [], "secrets": [] }));

  // Going back takes the newest older version on both sides; none older is left after.
  let back = personal_apps::back(ground, ADMINISTRATOR, "weekly", None).await.unwrap();

  assert_eq!(back.version.as_deref(), Some(version.as_str()));
  assert_eq!(stood.home_pointer(ADMINISTRATOR, "weekly").unwrap()["version"], version);
  assert_eq!(stood.pc_pointer(ADMINISTRATOR, "weekly").unwrap()["version"], version);
  assert_eq!(
    personal_apps::back(ground, ADMINISTRATOR, "weekly", None).await,
    Err(Fault::NoOlder("weekly".to_owned()))
  );

  // A refusal at home leaves both pointers where they stood.
  stood.app("weekly", "Sums up the week.", "# refuse-me\n");

  assert!(
    matches!(personal_apps::put(ground, ADMINISTRATOR, "weekly").await, Err(Fault::Refused(why)) if why.contains("may not"))
  );
  assert_eq!(stood.home_pointer(ADMINISTRATOR, "weekly").unwrap()["version"], version);
  assert_eq!(stood.pc_pointer(ADMINISTRATOR, "weekly").unwrap()["version"], version);

  // A removal takes it out at home and on the machine; its source stands.
  let removed = personal_apps::remove(ground, ADMINISTRATOR, "weekly").await.unwrap();

  assert_eq!(removed.version, None);
  assert!(stood.home_pointer(ADMINISTRATOR, "weekly").is_none());
  assert!(stood.pc_pointer(ADMINISTRATOR, "weekly").is_none());
  assert!(ground.roots.data.join("my-apps/weekly/tool.toml").exists());
  assert!(matches!(
    personal_apps::remove(ground, ADMINISTRATOR, "weekly").await,
    Err(Fault::NotFound(_))
  ));
  assert_eq!(personal_apps::list(ground, ADMINISTRATOR).await.unwrap()["items"], json!([]));
  assert!(matches!(
    personal_apps::put(ground, ADMINISTRATOR, "Weekly").await,
    Err(Fault::BadName(_))
  ));
}

// The account service standing in: `personal-apps.make` off for person 7, every other kind saying nothing.
async fn account_service() -> String {
  let app = axum::Router::new().route(
    "/chain/kinds/{key}/resolve",
    axum::routing::get(
      |axum::extract::Path(key): axum::extract::Path<String>,
       axum::extract::Query(query): axum::extract::Query<BTreeMap<String, String>>| async move {
        let off = key == "personal-apps.make" && query.get("person").map(String::as_str) == Some("7");

        axum::Json(json!({ "value": if off { json!(false) } else { Value::Null } }))
      },
    ),
  );
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  address
}

// A person's PC's runner standing in on its socket: every asking answered with a placed version whose manifest asks
// for the network.
fn reaching_runner(socket: &Path) {
  let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();

  std::thread::spawn(move || {
    use std::io::{BufRead, Write};

    for stream in listener.incoming().flatten() {
      let mut line = String::new();
      let mut reading = std::io::BufReader::new(stream.try_clone().unwrap());

      let _ = reading.read_line(&mut line);

      let manifest = "[tool]\nname = \"reach\"\n\n[permissions]\nnetwork = [\"example.org\"]\n";
      let placed = json!({ "t": "placed", "version": "1-abc", "files": { "tool.toml": manifest } });
      let mut writing = stream;
      let _ = writeln!(writing, "{placed}");
    }
  });
}

// "a person with no runner, one whose chain withholds personal apps, and one whose app asks for a permission their
// chain leaves off are refused, and nothing is put"
#[tokio::test]
async fn a_put_is_refused_where_the_account_has_no_runner_or_its_chain_withholds_it() {
  let address = account_service().await;
  let stood = stood(&[("WEB_ADDRESS", &address), ("WEB_TOKEN", "chain-token")]).await;
  let ground = &stood.ground;
  let db = ground.db.clone();

  rows::supervisor(&db, "u8", &[("owner_account", 8i64.into())]).await;
  assert_eq!(personal_apps::put(ground, "8", "reach").await, Err(Fault::NoRunner("8".to_owned())));
  assert_eq!(personal_apps::put(ground, "7", "reach").await, Err(Fault::Withheld));

  // A PC whose link runs person 8's personal tools, its runner answering an app that asks for the network.
  sea_orm::ConnectionTrait::execute_unprepared(
    &db,
    r#"INSERT INTO "person_nodes" ("owner_account", "node", "shared", "set_at") VALUES ('8', 'u8pc', false, 1)"#,
  )
  .await
  .unwrap();

  let socket = ground.roots.runner_socket("u8pc");
  let spec_path = ground.roots.spec_path("u8pc");

  std::fs::create_dir_all(spec_path.parent().unwrap()).unwrap();
  std::fs::write(
    &spec_path,
    json!({
      "node": "u8pc", "ssh": [], "root": "/pc", "binary": "/pc/deploy/bin/node-link",
      "herdr": { "local": "/h.sock", "remote": "/rh.sock" }, "bridge": "/b.sock", "deploy": "/d",
      "control": "/c.sock", "proc": "/proc-here", "state": "/state.json",
      "runner": socket, "runner_owner": "8",
    })
    .to_string(),
  )
  .unwrap();
  reaching_runner(&socket);

  assert_eq!(
    personal_apps::put(ground, "8", "reach").await,
    Err(Fault::PermissionWithheld("the network".to_owned()))
  );
  assert!(stood.home_pointer("8", "reach").is_none(), "nothing is put at home");
}

// A tool host keeping the record each screen session is issued for, answering it as issued.
fn issuing_host(socket: &Path) -> std::sync::Arc<std::sync::Mutex<Vec<Value>>> {
  let issued = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
  let kept = issued.clone();
  let listener = tokio::net::UnixListener::bind(socket).unwrap();
  let app = axum::Router::new().route(
    "/tool-sessions",
    axum::routing::post(move |axum::Json(mut record): axum::Json<Value>| {
      let kept = kept.clone();

      async move {
        kept.lock().unwrap().push(record.clone());
        record["token"] = json!("screen-token");
        for (field, empty) in [("shown", json!([])), ("variables", json!({})), ("labels", json!({}))] {
          if record.get(field).is_none() {
            record[field] = empty;
          }
        }
        record["createdAt"] = json!(1);
        record["expiresAt"] = json!(2);

        (axum::http::StatusCode::CREATED, axum::Json(record))
      }
    }),
  );

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  issued
}

// "a screen session is issued for the account alone: the namespace `my`, no capability and no grant, the account and
// its runner, the deploy tree's screen kit named to the administrator's runner, a file of the apps the person does
// not reach, lasting ten minutes; an account with no runner is refused 409 `no-runner`"
#[tokio::test]
async fn a_screen_session_acts_for_the_account_alone_and_lapses() {
  let socket_dir = tempfile::tempdir().unwrap();
  let socket = socket_dir.path().join("mcp-host.sock");
  let issued = issuing_host(&socket);
  let named = socket.to_string_lossy().into_owned();
  let stood = stood(&[("BMO_MCP_HOST_SOCKET", &named), ("BMO_MCP_HOST_ISSUER_TOKEN", "issuer")]).await;
  let ground = &stood.ground;
  let before = u64::try_from(agency_server::now_ms()).unwrap();
  let answered = personal_apps::screen_session(ground, ADMINISTRATOR).await.unwrap();
  let record = issued.lock().unwrap().pop().unwrap();
  let lasts = agency_server::lines::token::SCREEN_SESSION_MS;

  assert_eq!(answered["token"], "screen-token");
  assert!(answered["expiresAt"].as_u64().unwrap() >= before + lasts, "{answered}");
  assert_eq!(record["holderName"], format!("personal-screens:{ADMINISTRATOR}"));
  assert_eq!(record["namespaces"], json!(["my"]));
  assert_eq!(record["capabilities"], json!([]));
  assert_eq!(record["grants"], json!([]));
  assert_eq!(record["ownerAccount"], ADMINISTRATOR);
  assert_eq!(record["ttlMs"], json!(lasts));
  assert!(record["withheldAppsFile"].is_string(), "{record}");

  let runner: Vec<String> = serde_json::from_value(record["runner"]["command"].clone()).unwrap();
  let kit = ground.roots.deploy.join(node_link::runner::SCREEN_KIT).to_string_lossy().into_owned();

  assert!(runner.windows(2).any(|two| two == ["--kit".to_owned(), kit.clone()]), "{runner:?}");

  let refused = personal_apps::screen_session(ground, "8").await.unwrap_err();

  assert_eq!(refused.status, axum::http::StatusCode::CONFLICT);
  assert_eq!(refused.code.as_deref(), Some("no-runner"));
}
