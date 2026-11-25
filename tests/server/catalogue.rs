//! A person's personal apps shared within their company through agency's server (docs/person-nodes.md, 「Shared
//! apps」): a maker's app shared with their team and heads, its source read off their PC's real runner and kept in a
//! stand-in of `bmo-pocket catalogue`; a target outside the maker's ceiling and one of another company refused; a
//! colleague of the same unit seeing and installing it, their PC receiving the source, building it and putting it
//! home standing for the shared app, their own switch turned on; a colleague of another unit and a person of another
//! company never meeting it; a new version reaching the copy as the copies are kept; a head's recall taking the copy
//! away; and a copy dropped and adopted. The account service stands in with the chain's links as they are written.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use agency_server::catalogue::{self, Asks, Fault, Reader};
use agency_server::personal_apps;
use agency_server::places::Ground;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

use crate::support::grounds::seeded_with;
use crate::support::scratch::Scratch;

// A stand-in of `bmo-pocket`: the owner verbs as the personal apps' tests keep them, `--app` kept on the pointer, and
// the catalogue verbs agency speaks, each record and bundle kept as the tool host's catalogue keeps them.
const POCKET: &str = r#"#!/usr/bin/env python3
import hashlib, json, os, shutil, sys, time, tomllib, urllib.parse
args = sys.argv[1:]
root = args[args.index("--data-root") + 1]
def opt(name, default=None):
    return args[args.index(name) + 1] if name in args else default
def source_hash(files):
    digest = hashlib.sha256()
    for path in sorted(files, key=lambda one: one.encode()):
        data = files[path].encode()
        digest.update(path.encode() + b"\0" + str(len(data)).encode() + b"\0" + data)
    return digest.hexdigest()
if args[2] == "catalogue":
    verb = args[3]
    if verb == "promotions":
        print("[]"); sys.exit(0)
    company = opt("--company")
    kept = os.path.join(root, "tool-host", "catalogue", company)
    os.makedirs(os.path.join(kept, "apps"), exist_ok=True)
    def load(id_):
        file = os.path.join(kept, "apps", id_ + ".json")
        if not os.path.exists(file):
            sys.stderr.write("bmo-pocket: the catalogue of %s holds no app %s\n" % (company, id_)); sys.exit(1)
        return json.load(open(file))
    def save(record):
        json.dump(record, open(os.path.join(kept, "apps", record["id"] + ".json"), "w"))
        print(json.dumps(record))
    positional = [word for at, word in enumerate(args[4:]) if not word.startswith("--") and not args[4 + at - 1].startswith("--")]
    if verb == "list":
        apps = [json.load(open(os.path.join(kept, "apps", file))) for file in sorted(os.listdir(os.path.join(kept, "apps")))]
        seen = os.path.join(kept, "seen", (opt("--reader") or "") + ".json")
        print(json.dumps({"apps": apps, "seenAt": json.load(open(seen))["at"] if os.path.exists(seen) else None}))
    elif verb == "show":
        print(json.dumps(load(positional[0])))
    elif verb == "share":
        bundle = json.load(open(opt("--bundle")))
        files = bundle["files"]
        roots = [root for root in ("tool.toml", "SKILL.md", "mcp.toml") if root in files]
        if len(roots) != 1:
            sys.stderr.write("bmo-pocket: the bundle names no kind, or several\n"); sys.exit(1)
        if roots[0] == "tool.toml":
            manifest = tomllib.loads(files["tool.toml"])
            kind, name, title, description = "app", manifest["tool"]["name"], manifest["tool"]["name"], manifest["entry"][0]["description"]
            permissions = manifest.get("permissions", {})
        elif roots[0] == "mcp.toml":
            manifest = tomllib.loads(files["mcp.toml"])
            server = manifest["server"]
            kind, name, title, description = "mcp", server["name"], server["title"], server["description"]
            permissions = {key: list(value) for key, value in manifest.get("permissions", {}).items()}
            if server["transport"] == "remote":
                permissions.setdefault("network", []).append(urllib.parse.urlparse(server["url"]).netloc)
                if server.get("secret"):
                    permissions.setdefault("secrets", []).append(server["secret"])
        else:
            front = dict(line.split(":", 1) for line in files["SKILL.md"].split("---")[1].strip().splitlines())
            kind, name, title, description = "skill", front["name"].strip(), front["name"].strip(), front["description"].strip()
            permissions = {}
        language = opt("--language") or "en"
        maker = opt("--maker")
        id_ = maker + "-" + name
        file = os.path.join(kept, "apps", id_ + ".json")
        record = json.load(open(file)) if os.path.exists(file) else {"id": id_, "kind": kind, "key": "shared-%s-%s-%s" % (company, maker, name.replace("_", "-")), "name": name, "maker": maker, "owner": maker, "unit": opt("--unit"), "targets": [], "versions": [], "withdrawn": None, "installs": {}, "promotion": None, "sharedAt": 1, "updatedAt": 1, "titles": {}, "descriptions": {}, "machine": []}
        record.update({"title": title, "description": description, "language": language})
        record["titles"][language] = title
        record["descriptions"][language] = description
        version = opt("--version")
        os.makedirs(os.path.join(kept, "sources", id_), exist_ok=True)
        json.dump(bundle, open(os.path.join(kept, "sources", id_, version + ".json"), "w"))
        record["targets"] = json.loads(opt("--targets"))
        record["withdrawn"] = None
        record["versions"].append({"version": version, "sourceHash": source_hash(bundle["files"]), "permissions": {"network": permissions.get("network", []), "secrets": permissions.get("secrets", [])}, "sharedAt": int(time.time() * 1000), "review": {"state": "pending"}, "approval": {"required": "--review-required" in args, "by": None}})
        save(record)
    elif verb == "source":
        record = load(positional[0])
        version = opt("--version") or record["versions"][-1]["version"]
        shared = [one for one in record["versions"] if one["version"] == version][0]
        files = json.load(open(os.path.join(kept, "sources", record["id"], version + ".json")))["files"]
        print(json.dumps({"version": version, "sourceHash": shared["sourceHash"], "files": files}))
    elif verb == "install":
        record = load(positional[0])
        record["installs"][opt("--account")] = {"version": opt("--version"), "how": opt("--how"), "at": 1}
        save(record)
    elif verb == "uninstall":
        record = load(positional[0])
        record["installs"].pop(opt("--account"), None)
        save(record)
    elif verb == "withdraw":
        record = load(positional[0]); record["withdrawn"] = 5; save(record)
    elif verb == "review":
        record = load(positional[0])
        for one in record["versions"]:
            if one["version"] == opt("--version"):
                one["review"] = {"state": opt("--state"), "report": open(opt("--report")).read(), "at": 1}
        save(record)
    elif verb == "text":
        record = load(positional[0])
        locale = opt("--locale")
        if "--clear" in args:
            record["titles"].pop(locale, None); record["descriptions"].pop(locale, None)
        else:
            record["titles"][locale] = opt("--title"); record["descriptions"][locale] = opt("--description")
        record["machine"] = [one for one in record["machine"] if one != locale] + ([locale] if "--machine" in args else [])
        save(record)
    elif verb == "seen":
        os.makedirs(os.path.join(kept, "seen"), exist_ok=True)
        json.dump({"at": 7}, open(os.path.join(kept, "seen", opt("--account") + ".json"), "w"))
        print(json.dumps({"at": 7}))
    else:
        sys.stderr.write("bmo-pocket: no verb %s here\n" % verb); sys.exit(2)
    sys.exit(0)
