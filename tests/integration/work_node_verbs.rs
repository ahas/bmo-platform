//! A work node's verbs of `bmo-agency` (docs/work-nodes.md): the workplace's setup run against a shell
//! standing in for the node and its rows over a scratch store, never a real node.

use crate::integration::common;

use std::sync::Mutex;

use agency_server::peers::node::NoNodeService;
use agency_server::verbs::work_node::{
  WorkNodeVerb, far_end_taken, refused_listens, work_node_on, work_refusal,
};
use agency_server::verbs::work_node_setup::{
  CONFINEMENT_TRY, FAR_END_UNIT, FarEnd, NODE_CRATES, SetupCheckout, SetupHands, SetupLogin, SetupOutcome,
  SetupPlace, SetupShell, ShellRun, far_end_unit, session_socket, set_up_workplace,
};
use common::Scratch;
use futures_util::future::BoxFuture;
use regex::Regex;

// A node answering each command by the first rule it matches, every command written down.
struct StandIn {
  rules: Vec<(Regex, ShellRun)>,
  ran: Mutex<Vec<String>>,
}

impl StandIn {
  fn new(rules: &[(&str, i32, &str, &str)]) -> Self {
    Self {
      rules: rules
        .iter()
        .map(|(pattern, code, stdout, stderr)| {
          (
            Regex::new(pattern).unwrap(),
            ShellRun {
              code: *code,
              stdout: (*stdout).to_owned(),
              stderr: (*stderr).to_owned(),
            },
          )
        })
        .collect(),
      ran: Mutex::new(Vec::new()),
    }
  }

  fn ran(&self) -> Vec<String> {
    self.ran.lock().unwrap().clone()
  }
}

impl SetupShell for StandIn {
  fn run<'a>(&'a self, command: &'a str) -> BoxFuture<'a, ShellRun> {
    self.ran.lock().unwrap().push(command.to_owned());

    let answer = self
      .rules
      .iter()
      .find(|(pattern, _)| pattern.is_match(command))
      .map(|(_, run)| run.clone());

    Box::pin(async move { answer.unwrap_or_default() })
  }
}

struct Hands {
  install: bool,
  copy_code: bool,
  /// Whether a copy lands on the node.
  lands: bool,
  said: Mutex<Vec<String>>,
}

impl SetupHands for Hands {
  fn install(&self) -> bool {
    self.install
  }

  fn copy_code(&self) -> bool {
    self.copy_code
  }

  fn copy<'a>(&'a self, _from: &'a str, _to: &'a str) -> BoxFuture<'a, (bool, String)> {
    let lands = self.lands;

    Box::pin(async move { (lands, if lands { String::new() } else { "the node timed out".to_owned() }) })
  }

  fn add_machine(&self) -> BoxFuture<'_, (bool, String)> {
    Box::pin(async { (true, String::new()) })
  }

  fn say(&self, line: &str) {
    self.said.lock().unwrap().push(line.to_owned());
  }
}

fn hands(copy_code: bool) -> Hands {
  Hands {
    install: false,
    copy_code,
    lands: true,
    said: Mutex::new(Vec::new()),
  }
}

const ACCOUNT: &str = "Account A";

fn to_node(path: &str, home: &str, _root: &str) -> String {
  path.replacen("/h", home, 1)
}

fn place(node_token: bool) -> SetupPlace<'static> {
  SetupPlace {
    node: "n1".into(),
    root: "~/.local/share/bmo-node".into(),
    deploy: "/h/deploy".into(),
    checkouts: vec![SetupCheckout {
      worker: "lab-app-n1".into(),
      cwd: "~/lab-app-n1".into(),
      remote: Some("https://example.com/lab-app.git".into()),
    }],
    logins: vec![SetupLogin {
      account: ACCOUNT.into(),
      own_directory: true,
      path: "/h/.claude".into(),
      node_token,
      token_command: format!("/h/deploy/bin/bmo node claude-token '{ACCOUNT}'"),
    }],
    to_node: &to_node,
    herdr: None,
    rust: None,
    bun: None,
    claude: None,
    session: "bmo".into(),
    far_end: None,
  }
}

const HOME: (&str, i32, &str, &str) = ("printf", 0, "/home/tester\n", "");

