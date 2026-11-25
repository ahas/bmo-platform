//! What every place is stood on (docs/agent-control.md, 「One standing pane per agent」, 「The job's
//! release」): the store, the service's configuration, peers, events and hooks, the machine's trees and
//! environment, this machine's herdr, and the source root a worker with no directory of its own works in;
//! and an agent's row as the places read it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use agent_session::{Apart, SessionId};
use cache_siren::Keeping;
use harness_providers::Machine;
use herdr::Herdr;
use policy::fill_marks;
use policy::place_words::PLACE_WORDS;
use sea_orm::{DatabaseConnection, QueryResult};
use store::{all, one};

use super::home::{HEAD_MARK, HEAD_STOPPED, Home, Launch};
use crate::config::Config;
use crate::events::Bus;
use crate::hooks::Hooks;
use crate::peers::Peers;
use crate::readiness::Readiness;
use crate::usage_windows::LoginRenewals;
use crate::watches::links::{self, Roots};
use crate::{Refusal, Service};

// The variable a run is told its own name in, and the job runner's marker, which a run apart never
// carries (docs/live-claude-guard.md).
const RUN_NAME: &str = "BMO_RUN_NAME";
const JOB_RUNNER_MARKER: &str = "BMO_TASK_LOCKED";

// The address of agency's own store, which no program it starts is handed (docs/store.md, 「Connecting」):
// the variable names agency's store, and a program reads the archive it works on from the data root's own
// setting. A command of agency's that a session runs finds the store in `env/agency.env`.
const STORE_ADDRESS: &str = store::place::VARIABLE;

/// The variable naming the tree a process reads its code out of: the deploy tree, or the release a
/// process is pinned to (docs/agent-control.md, 「The job's release」).
pub const DEPLOY_VARIABLE: &str = "BMO_DEPLOY_DIR";

/// The verbs' binary, as a code tree's `bin/` holds it.
pub const VERBS_BINARY: &str = "bmo-agency";

/// The tokens of agency's own deployment a program it starts is handed as the file each stands in, never by
/// value (docs/agent-control.md, 「A token is handed as the file it stands in」).
pub const HANDED_AS_FILES: [&str; 5] = [
  "AGENCY_CONTROL_TOKEN",
  "AGENCY_BRAIN_TOKEN",
  "AGENCY_NODE_TOKEN",
  "BMO_JOB_TOKEN",
  "BMO_JOB_CHAT_TOKEN",
];

/// The variable naming the file a token stands in.
pub fn file_variable(name: &str) -> String {
  format!("{name}_FILE")
}

/// Where agency keeps a token it holds by value for the programs it starts: under the data root's
/// `secrets/`, by the token's variable.
pub fn secret_file(data_root: &Path, name: &str) -> PathBuf {
  data_root.join("secrets").join(name)
}

// A token written where the programs read it, readable by its owner alone, and left as it is where it already
// holds the value.
fn write_secret(data_root: &Path, name: &str, value: &str) -> std::io::Result<PathBuf> {
  use std::os::unix::fs::PermissionsExt as _;

  let path = secret_file(data_root, name);

  if std::fs::read_to_string(&path).ok().as_deref() == Some(value) {
    return Ok(path);
  }

  let dir = data_root.join("secrets");

  std::fs::create_dir_all(&dir)?;
  std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
  agent_host::disk::publish_bytes(&path, value.as_bytes())?;

  Ok(path)
}

/// An environment with each of agency's tokens handed as the file it stands in: a value it holds written under
/// the data root and named by the token's `_FILE` variable, a file the environment names alone kept as it is.
/// A value whose file cannot be written stays, and the log says so.
pub fn hand_as_files(env: &mut BTreeMap<String, String>, data_root: &Path) {
  for name in HANDED_AS_FILES {
    let Some(value) = env.get(name).map(|value| value.trim().to_owned()).filter(|value| !value.is_empty())
    else {
      continue;
    };

    match write_secret(data_root, name, &value) {
      Ok(path) => {
        env.remove(name);
        env.insert(file_variable(name), path.to_string_lossy().into_owned());
      }
      Err(why) => {
        eprintln!("places: {name} is handed by value, since its file could not be written: {why}")
      }
    }
  }
}