verb = args[2]
owner = opt("--owner")
owned = os.path.join(root, "tool-host", "owners", owner)
pointers = os.path.join(owned, "registry")
if verb == "put":
    version = opt("--version")
    folder = args[-1]
    manifest = tomllib.loads(open(os.path.join(folder, "tool.toml")).read())
    name = manifest["tool"]["name"]
    placed = os.path.join(owned, "tools", name, version)
    if not os.path.isdir(placed):
        shutil.copytree(folder, placed)
    os.makedirs(pointers, exist_ok=True)
    pointer = {"name": name, "version": version, "dir": placed, "origin": "machine:" + owner, "owner": owner, "putAt": 5}
    if opt("--app"):
        pointer["app"] = opt("--app")
    json.dump(pointer, open(os.path.join(pointers, name + ".json"), "w"))
    print("put %s's %s %s" % (owner, name, version))
elif verb == "list":
    rows = []
    for file in sorted(os.listdir(pointers)) if os.path.isdir(pointers) else []:
        pointer = json.load(open(os.path.join(pointers, file)))
        manifest = tomllib.loads(open(os.path.join(pointer["dir"], "tool.toml")).read())
        entries = [{"name": "my/" + entry["name"], "description": entry["description"]} for entry in manifest.get("entry", [])]
        rows.append(dict(pointer, entries=entries, permissions={"network": [], "secrets": []}))
    print(json.dumps(rows))
elif verb == "take":
    name = args[-1]
    pointer = os.path.join(pointers, name + ".json")
    if not os.path.exists(pointer):
        sys.stderr.write("bmo-pocket: %s holds no personal tool %s\n" % (owner, name)); sys.exit(1)
    os.remove(pointer)
    print("took %s's %s" % (owner, name))
"#;

const ADMINISTRATOR: &str = "1";
const MAKER: i64 = 17;
const TEAMMATE: i64 = 21;
const NEIGHBOUR: i64 = 31;
const STRANGER: i64 = 51;
const HEAD: i64 = 9;

// The account service standing in: people's paths, the head of unit 40, and the chain's links as they are written.
#[derive(Clone, Default)]
struct Chain {
  links: Arc<Mutex<BTreeMap<(String, String), Value>>>,
  locks: Arc<Mutex<BTreeMap<(String, String), String>>>,
  rules: Arc<Mutex<BTreeMap<String, Value>>>,
}

fn units_of(person: i64) -> Vec<i64> {
  match person {
    MAKER | TEAMMATE | HEAD => vec![3, 40],
    NEIGHBOUR => vec![3, 41],
    _ => vec![5, 50],
  }
}

fn path_of(units: &[i64], person: Option<i64>) -> Vec<String> {
  std::iter::once("platform".to_owned())
    .chain(units.iter().map(|unit| format!("unit:{unit}")))
    .chain(person.map(|person| format!("person:{person}")))
    .collect()
}

impl Chain {
  fn said(&self, key: &str, link: &str) -> Option<Value> {
    self.links.lock().unwrap().get(&(key.to_owned(), link.to_owned())).cloned()
  }

  fn locked_above(&self, key: &str, path: &[String]) -> bool {
    let locks = self.locks.lock().unwrap();

    path
      .iter()
      .any(|link| locks.get(&(key.to_owned(), link.clone())).is_some_and(|one| one == "lock"))
  }

  // A kind resolved down a path: an offered flag narrows, every other kind takes its nearest word.
  fn resolve(&self, key: &str, path: &[String]) -> Value {
    if key == "personal-apps.share-scope" {
      return json!({ "value": ["team", "heads"], "path": path });
    }

    if key.ends_with(".offered") {
      let company = path.get(1).cloned().unwrap_or_default();
      let offered = self.rules.lock().unwrap().get(key).and_then(Value::as_array).is_some_and(|rules| {
        rules.iter().any(|rule| rule["who"] == json!(company) && rule["value"] == true)
      });
      let off = path.iter().skip(1).any(|link| self.said(key, link) == Some(json!(false)));

      return json!({ "value": offered && !off, "path": path });
    }

    let nearest = path.iter().rev().find_map(|link| {
      self.said(key, link).filter(|value| !value.is_null()).map(|value| (link.clone(), value))
    });

    match nearest {
      Some((from, value)) => json!({ "value": value, "from": from, "path": path }),
      None => json!({ "value": null, "path": path }),
    }
  }

  // Whether the caller writes the link: the head of unit 40 its link, and a person their own.
  fn writes(&self, caller: i64, link: &str) -> bool {
    link == format!("person:{caller}") || (caller == HEAD && link == "unit:40")
  }
}

fn caller(headers: &HeaderMap) -> i64 {
  headers.get("owner-key").and_then(|key| key.to_str().ok()?.parse().ok()).unwrap_or(0)
}