// "names everything still missing, one thing per line, and reads the herdr session's socket"
#[tokio::test]
async fn names_everything_still_missing_one_thing_per_line_and_reads_the_herdr_session_s_socket() {
  let shell = StandIn::new(&[
    HOME,
    (r"command -v 'bun'|command -v bun\b", 1, "", ""),
    (
      "herdr session list",
      0,
      "name status directory socket\nbmo running /home/tester/.config/herdr/sessions/bmo /home/tester/.config/herdr/sessions/bmo/herdr.sock\n",
      "",
    ),
    (r"test -d .*lab-app-n1/\.git", 0, "", ""),
    ("git push --dry-run", 1, "", "fatal: could not read Username\n"),
    (r"credentials\.json", 1, "", ""),
  ]);
  let outcome = set_up_workplace(&shell, &place(false), &hands(true)).await;

  assert_eq!(outcome.home.as_deref(), Some("/home/tester"));
  assert_eq!(outcome.root.as_deref(), Some("/home/tester/.local/share/bmo-node"));
  assert!(outcome.copied, "the copy landed, so the node holds the deployed commit");
  assert_eq!(
    outcome.herdr_socket.as_deref(),
    Some("/home/tester/.config/herdr/sessions/bmo/herdr.sock")
  );
  assert_eq!(
    outcome.missing,
    [
      "bun: not installed; run the setup with --install".to_owned(),
      "lab-app-n1: the node cannot push to its remote: fatal: could not read Username".to_owned(),
      format!(
        "login {ACCOUNT}: no token for the work nodes; on this machine, run /h/deploy/bin/bmo node claude-token '{ACCOUNT}'"
      ),
    ]
  );
  // The checkout is trusted under the login's directory on the node.
  assert!(shell.ran().iter().any(|command| command.contains("/deploy/bin/agent-host trust ")
    && command.contains("/home/tester/.claude/.claude.json")));
}

// "an account whose token for the work nodes this machine holds needs no login on the node"
#[tokio::test]
async fn an_account_whose_token_this_machine_holds_needs_no_login_on_the_node() {
  let shell = StandIn::new(&[HOME, (r"credentials\.json", 1, "", "")]);
  let outcome = set_up_workplace(&shell, &place(true), &hands(false)).await;

  assert!(outcome.missing.iter().all(|line| !line.starts_with("login ")));
  assert!(shell.ran().iter().all(|command| !command.contains("credentials.json")));
}

// "a check copies nothing, clones nothing, and says what the setup would have to make"
#[tokio::test]
async fn a_check_copies_nothing_clones_nothing_and_says_what_the_setup_would_have_to_make() {
  let shell = StandIn::new(&[HOME, ("test -x", 1, "", ""), (r"test -d .*\.git", 1, "", "")]);
  let outcome = set_up_workplace(&shell, &place(false), &hands(false)).await;

  assert!(outcome.missing.contains(
    &"code: the deploy tree's copy or its binaries are not on the node; run the setup".to_owned()
  ));
  assert!(
    outcome
      .missing
      .contains(&"lab-app-n1: no checkout at /home/tester/lab-app-n1; run the setup".to_owned())
  );
  assert!(
    shell
      .ran()
      .iter()
      .all(|command| !command.contains("git clone") && !command.contains("cargo build"))
  );
}

// "a copy that did not land says so and leaves the node's commit as it was"
#[tokio::test]
async fn a_copy_that_did_not_land_says_so_and_leaves_the_node_s_commit_as_it_was() {
  let shell = StandIn::new(&[HOME]);
  let mut stalled = hands(true);

  stalled.lands = false;

  let outcome = set_up_workplace(&shell, &place(true), &stalled).await;

  assert!(!outcome.copied);
  assert!(
    outcome
      .missing
      .contains(&"code: the deploy tree could not be copied: the node timed out".to_owned()),
    "{:?}",
    outcome.missing
  );
  assert!(
    !set_up_workplace(&shell, &place(true), &hands(false)).await.copied,
    "a check copies nothing"
  );
}

// "a node whose login fails names that alone"
#[tokio::test]
async fn a_node_whose_login_fails_names_that_alone() {
  let shell =
    StandIn::new(&[("printf", 255, "", "ssh: connect to host n1.example port 22: No route to host\n")]);

  assert_eq!(
    set_up_workplace(&shell, &place(false), &hands(true)).await,
    SetupOutcome {
      home: None,
      root: None,
      missing: vec!["login: ssh: connect to host n1.example port 22: No route to host".into()],
      herdr_socket: None,
      copied: false,
      confinement: None,
      claude: None,
    }
  );
}