/// A token the environment holds by value, or the one the file its `_FILE` variable names holds.
pub fn token_or_file(value: Option<String>, file: Option<String>) -> Option<String> {
  value
    .map(|value| value.trim().to_owned())
    .filter(|value| !value.is_empty())
    .or_else(|| {
      file
        .filter(|path| !path.trim().is_empty())
        .and_then(|path| std::fs::read_to_string(path.trim()).ok())
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
    })
}

/// One place's row as the places read it: a supervisor, whose resident program stands in a home of its own,
/// or a repository's checkout, where runs of roles work and nothing stands between them.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentRow {
  pub name: String,
  /// `chat` for a supervisor, `worker` for a checkout.
  pub kind: String,
  pub label: Option<String>,
  pub herdr_label: Option<String>,
  pub node: Option<String>,
  pub cwd: Option<String>,
  pub dev_command: Option<String>,
  pub support_of: Option<String>,
  pub resting: bool,
}

// A supervisor's row as it is read, with its owner account and the node that account's record names.
const SUPERVISOR_SELECT: &str = r#"SELECT s."name", s."label", s."herdr_label", s."owner_account"::text AS "account",
  p."node" FROM "supervisors" s LEFT JOIN "person_nodes" p ON p."owner_account" = s."owner_account"::text"#;

// The kind a supervisor's row is read as, and a checkout's.
const SUPERVISOR: &str = "chat";
const CHECKOUT: &str = "worker";

impl AgentRow {
  fn supervisor(row: &QueryResult) -> Result<Self, sea_orm::DbErr> {
    Ok(Self {
      name: row.try_get("", "name")?,
      kind: SUPERVISOR.to_owned(),
      label: row.try_get("", "label")?,
      herdr_label: row.try_get("", "herdr_label")?,
      // The node its owner account's record names; `placed` gives an account with none its isolation node.
      node: row.try_get("", "node")?,
      cwd: None,
      dev_command: None,
      support_of: None,
      resting: false,
    })
  }

  // A supervisor's row with the node its owner account's runs stand on: the one its record names, else the
  // account's isolation node, none for no account or the administrator's (docs/account-isolation.md).
  async fn placed(db: &DatabaseConnection, row: &QueryResult) -> Result<Self, Refusal> {
    let mut agent = Self::supervisor(row)?;
    let account: Option<String> = row.try_get("", "account")?;

    agent.node = crate::person_nodes::placed_for(db, account.as_deref(), agent.node.take()).await?;
    Ok(agent)
  }

  async fn checkouts(db: &DatabaseConnection) -> Result<Vec<Self>, Refusal> {
    let mut rows = vec![];

    for worker in crate::queue::workers::workers(db, Path::new("")).await? {
      if worker.builtin {
        continue;
      }

      let base = worker.support_of.clone().unwrap_or_else(|| worker.name.clone());
      let dev_command = crate::repositories::named(db, &base)
        .await?
        .and_then(|repository| repository.dev_command);

      rows.push(Self {
        name: worker.name,
        kind: CHECKOUT.to_owned(),
        label: Some(worker.label),
        herdr_label: None,
        node: worker.node,
        cwd: Some(worker.cwd),
        dev_command,
        support_of: worker.support_of,
        resting: false,
      });
    }

    Ok(rows)
  }