async fn account_service(chain: Chain) -> String {
  use axum::routing::{get, put};

  let app = axum::Router::new()
    .route(
      "/chain/kinds/{key}/resolve",
      get(|State(chain): State<Chain>, UrlPath(key): UrlPath<String>, Query(query): Query<BTreeMap<String, String>>| async move {
        let path = match (query.get("person"), query.get("unit")) {
          (Some(person), _) => {
            let person: i64 = person.parse().unwrap();

            path_of(&units_of(person), Some(person))
          }
          (_, Some(unit)) => {
            let unit: i64 = unit.parse().unwrap();

            path_of(&if unit == 41 { vec![3, 41] } else if unit / 10 == 5 { vec![5, unit] } else { vec![3, unit] }, None)
          }
          _ => Vec::new(),
        };

        axum::Json(chain.resolve(&key, &path))
      }),
    )
    .route(
      "/chain/kinds/{key}",
      put(|| async { axum::Json(json!({})) }),
    )
    .route(
      "/chain/kinds/{key}/links/{link}",
      get(|State(chain): State<Chain>, UrlPath((key, link)): UrlPath<(String, String)>, headers: HeaderMap| async move {
        let caller = caller(&headers);

        if !chain.writes(caller, &link) {
          return (StatusCode::FORBIDDEN, axum::Json(json!({ "code": "forbidden" })));
        }

        let above = path_of(&units_of(caller), None);
        let writable = !link.starts_with("person:") || !chain.locked_above(&key, &above);

        (StatusCode::OK, axum::Json(json!({ "link": link, "writable": writable })))
      }),
    )
    .route(
      "/chain/kinds/{key}/links/{link}/value",
      put(|State(chain): State<Chain>, UrlPath((key, link)): UrlPath<(String, String)>, headers: HeaderMap, axum::Json(body): axum::Json<Value>| async move {
        let caller = caller(&headers);

        if !chain.writes(caller, &link) {
          return (StatusCode::FORBIDDEN, axum::Json(json!({ "code": "forbidden" })));
        }

        if link.starts_with("person:") && chain.locked_above(&key, &path_of(&units_of(caller), None)) {
          return (StatusCode::CONFLICT, axum::Json(json!({ "code": "locked" })));
        }

        chain.links.lock().unwrap().insert((key, link), body["value"].clone());
        (StatusCode::OK, axum::Json(json!({})))
      }),
    )
    .route(
      "/chain/kinds/{key}/links/{link}/delegation",
      put(|State(chain): State<Chain>, UrlPath((key, link)): UrlPath<(String, String)>, headers: HeaderMap, axum::Json(body): axum::Json<Value>| async move {
        if !chain.writes(caller(&headers), &link) {
          return (StatusCode::FORBIDDEN, axum::Json(json!({ "code": "forbidden" })));
        }

        chain.locks.lock().unwrap().insert((key, link), body["delegation"].as_str().unwrap_or_default().to_owned());
        (StatusCode::OK, axum::Json(json!({})))
      }),
    )
    .route(
      "/chain/kinds/{key}/links/platform/rules",
      put(|State(chain): State<Chain>, UrlPath(key): UrlPath<String>, headers: HeaderMap, axum::Json(body): axum::Json<Value>| async move {
        if caller(&headers).to_string() != ADMINISTRATOR {
          return (StatusCode::FORBIDDEN, axum::Json(json!({ "code": "forbidden" })));
        }

        chain.rules.lock().unwrap().insert(key, body["rules"].clone());
        (StatusCode::OK, axum::Json(json!({})))
      }),
    )
    .route(
      "/units/{unit}",
      get(|UrlPath(unit): UrlPath<i64>| async move { axum::Json(json!({ "id": unit, "heads": [HEAD], "left": [], "path": [] })) }),
    )
    .route(
      "/units/{unit}/subtree",
      get(|| async {
        axum::Json(json!({ "units": [{ "id": 40, "heads": [{ "user": HEAD }], "members": [{ "user": MAKER }, { "user": TEAMMATE }] }] }))
      }),
    )
    .with_state(chain);
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  address
}

struct Stood {
  _scratch: Scratch,
  home: tempfile::TempDir,
  ground: Arc<Ground>,
  chain: Chain,
}

impl Stood {
  // A person's PC: its own data root, holding 'my apps' and its runner's root, its runner answering on the link's
  // runner socket by the real runner of this build.
  async fn pc(&self, person: i64) -> PathBuf {
    let node = format!("u{person}pc");
    let data = self.home.path().join(format!("pc-{person}"));
    let root = data.join(node_link::runner::PERSONAL_TOOLS);
    let socket = self.ground.roots.runner_socket(&node);
    let spec_path = self.ground.roots.spec_path(&node);

    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(spec_path.parent().unwrap()).unwrap();
    sea_orm::ConnectionTrait::execute_unprepared(
      &self.ground.db,
      &format!(
        r#"INSERT INTO "person_nodes" ("owner_account", "node", "shared", "set_at") VALUES ('{person}', '{node}', false, 1)"#
      ),
    )
    .await
    .unwrap();
    std::fs::write(
      &spec_path,
      json!({
        "node": node, "ssh": [], "root": "/pc", "binary": "/pc/deploy/bin/node-link",
        "herdr": { "local": "/h.sock", "remote": "/rh.sock" }, "bridge": "/b.sock", "deploy": "/d",
        "control": "/c.sock", "proc": "/proc-here", "state": "/state.json",
        "runner": socket, "runner_owner": person.to_string(),
      })
      .to_string(),
    )
    .unwrap();
    serve_runner(&socket, &root, &person.to_string());
    data
  }
}

// The real runner of this build, started for each connection on the socket, the connection its passage.
fn serve_runner(socket: &Path, root: &Path, owner: &str) {
  let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
  let (root, owner) = (root.to_path_buf(), owner.to_owned());

  std::thread::spawn(move || {
    for stream in listener.incoming().flatten() {
      let input = stream.try_clone().unwrap();
      let mut child = std::process::Command::new(crate::support::binary::path("node-link"))
        .args(["runner", "--root"])
        .arg(&root)
        .args(["--owner", &owner])
        .stdin(std::process::Stdio::from(std::os::fd::OwnedFd::from(input)))
        .stdout(std::process::Stdio::from(std::os::fd::OwnedFd::from(stream)))
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

      std::thread::spawn(move || {
        let _ = child.wait();
      });
    }
  });
}