// "the session's socket is read off herdr's own listing"
#[test]
fn the_session_s_socket_is_read_off_herdr_s_own_listing() {
  assert_eq!(
    session_socket(
      "name status directory socket\ndefault running /h/.config/herdr /h/.config/herdr/herdr.sock\n",
      "bmo"
    ),
    None
  );
  assert_eq!(
    session_socket("bmo running /h/.config/herdr/sessions/bmo /h/x.sock", "bmo").as_deref(),
    Some("/h/x.sock")
  );
}

// "takes each dependency as the root workspace declares it, its members are the crates the setup copies, and
// they reach no crate outside it"
#[test]
fn the_node_build_takes_each_dependency_as_the_root_workspace_declares_it() {
  let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
  let read =
    |path: &str| -> toml::Table { std::fs::read_to_string(root.join(path)).unwrap().parse().unwrap() };
  let dependencies = |table: &toml::Table| -> toml::Table {
    table
      .get("workspace")
      .and_then(|workspace| workspace.get("dependencies"))
      .and_then(toml::Value::as_table)
      .cloned()
      .unwrap_or_default()
  };
  let whole = dependencies(&read("Cargo.toml"));
  let node = dependencies(&read("crates/node-link/node-build.toml"));
  // What a crate's package takes from the workspace, its license among it, the node's build declares alike.
  let package =
    |table: &toml::Table| table.get("workspace").and_then(|workspace| workspace.get("package")).cloned();

  assert_eq!(package(&read("crates/node-link/node-build.toml")), package(&read("Cargo.toml")));

  for (name, declared) in &node {
    assert_eq!(Some(declared), whole.get(name), "{name}");
  }

  let build = read("crates/node-link/node-build.toml");
  let members: Vec<&str> = build
    .get("workspace")
    .and_then(|workspace| workspace.get("members"))
    .and_then(toml::Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(toml::Value::as_str)
    .collect();

  assert_eq!(members, NODE_CRATES);

  for krate in NODE_CRATES {
    let manifest = read(&format!("{krate}/Cargo.toml"));

    for section in ["dependencies", "dev-dependencies"] {
      for (name, spec) in manifest.get(section).and_then(toml::Value::as_table).into_iter().flatten() {
        if spec.get("path").is_some() {
          assert!(
            NODE_CRATES.iter().any(|one| one.ends_with(&format!("/{name}"))),
            "{krate} reaches {name}"
          );
        } else {
          assert!(node.contains_key(name), "{krate} takes {name}, which the node's build does not declare");
        }
      }
    }
  }
}

#[test]
fn a_node_takes_work_only_where_the_node_service_names_a_login_and_reserves_it_for_none_but_jobs() {
  assert_eq!(work_refusal("n1", true, None), None);
  assert_eq!(work_refusal("n1", true, Some("jobs+voice")), None);
  assert_eq!(
    work_refusal("n1", true, Some("voice")).as_deref(),
    Some("n1 is reserved for voice and carries no jobs")
  );
  assert_eq!(work_refusal("n1", false, None).as_deref(), Some("n1 has no SSH route to carry a job"));
}

#[tokio::test]
async fn a_workplace_is_listed_its_runs_switched_and_let_go() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let db = scratch.db();
  let listed = work_node_on(db, &NoNodeService, data.path(), None).await.unwrap();

  assert_eq!(listed.lines, ["no workplace on any node"]);
  scratch
    .run(
      r#"INSERT INTO "node_workplaces" ("node", "root", "runs", "state", "detail", "created_at", "updated_at") VALUES ('n1', '~/w', 0, 'missing', 'bun: not installed', 1, 1)"#,
      vec![],
    )
    .await;

  let listed = work_node_on(db, &NoNodeService, data.path(), None).await.unwrap();

  assert_eq!(
    listed.lines,
    [
      "n1           missing     runs off root ~/w · checked never · no link",
      "  - bun: not installed"
    ]
  );
  assert_eq!(
    work_node_on(
      db,
      &NoNodeService,
      data.path(),
      Some(WorkNodeVerb::Runs {
        node: "n1".into(),
        switched: "on".into()
      })
    )
    .await
    .unwrap()
    .lines,
    ["n1: heavy runs on"]
  );
  assert_eq!(
    work_node_on(db, &NoNodeService, data.path(), Some(WorkNodeVerb::Remove { node: "n1".into() }))
      .await
      .unwrap()
      .lines,
    ["n1: its workplace is no longer one; what stands on the node is left as it is"]
  );
  assert!(
    work_node_on(db, &NoNodeService, data.path(), Some(WorkNodeVerb::Remove { node: "n1".into() }))
      .await
      .is_err()
  );
}