  /// Every supervisor: the only rows whose program stands between turns.
  pub async fn standing(db: &DatabaseConnection) -> Result<Vec<Self>, Refusal> {
    let rows = all(db, &format!(r#"{SUPERVISOR_SELECT} ORDER BY s."name""#), vec![]).await?;
    let mut placed = Vec::new();

    for row in &rows {
      placed.push(Self::placed(db, row).await?);
    }

    Ok(placed)
  }

  /// Every repository's checkouts, in the order the queue lists them.
  pub async fn workers(db: &DatabaseConnection) -> Result<Vec<Self>, Refusal> {
    Self::checkouts(db).await
  }

  /// One supervisor's row, or one checkout's.
  pub async fn named(db: &DatabaseConnection, name: &str) -> Result<Option<Self>, Refusal> {
    let supervisor =
      one(db, &format!(r#"{SUPERVISOR_SELECT} WHERE s."name" = $1"#), vec![name.into()]).await?;

    if let Some(row) = supervisor {
      return Ok(Some(Self::placed(db, &row).await?));
    }

    Ok(Self::checkouts(db).await?.into_iter().find(|row| row.name == name))
  }

  /// The line the row's work goes out on: the chat line for a supervisor, the worker line for a checkout.
  pub fn line(&self) -> &str {
    &self.kind
  }

  /// The label herdr shows it by.
  pub fn shown(&self) -> &str {
    self.herdr_label.as_deref().or(self.label.as_deref()).unwrap_or(&self.name)
  }
}

/// What every place is stood on: the store, the service's configuration, the services it calls, the
/// accounts' usage windows it keeps and the runs that renew a login its reader finds expired, the bus its
/// events go on and the hooks its consumers registered, whether the places have stood once, the machine's
/// trees and environment, this machine's herdr, and the source root a worker with no directory of its own
/// works in.
pub struct Ground {
  pub db: DatabaseConnection,
  pub config: Arc<Config>,
  pub peers: Peers,
  pub windows: Keeping,
  pub renewals: LoginRenewals,
  pub events: Bus,
  pub hooks: Hooks,
  pub readiness: Arc<Readiness>,
  pub roots: Roots,
  pub herdr: Option<Herdr>,
  pub source: PathBuf,
  /// The thrown-away sessions a pickup carries across this process's start, by their id: each stands until
  /// its turn's end is handed back, and its state answers by id meanwhile (docs/sessions.md, 「A restart
  /// hands the session over」).
  pub carried: Arc<Mutex<BTreeMap<SessionId, (String, Apart)>>>,
  /// What every session's turns have written, the turns this process sent and those it picked up at its
  /// start alike, read by the progress read (docs/api.md, 「sessions」).
  pub outputs: Arc<crate::session_output::Outputs>,
  /// The turns standing under their callers' keys, and the keyed turns' ends kept (docs/api.md,
  /// 「sessions」).
  pub turn_keys: Arc<crate::turn_keys::TurnKeys>,
  /// Whether the server drains on its stop (docs/sessions.md, 「A restart hands the session over」).
  pub drain: crate::drain::Drain,
  vars: BTreeMap<String, String>,
  env: BTreeMap<String, String>,
}

/// Whether a tree holds the code the heads and the lines are composed of.
fn holds_code(tree: &Path) -> bool {
  tree.join("bin").join("pane-head").exists() || tree.join("prompts").join("rules").is_dir()
}

/// The first of these trees that holds code — the one `BMO_DEPLOY_DIR` names, the one the configuration
/// names, the deploy tree's link — and the working directory where none does: a test, or a machine with
/// no deployment.
fn first_code_tree(named: Option<PathBuf>, configured: Option<PathBuf>, deploy: &Path) -> PathBuf {
  named
    .into_iter()
    .chain(configured)
    .chain([deploy.to_path_buf()])
    .find(|tree| holds_code(tree))
    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| deploy.to_path_buf()))
}

/// The tree the code is read from as a machine names it, before any ground stands on it: what the store's
/// seeds read their files from as the server starts.
pub fn machine_code_root(machine: &Machine) -> PathBuf {
  let trees = crate::config::Trees::of_machine(machine);
  let home = PathBuf::from(machine.var("HOME").unwrap_or("/"));
  let deploy = trees.deploy_link(&home);

  first_code_tree(machine.var(DEPLOY_VARIABLE).map(PathBuf::from), trees.code, &deploy)
}

impl Ground {
  /// The ground of the service on a machine: its trees on the deploy link and the source root the
  /// configuration names. A process whose launcher named no code tree hands its own on to everything it
  /// starts, so no child reads a newer release than its parent (docs/agent-control.md, 「The job's
  /// release」).
  pub fn new(service: &Service, machine: &Machine, herdr: Option<Herdr>) -> Self {
    let home = PathBuf::from(machine.var("HOME").unwrap_or("/"));
    let trees = &service.config.trees;
    let renewals = LoginRenewals::default();
    let mut ground = Self {
      db: service.db.clone(),
      config: service.config.clone(),
      peers: service.peers.clone(),
      windows: crate::usage_windows::keeping(&service.db, machine, &renewals),
      renewals,
      events: service.events.clone(),
      hooks: service.hooks.clone(),
      readiness: service.readiness.clone(),
      roots: Roots::of_machine(machine, trees.deploy_link(&home)),
      herdr,
      source: trees.source_root(),
      carried: Arc::default(),
      outputs: Arc::default(),
      turn_keys: Arc::default(),
      drain: crate::drain::Drain::default(),
      vars: machine.vars().map(|(name, value)| (name.to_owned(), value.to_owned())).collect(),
      env: machine
        .every_var()
        .filter(|(name, _)| *name != STORE_ADDRESS)
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect(),
    };

    if machine.var(DEPLOY_VARIABLE).is_none() {
      let own = ground.code_root().to_string_lossy().into_owned();

      ground.vars.insert(DEPLOY_VARIABLE.to_owned(), own.clone());
      ground.env.insert(DEPLOY_VARIABLE.to_owned(), own);
    }

    // What a launch hands on carries agency's tokens as their files; agency's own calls read `vars`.
    hand_as_files(&mut ground.env, &ground.roots.data);

    ground
  }

  /// The ground of the service on a data root: the machine under that root, and this machine's herdr
  /// where the environment names one.
  pub fn of_data_root(service: &Service, data_root: &Path) -> Self {
    let machine = Machine::of_process(data_root);
    let herdr = herdr::socket_path(machine.var("XDG_CONFIG_HOME"), machine.var("HOME"))
      .map(|socket| Herdr::here(socket, herdr::silences_path(machine.var("BMO_HERDR_SILENCES"))));

    Self::new(service, &machine, herdr)
  }

  /// Whether this ground stands a test's server: it reaches no deployment's peer, whatever its process
  /// environment carried, and the calls that would reach one are not made (docs/api.md, 「Calls agency
  /// makes」).
  pub fn testing(&self) -> bool {
    self.vars.get("BMO_TEST_GROUND").is_some_and(|set| set.trim() == "1")
  }

  /// The ground with one variable of its environment set to the value given, or taken out where none is.
  pub fn with_var(mut self, name: &str, value: Option<&str>) -> Self {
    match value {
      Some(value) => self.env.insert(name.to_owned(), value.to_owned()),
      None => self.env.remove(name),
    };

    self
  }

  /// The tree the heads, the lines and the verbs run from, the first of these that holds code: the one
  /// `BMO_DEPLOY_DIR` names — a job's own release — the one the configuration names as the tree the
  /// deployment started this process on, and the deploy tree's link; the working directory where none
  /// does — a test, or a machine with no deployment.
  pub fn code_root(&self) -> PathBuf {
    let named = self.env.get(DEPLOY_VARIABLE).filter(|named| !named.is_empty()).map(PathBuf::from);

    first_code_tree(named, self.config.trees.code.clone(), &self.roots.deploy)
  }

  /// The folder every pane agency stands on this machine starts in: the data root, never the code tree.
  /// A folder is resolved by its real path as a process enters it, so a pane started in the deploy link
  /// would stand in the release live at its start for as long as its loop runs, and the deployment's prune
  /// keeps every release a process stands in (docs/agent-control.md, 「Where a pane stands」).
  pub fn pane_dir(&self) -> String {
    self.roots.data.to_string_lossy().into_owned()
  }

  /// The code tree as text a person or an agent reads names it: the deploy link wherever the link holds
  /// code, so no release path enters what is cached across a deployment.
  pub fn shown_code_root(&self) -> PathBuf {
    if holds_code(&self.roots.deploy) {
      self.roots.deploy.clone()
    } else {
      self.code_root()
    }
  }

  /// The verbs' binary a launch starts one of agency's verbs on: the code tree's own, as its scripts are
  /// the tree's, and the one beside this program where the tree holds none.
  pub fn verbs_binary(&self) -> PathBuf {
    let deployed = self.code_root().join("bin").join(VERBS_BINARY);

    if deployed.exists() {
      return deployed;
    }

    std::env::current_exe()
      .ok()
      .and_then(|own| Some(own.parent()?.join(VERBS_BINARY)))
      .unwrap_or(deployed)
  }

  /// The copy of the data root this process stands on, by the name its workspaces are told apart by;
  /// none on the machine's own.
  pub fn copy(&self) -> Option<String> {
    if self.roots.own_data_root() {
      return None;
    }

    let data = &self.roots.data;
    let own = data.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();

    Some(if own == "data" {
      data
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
    } else {
      own
    })
  }

  /// What a run apart is started with: the search path, the home this process stands under, every
  /// `BMO_` variable this process carries but the job runner's marker and the store's address, and the
  /// run's own name, which wins. The host starts a run on an emptied environment, and a runner of this
  /// binary finds its data root, its trees and its logins under the home it is handed.
  pub fn run_env(&self, name: &str) -> BTreeMap<String, String> {
    let mut env = BTreeMap::from([("PATH".to_owned(), self.roots.path.clone())]);

    for (variable, value) in &self.vars {
      if variable != STORE_ADDRESS
        && (variable == "HOME" || variable.starts_with("BMO_") && variable != JOB_RUNNER_MARKER)
      {
        env.insert(variable.clone(), value.clone());
      }
    }

    env.insert(RUN_NAME.to_owned(), name.to_owned());
    hand_as_files(&mut env, &self.roots.data);
    env
  }

  /// The machine as the providers ask it: this environment and data root.
  pub fn machine(&self) -> Machine {
    Machine::with_env(self.env.clone(), self.roots.data.clone())
  }

  /// The machine's environment, every variable set to something.
  pub fn vars(&self) -> &BTreeMap<String, String> {
    &self.vars
  }

  /// The environment as this process holds it, a variable set to nothing among it but for the store's
  /// address, which no launched program is handed: what a launch composes a program's environment from.
  pub fn env(&self) -> &BTreeMap<String, String> {
    &self.env
  }

  /// A head pane's command: `pane-head` under a loop that starts it again wherever it ends but on the one
  /// exit it obeys, under the agent's mark. The loop keeps no count of its own: a program the bot ends to
  /// stand another, or a view giving way, ends the head as a crash does, and a loop that stopped on a count
  /// would take the pane, and the workspace whose one pane it is, from a session still taking turns. How
  /// often a program starts again is the agent host's bound (docs/agent-control.md, 「One standing pane per
  /// agent」). The pause between tries is a second, or the seconds `BMO_HEAD_PAUSE` names.
  pub fn head_command(&self, agent: &str, root: &Path) -> Vec<String> {
    vec![
      "/bin/sh".to_owned(),
      "-c".to_owned(),
      format!(
        r#"while :; do "$1" "$2"; code=$?; case $code in {HEAD_STOPPED}) exit 0;; esac; if [ -n "$AGENCY_CONTROL_TOKEN" ]; then data="${{BMO_DATA_DIR:-$HOME/.local/share/bmo/data}}"; last=$(tail -n 1 "$data/agent-places/$2/stderr.log" 2>/dev/null); curl -s -m 3 -X POST --get --data-urlencode "agent=$2" --data-urlencode "code=$code" --data-urlencode "last=$last" -H "Authorization: Bearer $AGENCY_CONTROL_TOKEN" "http://127.0.0.1:$AGENCY_CONTROL_PORT/pane-deaths" >/dev/null 2>&1; fi; sleep "${{BMO_HEAD_PAUSE:-1}}"; done"#
      ),
      format!("{HEAD_MARK}{agent}"),
      root.join("bin").join("pane-head").to_string_lossy().into_owned(),
      agent.to_owned(),
    ]
  }

  /// The home of a name under a label, on this machine's herdr; none where this machine names no herdr.
  pub fn home_of(&self, agent: &str, label: &str) -> Option<Home> {
    let copy = self.copy();
    let root = self.code_root();

    Some(Home {
      agent: agent.to_owned(),
      resting: None,
      label: match &copy {
        None => label.to_owned(),
        Some(name) => fill_marks(&PLACE_WORDS.copy_label, &[("label", label), ("name", name)]),
      },
      data: copy.map(|_| self.roots.data.to_string_lossy().into_owned()),
      head: Launch {
        command: self.head_command(agent, &root),
        cwd: self.pane_dir(),
        env: self.run_env(&format!("{HEAD_MARK}{agent}")),
      },
      herdr: self.herdr.clone()?,
    })
  }

  /// An agent's home by its row: under the label herdr shows it by; on a work node, in that node's herdr,
  /// its head the node's own `pane-head` on the node's roots.
  pub async fn home_of_row(&self, row: &AgentRow) -> Result<Option<Home>, Refusal> {
    self.home_on(&row.name, row.shown(), row.node.as_deref()).await
  }

  /// An agent's home under the label given, on this machine where the node is none; on a node, in that node's
  /// herdr, its head the node's own `pane-head` on the node's roots.
  pub async fn home_on(
    &self,
    agent: &str,
    label: &str,
    node: Option<&str>,
  ) -> Result<Option<Home>, Refusal> {
    let Some(mut home) = self.home_of(agent, label) else {
      return Ok(None);
    };
    let Some(node) = node.map(str::to_owned) else { return Ok(Some(home)) };

    home.herdr = Herdr::there(self.roots.data.join("nodes").join(&node).join("herdr.sock"))
      .within(node_herdr_ceiling());

    let workplace = one(
      &self.db,
      r#"SELECT "root", "home" FROM "node_workplaces" WHERE "node" = $1"#,
      vec![node.as_str().into()],
    )
    .await?;
    let Some(workplace) = workplace else { return Ok(Some(home)) };
    let (root, node_home): (String, Option<String>) =
      (workplace.try_get("", "root")?, workplace.try_get("", "home")?);
    let Some(node_home) = node_home else { return Ok(Some(home)) };
    let node_root = PathBuf::from(links::node_path(&root, Some(&node_home)));
    let code = node_root.join("deploy");
    let at = |path: &Path| path.to_string_lossy().into_owned();

    home.head = Launch {
      command: self.head_command(agent, &code),
      cwd: at(&node_root.join("data")),
      env: BTreeMap::from([
        ("HOME".to_owned(), node_home.clone()),
        (
          "PATH".to_owned(),
          links::path_head(&node_home, &at(&node_root))
            .into_iter()
            .chain(["/usr/local/bin", "/usr/bin", "/bin"].map(str::to_owned))
            .collect::<Vec<_>>()
            .join(":"),
        ),
        ("LANG".to_owned(), self.vars.get("LANG").cloned().unwrap_or_else(|| "C.UTF-8".into())),
        ("BMO_DATA_DIR".to_owned(), at(&node_root.join("data"))),
        ("BMO_SOURCE_DIR".to_owned(), at(&code)),
        ("BMO_HOME_BRIDGE".to_owned(), at(&node_root.join("data").join("bridge.sock"))),
        ("BMO_NODE".to_owned(), node.clone()),
        (RUN_NAME.to_owned(), format!("{HEAD_MARK}{agent}")),
      ]),
    };

    Ok(Some(home))
  }

  /// Whether what stands on a node can stand now: anything of this machine's, and on a node, once the node's
  /// link has been heard from.
  pub fn node_heard(&self, node: Option<&str>) -> bool {
    node.is_none_or(|node| links::link_heard(&self.roots.data, node))
  }

  /// The directory a worker works in: its own, or the source root.
  pub fn checkout_of(&self, row: &AgentRow) -> String {
    row.cwd.clone().unwrap_or_else(|| self.source.to_string_lossy().into_owned())
  }
}

/// How long a call to a node's herdr is waited on before it is answered as herdr silent (docs/work-nodes.md,
/// 「When a node drops」).
pub fn node_herdr_ceiling() -> std::time::Duration {
  std::time::Duration::from_millis(policy::resident_turn::RESIDENT_TURN.node_herdr_ceiling_ms)
}