// The maker's app in their PC's 'my apps', what it does `said`.
fn app(pc: &Path, name: &str, said: &str) {
  let dir = pc.join(node_link::runner::MY_APPS).join(name);

  std::fs::create_dir_all(&dir).unwrap();
  std::fs::write(
    dir.join("tool.toml"),
    format!(
      "[tool]\nname = \"{name}\"\nprotocol = 1\nrun = [\"python3\", \"main.py\"]\n\n[[entry]]\nnamespace = \"my\"\nname = \"{name}\"\ndescription = \"{said}\"\ninput = \"input.json\"\ncapabilities = [\"my.own\"]\n"
    ),
  )
  .unwrap();
  std::fs::write(dir.join("input.json"), r#"{ "type": "object" }"#).unwrap();
  std::fs::write(dir.join("main.py"), "print('{}')").unwrap();
}

async fn stood() -> Stood {
  let chain = Chain::default();
  let address = account_service(chain.clone()).await;
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let pocket = home.path().join("fake-pocket");

  std::fs::write(&pocket, POCKET).unwrap();
  std::fs::set_permissions(&pocket, std::fs::Permissions::from_mode(0o755)).unwrap();

  let variables = BTreeMap::from([
    (personal_apps::POCKET_VARIABLE.to_owned(), pocket.to_string_lossy().into_owned()),
    ("WEB_ADDRESS".to_owned(), address),
    ("WEB_TOKEN".to_owned(), "chain-token".to_owned()),
  ]);
  let ground = seeded_with(scratch.db(), home.path(), &variables).await;

  scratch
    .run(
      &format!(
        r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '{ADMINISTRATOR}')"#
      ),
      vec![],
    )
    .await;

  Stood {
    _scratch: scratch,
    home,
    ground: Arc::new(ground),
    chain,
  }
}

async fn reader(stood: &Stood, person: i64) -> Reader {
  catalogue::reader(&stood.ground, &person.to_string(), None).await.unwrap()
}

fn home_pointer(stood: &Stood, account: i64, name: &str) -> Option<Value> {
  let file = stood.ground.roots.data.join(format!("tool-host/owners/{account}/registry/{name}.json"));

  serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()
}

#[test]
fn a_review_is_handed_what_the_manifest_asks_and_every_file_under_its_path() {
  let input = catalogue::review_input(
    "app",
    &Asks {
      network: vec!["rates.example.org".to_owned()],
      secrets: Vec::new(),
    },
    &json!({ "main.py": "print('{}')", "tool.toml": "[tool]" }),
  );

  assert!(input.contains("the network at: rates.example.org."));
  assert!(input.contains("the secrets: none."));
  assert!(input.contains("=== main.py ===\nprint('{}')"));
  assert!(input.find("=== main.py ===") < input.find("=== tool.toml ==="));
  assert!(
    Asks {
      network: vec!["a".to_owned()],
      secrets: Vec::new()
    }
    .grows_over(&Asks::default())
  );
  assert!(!Asks::default().grows_over(&Asks {
    network: vec!["a".to_owned()],
    secrets: Vec::new()
  }));
}

#[tokio::test]
async fn an_app_is_shared_seen_installed_updated_and_recalled_within_the_company_alone() {
  let stood = stood().await;
  let ground = &stood.ground;
  let maker_pc = stood.pc(MAKER).await;
  let teammate_pc = stood.pc(TEAMMATE).await;

  app(&maker_pc, "weekly", "Sums up the week.");

  // The maker's app is placed and pointed at on their PC first, as their BMO's put does.
  let put = personal_apps::put(ground, &MAKER.to_string(), "weekly").await.unwrap();
  let first = put.version.clone().unwrap();
  let maker = reader(&stood, MAKER).await;

  // A target beyond the maker's ceiling, and one of another company, are refused.
  assert!(matches!(
    catalogue::share(ground, &maker, "weekly", &[json!({ "company": true })], (None, None)).await,
    Err(Fault::ScopeWithheld(scope)) if scope == "company"
  ));
  assert!(matches!(
    catalogue::share(ground, &maker, "weekly", &[json!({ "person": STRANGER })], (None, None)).await,
    Err(Fault::OtherCompany(_))
  ));

  let shared = catalogue::share(ground, &maker, "weekly", &[], (None, None)).await.unwrap();

  // It is offered to the maker's company alone, by the platform's rule naming it.
  assert_eq!(
    stood.chain.rules.lock().unwrap().get("app.shared-3-17-weekly.offered"),
    Some(&json!([{ "who": "unit:3", "value": true }]))
  );
  assert_eq!(shared.id, "17-weekly");
  assert_eq!(shared.version, first);
  assert_eq!(shared.targets, vec![json!({ "team": true }), json!({ "heads": true })]);
  assert!(!shared.may.install, "a maker installs no copy of their own app");

  // A teammate sees it and installs it; a colleague of another unit and a stranger never meet it.
  let teammate = reader(&stood, TEAMMATE).await;
  let listed = catalogue::list(ground, &teammate).await.unwrap();

  assert_eq!(listed.apps.len(), 1);
  assert!(listed.apps[0].may.install);
  assert_eq!(listed.notices[0].kind, "shared");
  assert!(catalogue::list(ground, &reader(&stood, NEIGHBOUR).await).await.unwrap().apps.is_empty());
  assert!(catalogue::list(ground, &reader(&stood, STRANGER).await).await.unwrap().apps.is_empty());
  assert!(matches!(
    catalogue::install(ground, &reader(&stood, NEIGHBOUR).await, "17-weekly").await,
    Err(Fault::NotFound(_))
  ));

  let installed = catalogue::install(ground, &teammate, "17-weekly").await.unwrap();

  assert_eq!(installed.installed.as_ref().unwrap().version, first);
  assert_eq!(installed.installed.as_ref().unwrap().how, "self");
  assert!(
    teammate_pc.join("my-apps/weekly/main.py").exists(),
    "the source reaches the teammate's PC"
  );
  assert_eq!(home_pointer(&stood, TEAMMATE, "weekly").unwrap()["app"], "shared-3-17-weekly");
  assert_eq!(stood.chain.said("app.shared-3-17-weekly.on", "person:21"), Some(json!(true)));

  // A new version asking for nothing more reaches the copy as the copies are kept.
  tokio::time::sleep(std::time::Duration::from_millis(3)).await;
  app(&maker_pc, "weekly", "Sums up the week, shorter.");

  let second = personal_apps::put(ground, &MAKER.to_string(), "weekly").await.unwrap().version.unwrap();

  catalogue::share(ground, &maker, "weekly", &[], (None, None)).await.unwrap();
  catalogue::keep(ground, &TEAMMATE.to_string()).await.unwrap();

  let moved = catalogue::entry(ground, &teammate, "17-weekly").await.unwrap();

  assert_eq!(moved.installed.unwrap().version, second);

  // The head of the unit recalls it there: the copy is taken away as the teammate's copies are kept.
  let head = reader(&stood, HEAD).await;
  let headed = catalogue::entry(ground, &head, "17-weekly").await.unwrap();

  assert_eq!(headed.may.recall, vec!["unit:40".to_owned()]);
  assert!(matches!(
    catalogue::recall(ground, &teammate, "17-weekly", Some(40)).await,
    Err(Fault::Forbidden(_))
  ));
  catalogue::recall(ground, &head, "17-weekly", Some(40)).await.unwrap();
  assert_eq!(stood.chain.said("app.shared-3-17-weekly.offered", "unit:40"), Some(json!(false)));
  catalogue::keep(ground, &TEAMMATE.to_string()).await.unwrap();
  assert!(home_pointer(&stood, TEAMMATE, "weekly").is_none(), "the recalled copy is taken away");
  assert!(catalogue::entry(ground, &maker, "17-weekly").await.unwrap().installs == 0);
}