// "a PC's far end another node's ready workplace holds, told by the machine id its setup record keeps, is said
// taken; a workplace not ready, the node's own and another machine's hold none"
#[tokio::test]
async fn a_pc_s_far_end_another_ready_workplace_holds_is_said_taken() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let db = scratch.db();

  for (node, state, machine) in
    [("pc-a", "ready", "m1"), ("pc-b", "missing", "m1"), ("pc-c", "ready", "m2")]
  {
    scratch
      .run(
        r#"INSERT INTO "node_workplaces" ("node", "root", "runs", "state", "created_at", "updated_at") VALUES ($1, '~/w', 0, $2, 1, 1)"#,
        vec![node.into(), state.into()],
      )
      .await;
    std::fs::create_dir_all(data.path().join("nodes").join(node)).unwrap();
    std::fs::write(
      data.path().join("nodes").join(node).join("setup.json"),
      format!(r#"{{"herdrSession": "bmo", "machine": "{machine}"}}"#),
    )
    .unwrap();
  }

  let taken = |node: &'static str, machine: &'static str| far_end_taken(db, data.path(), node, machine);

  assert_eq!(
    taken("pc-b", "m1").await.unwrap().as_deref(),
    Some(
      "far end: pc-a's ready workplace holds this PC's far end, the same Linux shell answering both; one far end serves one node, so one of the two workplaces is removed first"
    )
  );
  assert_eq!(taken("pc-a", "m1").await.unwrap(), None);
  assert_eq!(taken("pc-d", "m3").await.unwrap(), None);
}

const PC_PORTS: FarEnd = FarEnd {
  mirror_port: 47201,
  herdr_port: 47202,
};

const HERDR_LISTING: (&str, i32, &str, &str) = (
  "herdr session list",
  0,
  "name status directory socket\nbmo running /home/tester/.config/herdr/sessions/bmo /home/tester/.config/herdr/sessions/bmo/herdr.sock\n",
  "",
);

// "a PC's far end is written as a user service and started again after the copy, and a check reads it running"
#[tokio::test]
async fn a_pc_s_far_end_is_written_as_a_user_service_and_started_again_after_the_copy() {
  let pc = || SetupPlace {
    far_end: Some(PC_PORTS),
    checkouts: vec![],
    ..place(true)
  };
  let shell = StandIn::new(&[HOME, HERDR_LISTING]);
  let outcome = set_up_workplace(&shell, &pc(), &hands(true)).await;
  let stood = shell
    .ran()
    .into_iter()
    .find(|command| command.contains("restart bmo-node-link.service"));

  assert!(outcome.missing.iter().all(|line| !line.starts_with("far end")));
  assert!(
    stood
      .is_some_and(|command| command.contains("/home/tester/.config/systemd/user/bmo-node-link.service")
        && command.contains("enable -q bmo-node-link.service"))
  );

  let shell = StandIn::new(&[HOME, HERDR_LISTING, ("is-active -q bmo-node-link", 3, "", "")]);
  let outcome = set_up_workplace(&shell, &pc(), &hands(false)).await;

  assert!(
    outcome
      .missing
      .contains(&format!("far end: {FAR_END_UNIT} does not run on the node; run the setup"))
  );
}

// "a PC's setup looks for bubblewrap and tries it, saying why its personal tools cannot run confined, and a node that
// is no PC is asked neither"
#[tokio::test]
async fn a_pc_s_setup_looks_for_bubblewrap_and_tries_it() {
  let pc = || SetupPlace {
    far_end: Some(PC_PORTS),
    checkouts: vec![],
    ..place(true)
  };
  let tried = |shell: &StandIn| shell.ran().iter().any(|command| command.contains(CONFINEMENT_TRY));

  let shell = StandIn::new(&[HOME, HERDR_LISTING]);
  let outcome = set_up_workplace(&shell, &pc(), &hands(true)).await;

  assert_eq!(outcome.confinement, None);
  assert!(tried(&shell));
  assert!(
    outcome.missing.iter().all(|line| !line.contains("bubblewrap")),
    "it holds nothing else back"
  );

  let shell = StandIn::new(&[HOME, HERDR_LISTING, (r"command -v 'bwrap'|command -v bwrap\b", 1, "", "")]);
  let outcome = set_up_workplace(&shell, &pc(), &hands(true)).await;

  assert!(
    outcome
      .confinement
      .as_deref()
      .is_some_and(|why| why.contains("bubblewrap (bwrap) is not on the PC"))
  );
  assert!(!tried(&shell));

  let shell = StandIn::new(&[
    HOME,
    HERDR_LISTING,
    (r"bwrap --ro-bind", 1, "", "bwrap: setting up uid map: Permission denied\n"),
  ]);
  let outcome = set_up_workplace(&shell, &pc(), &hands(true)).await;

  assert!(
    outcome
      .confinement
      .as_deref()
      .is_some_and(|why| why.contains("makes no namespace") && why.contains("uid map")),
    "{:?}",
    outcome.confinement
  );

  let shell = StandIn::new(&[HOME, HERDR_LISTING]);
  let outcome = set_up_workplace(&shell, &place(true), &hands(true)).await;

  assert_eq!(outcome.confinement, None);
  assert!(!tried(&shell));
}