#[tokio::test]
async fn a_head_turns_an_app_on_for_their_unit_and_a_copy_is_dropped_or_adopted() {
  let stood = stood().await;
  let ground = &stood.ground;
  let maker_pc = stood.pc(MAKER).await;
  let teammate_pc = stood.pc(TEAMMATE).await;

  app(&maker_pc, "tally", "Counts the days.");
  personal_apps::put(ground, &MAKER.to_string(), "tally").await.unwrap();

  let maker = reader(&stood, MAKER).await;

  catalogue::share(ground, &maker, "tally", &[], (None, None)).await.unwrap();

  // The head turns it on for unit 40, locked: the teammate's copy is installed as their copies are kept, and they may
  // neither drop it nor adopt it.
  let head = reader(&stood, HEAD).await;

  catalogue::switch_unit(ground, &head, "17-tally", 40, true, true).await.unwrap();
  catalogue::keep(ground, &TEAMMATE.to_string()).await.unwrap();

  let teammate = reader(&stood, TEAMMATE).await;
  let copy = catalogue::entry(ground, &teammate, "17-tally").await.unwrap();

  assert_eq!(copy.installed.as_ref().unwrap().how, "unit");
  assert!(matches!(
    catalogue::uninstall(ground, &teammate, "17-tally").await,
    Err(Fault::Locked(_))
  ));

  // Unlocked, the teammate adopts the copy as their own: it no longer stands for the shared app.
  catalogue::switch_unit(ground, &head, "17-tally", 40, true, false).await.unwrap();

  let adopted = catalogue::adopt(ground, &teammate, "17-tally").await.unwrap();

  assert!(adopted.installed.is_none());
  assert!(home_pointer(&stood, TEAMMATE, "tally").unwrap().get("app").is_none());
  assert!(!teammate_pc.join("my-apps/tally").join(node_link::keeping::RECEIVED_MARK).exists());
  assert_eq!(stood.chain.said("app.shared-3-17-tally.on", "person:21"), Some(json!(false)));

  // Kept again, the adopted app is the teammate's own and nothing is installed over it.
  catalogue::keep(ground, &TEAMMATE.to_string()).await.unwrap();
  assert!(catalogue::entry(ground, &teammate, "17-tally").await.unwrap().installed.is_none());

  // A withdrawn app is installed by nobody new.
  assert!(matches!(
    catalogue::withdraw(ground, &teammate, "17-tally").await,
    Err(Fault::Forbidden(_))
  ));
  catalogue::withdraw(ground, &maker, "17-tally").await.unwrap();
  assert!(matches!(
    catalogue::install(ground, &teammate, "17-tally").await,
    Err(Fault::Withdrawn(_))
  ));
}

// "the administrator is a member of their own company's catalogue: they install a copy there on the runner their
// records name, as any member does, and keep recalling at the platform; in another company they install nothing"
#[tokio::test]
async fn the_administrator_installs_in_their_own_company_alone() {
  let stood = stood().await;
  let ground = &stood.ground;
  let own_pc = stood.pc(STRANGER).await;
  let other_pc = stood.pc(MAKER).await;
  // The administrator's runner is this deployment's node link, standing in the deploy tree.
  let bin = ground.roots.deploy.join("bin");

  std::fs::create_dir_all(&bin).unwrap();
  std::os::unix::fs::symlink(crate::support::binary::path("node-link"), bin.join("node-link")).unwrap();

  // An app of the administrator's own company, and one of another.
  app(&own_pc, "daily", "Says the day.");
  personal_apps::put(ground, &STRANGER.to_string(), "daily").await.unwrap();
  catalogue::share(ground, &reader(&stood, STRANGER).await, "daily", &[], (None, None))
    .await
    .unwrap();
  app(&other_pc, "weekly", "Sums up the week.");
  personal_apps::put(ground, &MAKER.to_string(), "weekly").await.unwrap();
  catalogue::share(ground, &reader(&stood, MAKER).await, "weekly", &[], (None, None))
    .await
    .unwrap();

  let own = catalogue::reader(ground, ADMINISTRATOR, Some(5)).await.unwrap();
  let other = catalogue::reader(ground, ADMINISTRATOR, Some(3)).await.unwrap();

  assert!(own.member().is_some(), "their home unit stands in company 5");
  assert!(other.member().is_none());

  // In another company: every app seen and recalled at the platform, none installed.
  let weekly = catalogue::entry(ground, &other, "17-weekly").await.unwrap();

  assert!(!weekly.may.install);
  assert_eq!(weekly.may.recall, vec!["platform".to_owned()]);
  assert!(matches!(
    catalogue::install(ground, &other, "17-weekly").await,
    Err(Fault::Forbidden(_))
  ));

  // In their own company: installed as a member, received into their own 'my apps' and put home for them.
  let daily = catalogue::entry(ground, &own, "51-daily").await.unwrap();

  assert!(daily.may.install);
  assert_eq!(daily.may.recall, vec!["platform".to_owned()]);

  let installed = catalogue::install(ground, &own, "51-daily").await.unwrap();

  assert_eq!(installed.installed.as_ref().unwrap().how, "self");
  assert!(installed.may.uninstall && installed.may.clone);
  assert!(
    ground.roots.data.join("my-apps/daily/main.py").exists(),
    "the source reaches the administrator's runner"
  );
  assert_eq!(home_pointer(&stood, 1, "daily").unwrap()["app"], "shared-5-51-daily");
}

// A stand-in of a PC's runner holding one MCP server, `tracker`, remote at `mcp.example.com`, signing in with the
// person's secret `tracker_token`: it answers each ask as the runner's passage does, a place that carries no secret
// with what it wants, and writes every ask down.
const TRACKER: &str = "[server]\nname = \"tracker\"\ntitle = \"Tracker\"\ndescription = \"Reads the team's issues.\"\ntransport = \"remote\"\nurl = \"https://mcp.example.com/mcp\"\nauth = \"bearer\"\nsecret = \"tracker_token\"\n";

const TRACKER_MANIFEST: &str = "[tool]\nname = \"tracker\"\nprotocol = 1\nrun = [\"/.bmo/node-link\", \"mcp\"]\n\n[[entry]]\nnamespace = \"my\"\nname = \"list_issues\"\ndescription = \"Lists the issues.\"\ninput = \"schemas/list_issues.json\"\ncapabilities = [\"my.own\"]\n\n[permissions]\nnetwork = [\"mcp.example.com\"]\nsecrets = [\"tracker_token\"]\n";

type RunnerAsks = Arc<Mutex<Vec<Value>>>;

fn serve_mcp_runner(socket: &Path, asks: RunnerAsks) {
  use std::io::{BufRead, Write};

  let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();

  std::thread::spawn(move || {
    for stream in listener.incoming().flatten() {
      let mut line = String::new();

      if std::io::BufReader::new(&stream).read_line(&mut line).is_err() {
        continue;
      }

      let asked: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
      let answer = if asked.get("versions").is_some() {
        json!({ "t": "versions", "tools": { "tracker": { "current": "v1", "kept": [] } } })
      } else if asked.get("source").is_some() {
        json!({ "t": "source", "version": "v1", "files": { "mcp.toml": TRACKER } })
      } else if asked.get("receive").is_some() {
        json!({ "t": "received" })
      } else if asked["place"].get("secrets").is_some() {
        json!({ "t": "placed", "version": "v1", "files": {
          "tool.toml": TRACKER_MANIFEST, "schemas/list_issues.json": "{\"type\": \"object\"}"
        } })
      } else if asked.get("place").is_some() {
        json!({ "t": "wants", "network": ["mcp.example.com"], "secrets": ["tracker_token"] })
      } else if asked.get("point").is_some() {
        json!({ "t": "pointed", "version": "v1", "kept": [] })
      } else if asked.get("adopt").is_some() {
        json!({ "t": "adopted" })
      } else {
        json!({ "t": "taken" })
      };

      asks.lock().unwrap().push(asked);

      let mut stream = stream;
      let _ = writeln!(stream, "{answer}");
    }
  });
}

impl Stood {
  // A person's PC whose runner is the stand-in holding `tracker`, the asks it was asked written down.
  async fn mcp_pc(&self, person: i64) -> RunnerAsks {
    let node = format!("u{person}pc");
    let socket = self.ground.roots.runner_socket(&node);
    let spec_path = self.ground.roots.spec_path(&node);
    let asks = RunnerAsks::default();

    std::fs::create_dir_all(spec_path.parent().unwrap()).unwrap();
    sea_orm::ConnectionTrait::execute_unprepared(
      &self.ground.db,
      &format!(
        r#"INSERT INTO "person_nodes" ("owner_account", "node", "shared", "set_at") VALUES ('{person}', '{node}', false, 1)"#
      ),
    )
    .await
    .unwrap();
    std::fs::write(
      &spec_path,
      json!({
        "node": node, "ssh": [], "root": "/pc", "binary": "/pc/deploy/bin/node-link",
        "herdr": { "local": "/h.sock", "remote": "/rh.sock" }, "bridge": "/b.sock", "deploy": "/d",
        "control": "/c.sock", "proc": "/proc-here", "state": "/state.json",
        "runner": socket, "runner_owner": person.to_string(),
      })
      .to_string(),
    )
    .unwrap();
    serve_mcp_runner(&socket, asks.clone());
    asks
  }

  // A secret saved in the person's own space of the secret store.
  fn secret(&self, person: i64, name: &str, value: &str) {
    let space = self.ground.roots.data.join("env/users").join(person.to_string());

    std::fs::create_dir_all(&space).unwrap();
    std::fs::write(space.join(name), value).unwrap();
  }

  // The chain lets the company's personal tools reach the network and secrets.
  fn reach_allowed(&self) {
    let mut links = self.chain.links.lock().unwrap();

    links.insert(("personal-apps.network".to_owned(), "unit:3".to_owned()), json!(true));
    links.insert(("personal-apps.secrets".to_owned(), "unit:3".to_owned()), json!(true));
  }
}

fn skill_files(name: &str, said: &str) -> BTreeMap<String, String> {
  BTreeMap::from([
    (
      "SKILL.md".to_owned(),
      format!("---\nname: {name}\ndescription: {said}\ntitle: Tidy notes\n---\nTidy the notes as asked.\n"),
    ),
    ("references/style.md".to_owned(), "Short lines.".to_owned()),
  ])
}