// "the far end's unit runs the serve program on the PC's loopback ports and stops alone"
#[test]
fn the_far_end_s_unit_runs_the_serve_program_on_the_pc_s_loopback_ports_and_stops_alone() {
  let unit = far_end_unit("/home/tester", "/home/tester/w", "bmo", "/run/herdr.sock", &PC_PORTS);
  let exec = unit.lines().find_map(|line| line.strip_prefix("ExecStart=")).unwrap();

  assert_eq!(
    exec,
    r#""/home/tester/w/deploy/bin/node-link" "serve" "--root" "/home/tester/w" "--herdr-session" "bmo" "--herdr-socket" "/run/herdr.sock" "--listen" "127.0.0.1:47201" "--herdr-listen" "127.0.0.1:47202""#
  );
  assert!(unit.lines().any(|line| line == "KillMode=process"));
  assert!(
    unit
      .lines()
      .any(|line| line.starts_with(r#"Environment="PATH=/home/tester/w/bun/bin:"#))
  );
}

// "a runtime of another version is installed into the workplace, leaving the account's own"
#[tokio::test]
async fn a_runtime_of_another_version_is_installed_into_the_workplace_leaving_the_account_s_own() {
  let shell = StandIn::new(&[HOME, ("bun --version", 0, "1.4.4\n", "")]);
  let hands = Hands {
    install: true,
    ..hands(false)
  };
  let place = SetupPlace {
    bun: Some("1.4.2".into()),
    ..place(true)
  };

  set_up_workplace(&shell, &place, &hands).await;

  let installed = shell
    .ran()
    .into_iter()
    .find(|command| command.contains("https://bun.sh/install"))
    .unwrap();

  assert!(installed.contains("BUN_INSTALL=/home/tester/.local/share/bmo-node/bun"));
  assert!(installed.contains("HOME=/home/tester/.local/share/bmo-node/bun/home"));
  assert!(installed.contains("bun-v1.4.2"));
}

// "the workplace's own Claude Code is laid at this machine's release under a name of its own, never into the
// account's tools, and a node whose copy already reports that release is left as it is"
#[tokio::test]
async fn the_workplace_s_own_claude_code_is_laid_at_this_machine_s_release_never_into_the_account_s_tools()
{
  const OWN: &str = "/home/tester/.local/share/bmo-node/bin/bmo-claude";
  let place = SetupPlace {
    claude: Some("2.1.9".into()),
    checkouts: vec![],
    ..place(true)
  };
  let older = StandIn::new(&[HOME, (r"bmo-claude --version", 0, "2.1.8 (Claude Code)\n", "")]);
  let outcome = set_up_workplace(&older, &place, &hands(true)).await;
  let laid: Vec<String> = older
    .ran()
    .into_iter()
    .filter(|command| command.contains("BMO_CLAUDE_RELEASE"))
    .collect();

  assert_eq!(laid.len(), 1, "{:?}", older.ran());
  assert!(laid[0].contains("BMO_CLAUDE_RELEASE=2.1.9"));
  assert!(laid[0].contains(&format!("BMO_CLAUDE_TO={OWN}")));
  assert!(laid[0].contains("BMO_CLAUDE_DOWNLOADS=https://downloads.claude.ai/claude-code-releases"));
  assert!(older.ran().iter().all(|command| !command.contains("claude.ai/install.sh")
    && !command.contains("command -v claude")
    && !command.contains(".local/bin/claude")));
  // What the copy reports after the laying is what the outcome names: the stand-in still answers the earlier one.
  assert_eq!(outcome.claude.as_deref(), Some("2.1.8"));

  let same = StandIn::new(&[HOME, (r"bmo-claude --version", 0, "2.1.9 (Claude Code)\n", "")]);
  let outcome = set_up_workplace(&same, &place, &hands(true)).await;

  assert!(same.ran().iter().all(|command| !command.contains("BMO_CLAUDE_RELEASE")));
  assert_eq!(outcome.claude.as_deref(), Some("2.1.9"));
  assert!(outcome.missing.iter().all(|line| !line.starts_with("claude:")), "{:?}", outcome.missing);
}

// "a workplace with no Claude Code of its own is missing it, and a check lays none"
#[tokio::test]
async fn a_workplace_with_no_claude_code_of_its_own_is_missing_it_and_a_check_lays_none() {
  let place = SetupPlace {
    claude: Some("2.1.9".into()),
    checkouts: vec![],
    ..place(true)
  };
  let shell = StandIn::new(&[HOME, (r"bmo-claude", 1, "", "")]);
  let outcome = set_up_workplace(&shell, &place, &hands(false)).await;

  assert_eq!(outcome.claude, None);
  assert!(outcome.missing.contains(
    &"claude: the workplace holds no Claude Code of its own at /home/tester/.local/share/bmo-node/bin/bmo-claude; run the setup".to_owned()
  ), "{:?}", outcome.missing);
  assert!(shell.ran().iter().all(|command| !command.contains("BMO_CLAUDE_RELEASE")));

  // A setup whose laying fails with none standing says why.
  let failing =
    StandIn::new(&[HOME, (r"bmo-claude", 1, "", "error: manifest could not be fetched: 503\n")]);
  let outcome = set_up_workplace(&failing, &place, &hands(true)).await;

  assert!(
    outcome.missing.iter().any(
      |line| line.starts_with("claude: Claude Code 2.1.9 could not be laid at ") && line.contains("503")
    ),
    "{:?}",
    outcome.missing
  );
}

// "a port the far end could not listen on is said by what it is for while the link is connected, and nothing is
// said of a link that is down"
#[test]
fn a_port_the_far_end_could_not_listen_on_is_said_while_the_link_is_connected() {
  let data = tempfile::tempdir().unwrap();
  let state = data.path().join("nodes/u4/link-state.json");
  let ports = [(13123, "the tool host"), (18030, "the model gateway")];
  let link = |state_word: &str| {
    std::fs::create_dir_all(state.parent().unwrap()).unwrap();
    std::fs::write(
      &state,
      format!(
        r#"{{"state": "{state_word}", "refused": [{{"port": 13123, "why": "Address in use"}}, {{"port": 9, "why": "denied"}}]}}"#
      ),
    )
    .unwrap();
  };

  link("connected");
  assert_eq!(
    refused_listens(data.path(), "u4", &ports),
    [
      "far end: it could not listen on port 13123 for the tool host: Address in use",
      "far end: it could not listen on port 9 for a program here: denied",
    ]
  );
  link("down");
  assert!(refused_listens(data.path(), "u4", &ports).is_empty());
}

// "every login on the node is marked past its first-run screens and trusts the workplace's data root"
#[tokio::test]
async fn every_login_on_the_node_is_marked_past_its_first_run_screens_and_trusts_the_data_root() {
  let shell = StandIn::new(&[HOME, HERDR_LISTING]);
  let place = SetupPlace {
    checkouts: vec![],
    ..place(true)
  };

  set_up_workplace(&shell, &place, &hands(true)).await;

  let marked = shell.ran().into_iter().find(|command| command.contains("trust --onboarded")).unwrap();
  let built = shell.ran().into_iter().find(|command| command.contains("cargo build")).unwrap();

  // A binary running on the node is replaced, never written over.
  assert!(built.contains(
    "/home/tester/.local/share/bmo-node/deploy/bin/.agent-host.new && mv -f /home/tester/.local/share/bmo-node/deploy/bin/.agent-host.new /home/tester/.local/share/bmo-node/deploy/bin/agent-host"
  ));

  assert!(marked.contains("mkdir -p /home/tester/.claude"));
  assert!(marked.contains(
    "/home/tester/.local/share/bmo-node/deploy/bin/agent-host trust --onboarded /home/tester/.claude/.claude.json /home/tester/.local/share/bmo-node/data"
  ));

  let shell = StandIn::new(&[HOME, HERDR_LISTING]);

  set_up_workplace(&shell, &place, &hands(false)).await;
  assert!(
    shell.ran().iter().all(|command| !command.contains("trust --onboarded")),
    "a check writes nothing"
  );
}