// "a skill is shared from the maker's skills at home, installed into the receiver's with no PC, kept on its newest
// version whether or not their PC is heard, dropped with its attachments, and adopted as their own"
#[tokio::test]
async fn a_skill_is_shared_installed_moved_dropped_and_adopted_at_home_with_no_pc() {
  let stood = stood().await;
  let ground = &stood.ground;
  let data = &ground.roots.data;

  agency_server::skills::save(
    ground,
    &MAKER.to_string(),
    "tidy-notes",
    &skill_files("tidy-notes", "Tidies."),
  )
  .await
  .unwrap();

  let maker = reader(&stood, MAKER).await;
  let shared = catalogue::share(ground, &maker, "tidy-notes", &[], (Some("skill"), Some("ko")))
    .await
    .unwrap();

  assert_eq!(shared.kind, "skill");
  assert_eq!(shared.language.as_deref(), Some("ko"));
  assert_eq!(shared.titles.get("ko").map(String::as_str), Some("tidy-notes"));
  assert!(shared.permissions.network.is_empty() && shared.permissions.secrets.is_empty());
  assert!(shared.server.is_none());

  // The teammate holds no PC at all, and installs it into their skills at home.
  let teammate = reader(&stood, TEAMMATE).await;
  let installed = catalogue::install(ground, &teammate, "17-tidy-notes").await.unwrap();
  let copy = agency_server::skills::one(data, &TEAMMATE.to_string(), "tidy-notes").unwrap();

  assert_eq!(installed.installed.as_ref().unwrap().version, shared.version);
  assert_eq!(copy.received, Some(("17-tidy-notes".to_owned(), shared.version.clone())));
  assert_eq!(copy.origin(), "installed");
  assert!(
    data.join("agency/skills/21/tidy-notes/references/style.md").is_file(),
    "the reference file reaches the receiver's skills"
  );
  assert!(matches!(
    agency_server::skills::save(
      ground,
      &TEAMMATE.to_string(),
      "tidy-notes",
      &skill_files("tidy-notes", "Mine.")
    )
    .await,
    Err(agency_server::skills::Fault::NameTaken(_))
  ));

  // A new version asking for nothing more moves the copy as the copies are kept, with no PC heard.
  tokio::time::sleep(std::time::Duration::from_millis(3)).await;
  agency_server::skills::save(
    ground,
    &MAKER.to_string(),
    "tidy-notes",
    &skill_files("tidy-notes", "Tidier."),
  )
  .await
  .unwrap();

  let second = catalogue::share(ground, &maker, "tidy-notes", &[], (Some("skill"), None))
    .await
    .unwrap()
    .version;

  catalogue::keep(ground, &TEAMMATE.to_string()).await.unwrap();
  assert_eq!(
    catalogue::entry(ground, &teammate, "17-tidy-notes")
      .await
      .unwrap()
      .installed
      .unwrap()
      .version,
    second
  );
  assert!(
    std::fs::read_to_string(data.join("agency/skills/21/tidy-notes/SKILL.md"))
      .unwrap()
      .contains("Tidier."),
    "the copy stands on the newest version"
  );

  // Dropped, the folder goes with its attachments; installed again and adopted, it is the teammate's own.
  sea_orm::ConnectionTrait::execute_unprepared(
    &ground.db,
    r#"INSERT INTO "agent_attachments" VALUES ('21', 'supervisor', 'mate-bmo', 'skill', 'tidy-notes', 1)"#,
  )
  .await
  .unwrap();
  catalogue::uninstall(ground, &teammate, "17-tidy-notes").await.unwrap();
  assert!(agency_server::skills::one(data, &TEAMMATE.to_string(), "tidy-notes").is_none());
  assert!(
    agency_server::attachments::holders_of(&ground.db, "21", "skill", "tidy-notes")
      .await
      .unwrap()
      .is_empty(),
    "a dropped copy's attachments go with it"
  );

  catalogue::install(ground, &teammate, "17-tidy-notes").await.unwrap();

  let adopted = catalogue::adopt(ground, &teammate, "17-tidy-notes").await.unwrap();

  assert!(adopted.installed.is_none());
  assert_eq!(
    agency_server::skills::one(data, &TEAMMATE.to_string(), "tidy-notes").unwrap().origin(),
    "own"
  );
}

// "an MCP server is put under its own key; its place asks for what it wants, carried read from the person's own
// secret space; it is shared as an app, shown with how it is reached, and installed with its place carrying what its
// version asks for, refused where the receiver's space holds no file of its secret"; and agency's record of each
// account's MCP servers keeps the key through the put, the install, the adoption and the removal
#[tokio::test]
async fn an_mcp_server_is_put_under_its_own_key_shared_and_installed_carrying_its_secret() {
  use base64::Engine;

  let stood = stood().await;
  let ground = &stood.ground;
  let maker_asks = stood.mcp_pc(MAKER).await;
  let mate_asks = stood.mcp_pc(TEAMMATE).await;

  stood.reach_allowed();

  // The maker's secret is not saved yet: the place wants it, and the put is refused naming it.
  assert!(matches!(
    personal_apps::put(ground, &MAKER.to_string(), "tracker").await,
    Err(personal_apps::Fault::SecretMissing(name)) if name == "tracker_token"
  ));

  stood.secret(MAKER, "tracker_token", "maker-value");
  personal_apps::put(ground, &MAKER.to_string(), "tracker").await.unwrap();

  let places: Vec<Value> = maker_asks
    .lock()
    .unwrap()
    .iter()
    .filter_map(|asked| asked.get("place").cloned())
    .collect();
  let encoded = base64::engine::general_purpose::STANDARD.encode("maker-value");

  assert_eq!(places.last().unwrap()["secrets"], json!({ "tracker_token": encoded }));
  assert_eq!(places.last().unwrap()["network"], json!(["mcp.example.com"]));
  assert_eq!(home_pointer(&stood, MAKER, "tracker").unwrap()["app"], "own-17-tracker");

  // The maker's listing of their personal apps names it an MCP server.
  let listed = personal_apps::list(ground, &MAKER.to_string()).await.unwrap();

  assert_eq!(listed["items"][0]["kind"], "mcp", "{listed}");

  // Shared as an app is, its kind read off its source, and shown with how it is reached.
  let maker = reader(&stood, MAKER).await;
  let shared = catalogue::share(ground, &maker, "tracker", &[], (None, None)).await.unwrap();
  let server = shared.server.clone().unwrap();

  assert_eq!(shared.kind, "mcp");
  assert_eq!(shared.permissions.secrets, ["tracker_token"]);
  assert_eq!(
    (
      server.transport.as_str(),
      server.url.as_deref(),
      server.auth.as_deref(),
      server.secret.as_deref()
    ),
    ("remote", Some("https://mcp.example.com/mcp"), Some("bearer"), Some("tracker_token"))
  );

  // The teammate's space holds no file of the secret: refused, naming it.
  let teammate = reader(&stood, TEAMMATE).await;
  let refused = catalogue::install(ground, &teammate, "17-tracker").await.unwrap_err();

  assert!(format!("{refused:?}").contains("SecretMissing"), "{refused:?}");

  stood.secret(TEAMMATE, "tracker_token", "mate-value");
  catalogue::install(ground, &teammate, "17-tracker").await.unwrap();

  let first_place = mate_asks
    .lock()
    .unwrap()
    .iter()
    .filter_map(|asked| asked.get("place").cloned())
    .find(|place| place.get("secrets").is_some())
    .unwrap();

  assert_eq!(
    first_place["secrets"],
    json!({ "tracker_token": base64::engine::general_purpose::STANDARD.encode("mate-value") }),
    "a received copy's place carries its version's permissions from the first ask"
  );
  assert_eq!(home_pointer(&stood, TEAMMATE, "tracker").unwrap()["app"], "shared-3-17-tracker");

  // Agency's own record of each account's MCP servers holds the key each copy stands under.
  let recorded = |account: i64| async move {
    agency_server::attachments::recorded_servers(&ground.db, &account.to_string())
      .await
      .unwrap()
  };
  let one = |key: &str| BTreeMap::from([("tracker".to_owned(), key.to_owned())]);

  assert_eq!(recorded(MAKER).await, one("own-17-tracker"));
  assert_eq!(recorded(TEAMMATE).await, one("shared-3-17-tracker"));

  // Adopted, the copy is the teammate's own and stands under their own key; removed, it leaves the record.
  catalogue::adopt(ground, &teammate, "17-tracker").await.unwrap();
  assert_eq!(home_pointer(&stood, TEAMMATE, "tracker").unwrap()["app"], "own-21-tracker");
  assert_eq!(recorded(TEAMMATE).await, one("own-21-tracker"));

  personal_apps::remove(ground, &TEAMMATE.to_string(), "tracker").await.unwrap();
  assert!(recorded(TEAMMATE).await.is_empty());
  assert_eq!(recorded(MAKER).await, one("own-17-tracker"), "another account's record stands");

  // A dropped copy leaves the record as well.
  catalogue::install(ground, &teammate, "17-tracker").await.unwrap();
  assert_eq!(recorded(TEAMMATE).await, one("shared-3-17-tracker"));
  catalogue::uninstall(ground, &teammate, "17-tracker").await.unwrap();
  assert!(recorded(TEAMMATE).await.is_empty());
}

// "the maker asks for the other languages: each locale holding no text is filled and marked machine; the maker sets
// one locale, which takes its mark away, or clears it; a reader who is neither maker, owner nor head may do neither"
#[tokio::test]
async fn filled_languages_are_marked_machine_and_a_correction_takes_the_mark_away() {
  let stood = stood().await;
  let ground = &stood.ground;

  agency_server::skills::save(
    ground,
    &MAKER.to_string(),
    "tidy-notes",
    &skill_files("tidy-notes", "Tidies."),
  )
  .await
  .unwrap();

  let maker = reader(&stood, MAKER).await;

  catalogue::share(ground, &maker, "tidy-notes", &[], (Some("skill"), Some("ko")))
    .await
    .unwrap();

  let translated = |language: &'static str, text: String| async move {
    Ok::<String, String>(format!("[{language}] {text}"))
  };
  let teammate = reader(&stood, TEAMMATE).await;

  assert!(matches!(
    catalogue::fill_languages(ground, &teammate, "17-tidy-notes", translated).await,
    Err(Fault::Forbidden(_))
  ));

  let filled = catalogue::fill_languages(ground, &maker, "17-tidy-notes", translated).await.unwrap();

  assert_eq!(filled.titles.get("en").map(String::as_str), Some("[English] tidy-notes"));
  assert_eq!(filled.descriptions.get("ja").map(String::as_str), Some("[Japanese] Tidies."));
  assert_eq!(filled.titles.get("ko").map(String::as_str), Some("tidy-notes"), "the maker's own stays");
  assert_eq!(filled.machine, ["en", "ja", "zh-CN"]);

  // The head of the maker's unit may fill them too; a teammate may set none.
  assert!(
    catalogue::entry(ground, &reader(&stood, HEAD).await, "17-tidy-notes")
      .await
      .unwrap()
      .may
      .translate
  );
  assert!(matches!(
    catalogue::text(ground, &teammate, "17-tidy-notes", "en", Some("Mine"), None).await,
    Err(Fault::Forbidden(_))
  ));

  let corrected =
    catalogue::text(ground, &maker, "17-tidy-notes", "en", Some("Tidy notes"), Some("Tidies the notes."))
      .await
      .unwrap();

  assert_eq!(corrected.titles.get("en").map(String::as_str), Some("Tidy notes"));
  assert_eq!(corrected.machine, ["ja", "zh-CN"]);

  let cleared = catalogue::text(ground, &maker, "17-tidy-notes", "ja", None, None).await.unwrap();

  assert!(!cleared.titles.contains_key("ja"));
  assert_eq!(cleared.machine, ["zh-CN"]);
  assert!(matches!(
    catalogue::text(ground, &maker, "17-tidy-notes", "fr", Some("x"), None).await,
    Err(Fault::BadRequest(_))
  ));
}

#[test]
fn a_review_names_the_item_s_kind_on_its_first_line() {
  let files = json!({ "SKILL.md": "---\nname: a\ndescription: b\n---\n", "mcp.toml": "[server]" });

  for (kind, first) in [
    ("skill", "The item is a skill"),
    ("mcp", "The item is an MCP server"),
    ("app", "The item is an app"),
  ] {
    let input = catalogue::review_input(kind, &Asks::default(), &files);

    assert!(input.lines().next().unwrap().starts_with(first), "{input}");
    assert!(input.contains("=== SKILL.md ===") && input.contains("=== mcp.toml ==="), "{input}");
  }
}

#[test]
fn an_mcp_server_s_reach_reads_off_its_mcp_toml() {
  let local = catalogue::server_in(
    "[server]\nname = \"files\"\ntitle = \"Files\"\ndescription = \"d\"\ntransport = \"local\"\ncommand = [\"python3\", \"server.py\"]\n",
  )
  .unwrap();

  assert_eq!(local.transport, "local");
  assert_eq!(local.command, Some(vec!["python3".to_owned(), "server.py".to_owned()]));
  assert!(catalogue::server_in("[app]").is_none());
}
