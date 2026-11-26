//! A resident program's session token: issued by the tool
//! host for a record naming the line, the role of the agent it is issued for, the capabilities the line
//! declares with the bundles that role gives, the namespaces it may use and the entries it shows by name,
//! the checkout, the data root and archive, and the place each call reads its run
//! from; carried in the program's own environment and taken back once no written program carries it.
//!
//! The records, the reads of a namespace and the paragraph on finding and using tools go through the tool
//! host's published client (`bmo-pocket-client`), with the issuer token the host keeps under the data root.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bmo_pocket_client::{Failure, Granted, Pocket, Runner, SessionRequest};
use policy::roles::ROLES;
use policy::run_lines::RUN_LINES;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::fault;
use crate::Refusal;
use crate::person_nodes::{Caller, acting_for};
use crate::places::Ground;

/// The variable a program carries its token in, which every configuration's bearer header reads.
pub const TOKEN_VARIABLE: &str = "BMO_MCP_TOKEN";

/// The variable a launcher may name the issuer token file in, where a deployment keeps it elsewhere.
pub const ISSUER_TOKEN_VARIABLE: &str = "BMO_MCP_HOST_ISSUER_TOKEN";

// How often a launch asks again while the host is restarting under a deployment: it waits for the host to
// answer however long that takes.
const RETRY_EVERY: Duration = Duration::from_millis(200);

/// The tool host's control socket: the one the environment names, or `bmo/mcp-host.sock` under the
/// user's runtime directory.
pub fn host_socket(ground: &Ground) -> PathBuf {
  host_socket_in(ground.vars())
}

/// The tool host's control socket as these variables name it.
pub fn host_socket_in(vars: &BTreeMap<String, String>) -> PathBuf {
  if let Some(named) = vars.get("BMO_MCP_HOST_SOCKET") {
    return PathBuf::from(named);
  }

  let runtime = vars.get("XDG_RUNTIME_DIR").map_or_else(
    // SAFETY: `getuid` reads the process's own user id and cannot fail.
    || PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })),
    PathBuf::from,
  );

  runtime.join("bmo").join("mcp-host.sock")
}

/// The issuer token a launcher asks the host with: the one the environment names, else the host's own
/// under the data root, which it makes at its first start.
fn issuer_token(ground: &Ground) -> Option<String> {
  ground
    .vars()
    .get(ISSUER_TOKEN_VARIABLE)
    .filter(|token| !token.is_empty())
    .cloned()
    .or_else(|| std::fs::read_to_string(ground.roots.data.join("tool-host").join("issuer-token")).ok())
    .map(|token| token.trim().to_owned())
    .filter(|token| !token.is_empty())
}

// The client the issuer's token reaches the host with, where the token stands.
fn pocket(ground: &Ground, token: String) -> Pocket {
  Pocket::at(host_socket(ground), token)
}

/// What a namespace's owner says of it beside its tools, as the tool host keeps it, or none where it says
/// nothing or the host does not answer.
pub async fn facts(ground: &Ground, namespace: &str) -> Option<Value> {
  let socket = host_socket(ground);
  let Some(token) = issuer_token(ground) else {
    eprintln!("mcp-host at {}: no {namespace} facts: the issuer token is not there", socket.display());
    return None;
  };

  match pocket(ground, token).namespace(namespace).await {
    Ok(namespace) => Some(Value::Object(namespace.facts)),
    Err(Failure::Refused { status, .. }) => {
      eprintln!("mcp-host at {}: no {namespace} facts ({status})", socket.display());
      None
    }
    Err(why) => {
      eprintln!("mcp-host at {}: no {namespace} facts: {why}", socket.display());
      None
    }
  }
}

/// The namespaces the tool host saw a client of this session token reach since the token was issued, or none
/// where it saw none, which a host started since then answers as well, or does not answer.
pub async fn connected(ground: &Ground, token: &str) -> Option<Vec<String>> {
  let socket = host_socket(ground);
  let issuer = issuer_token(ground)?;

  match pocket(ground, issuer).tool_session(token).await {
    Ok(session) => {
      let reached: Vec<String> = session.connected.into_iter().map(|reached| reached.namespace).collect();

      (!reached.is_empty()).then_some(reached)
    }
    Err(why) => {
      eprintln!("mcp-host at {}: the session's servers were not read: {why}", socket.display());
      None
    }
  }
}

/// The holder of the standing record a session token names, as the tool host answers it to the issuer: none
/// where the host keeps no such record, the issuer token is not there, or the host does not answer.
pub async fn holder_of(ground: &Ground, token: &str) -> Option<String> {
  let issuer = issuer_token(ground)?;

  match pocket(ground, issuer).tool_session(token).await {
    Ok(session) => Some(session.holder_name),
    Err(Failure::Refused { .. }) => None,
    Err(why) => {
      eprintln!("mcp-host at {}: a session token was not read: {why}", host_socket(ground).display());
      None
    }
  }
}

/// The tool host's paragraph on finding and using tools, as it words it, or why the host gave none: the
/// issuer token is not there, the socket does not answer, or the host refused.
pub async fn discipline(ground: &Ground) -> Result<String, String> {
  let socket = host_socket(ground);
  let Some(token) = issuer_token(ground) else {
    return Err(format!("mcp-host at {}: the issuer token is not there", socket.display()));
  };

  pocket(ground, token)
    .discipline()
    .await
    .map_err(|why| format!("mcp-host at {}: {why}", socket.display()))
}

/// What a record issued for an agent on a line reaches: the role it holds, its capabilities, the one road the
/// supervisor's own row names where it is a supervisor's record, and the namespaces and the entries shown by name.
struct Reach {
  role: Option<String>,
  capabilities: Vec<String>,
  standing: Option<crate::order::Entry>,
  namespaces: Vec<String>,
  shown: Vec<String>,
}

/// What a record issued for `agent` on `line` reaches (docs/agent-roles.md, 「Where the role is read」): the line's
/// capabilities, then the bundles the run's role gives — or the supervisor bundles for a supervisor, with the
/// capabilities that operate the platform where it is the administrator's (docs/person-nodes.md, 「Who reaches a
/// node」) — each once and narrowed by a supervisor's settings, and the line's namespaces and shown entries, with
/// the tool host's entries that stand in for the line's servers where a supervisor's program stands on a node
/// (docs/person-nodes.md, 「A supervisor's tools on a node」).
async fn capabilities(
  ground: &Ground,
  line: &str,
  agent: &str,
  checkout: &str,
  run_role: Option<&str>,
) -> Result<Reach, Refusal> {
  // The role a run names holds its own bundles; a thrown-away session's place holds its session's role; a
  // supervisor holds the supervisor bundles; a checkout's run and a branch place hold the job role's
  // (docs/agent-roles.md, 「Where the role is read」).
  let named = match crate::apart_places::session_agent(&ground.roots.data, agent) {
    Some(of_session) => of_session,
    None => agent.to_owned(),
  };
  let held = match run_role {
    Some(role) => crate::agent_roles::holds(&ground.db, role).await?,
    None => false,
  };
  let standing = match held {
    true => None,
    false => crate::supervisors::named(&ground.db, &named).await?,
  };
  let supervisor = standing.is_some();
  let role = if held {
    run_role.map(str::to_owned)
  } else if supervisor {
    None
  } else if crate::agent_roles::holds(&ground.db, &named).await? {
    Some(named.clone())
  } else if crate::places::branch::place_of(&ground.roots.data, checkout).is_some()
    || crate::queue::workers::repository_of(&ground.db, &named).await?.is_some()
  {
    Some(ROLES.job.clone())
  } else {
    None
  };
  let bundles = match role.as_deref() {
    Some(role) => crate::role_agents::bundles_at(&ground.db, role, Some(checkout)).await?,
    None if supervisor => ROLES.supervisor.clone(),
    None => vec![],
  };
  // The capabilities that operate the platform, on a line that hands its runs tools, where the supervisor is the
  // administrator's.
  let operating = match supervisor && !RUN_LINES.capabilities(line).is_empty() {
    true => {
      let caller = Caller {
        agent: &named,
        role: None,
        checkout: None,
      };

      acting_for(&ground.db, &ground.roots.data, caller).await?.capabilities()
    }
    false => vec![],
  };
  let mut held: Vec<String> = Vec::new();

  for capability in RUN_LINES.capabilities(line).iter().chain(&bundles).chain(&operating) {
    if !held.contains(capability) {
      held.push(capability.clone());
    }
  }

  let mut namespaces: Vec<String> = RUN_LINES.namespaces(line).to_vec();
  let mut shown: Vec<String> = RUN_LINES.shown(line).to_vec();

  // A supervisor's settings close what its brain switch and its tool limit close (docs/supervisors.md, 「Its
  // settings」), and where its program stands on a node its record reaches the tool host's entries that carry the
  // work of the servers its configuration leaves out there.
  if supervisor {
    let limits = crate::supervisor_limits::Limits::of(&ground.db, &named).await?;

    held = limits.narrowed(held);

    for hosted in limits.hosted(line) {
      for capability in &hosted.capabilities {
        if !held.contains(capability) {
          held.push(capability.clone());
        }
      }
    }

    namespaces = limits.namespaces(line);
    shown = limits.shown(line);
  }

  Ok(Reach {
    role,
    capabilities: held,
    standing: standing.and_then(|one| one.road()),
    namespaces,
    shown,
  })
}

// The record's `reads` label: the media the model the run goes out on reads natively, written under the
// road its caller names, or under the one its standing supervisor's row names where it names none, and left
// off the record entirely where the run reads nothing (docs/provider-order.md, 「The media a model reads」).
async fn reads_label(
  ground: &Ground,
  road: Option<(&str, &str)>,
  standing: Option<crate::order::Entry>,
  labels: &mut Map<String, Value>,
) {
  let (provider, model) = match road {
    Some((provider, model)) => (provider.to_owned(), model.to_owned()),
    None => match standing {
      Some(entry) => (entry.provider, entry.model),
      None => return,
    },
  };
  let media = crate::road::reads(ground, &provider, &model).await;

  if !media.is_empty() {
    labels.insert("reads".to_owned(), json!(media.iter().map(|media| media.word()).collect::<Vec<_>>()));
  }
}

// A record for a run held by `holder`, with the namespaces, capabilities and shown entries it reaches, the grants
// its caller holds, the data root every call reads, and the labels agency's tools read.
fn record(
  holder: &str,
  reach: Reach,
  grants: Vec<String>,
  variables: BTreeMap<String, String>,
  labels: Map<String, Value>,
  ttl_ms: Option<u64>,
) -> SessionRequest {
  SessionRequest {
    holder_name: holder.to_owned(),
    namespaces: reach.namespaces.into_iter().collect(),
    capabilities: reach.capabilities.into_iter().collect(),
    grants: grants.into_iter().map(|resource| Granted { resource }).collect(),
    shown: reach.shown.into_iter().collect(),
    // A run reaches every entry its lines' namespaces and capabilities give it.
    entries: None,
    variables,
    variables_file: None,
    // Named by the issue that resolves the apps the record's person reaches, where it acts for one.
    withheld_apps_file: None,
    // Named by the issue that judges the account its caller acts for (「owned」).
    owner_account: None,
    runner: None,
    secret_space: None,
    labels,
    ttl_ms,
  }
}

/// The variable a record turns the platform's base off by for its brain tools' readings (the brain's
/// `docs/boundary-api.md`, 「Its tool」).
pub const BRAIN_BASE: &str = "BMO_BRAIN_BASE";

/// The delegation chain's kind saying whether the platform's base is read for a person, which the brain's
/// deployment declares (the brain's `docs/design.md`, 「The layers a reading takes」).
const BASE_LAYER: &str = "brain.global";

// The record's variables told to turn the base off where it acts for a person whose chain says the base is not read
// for them (docs/tools.md, 「A tool's folder」); nothing where it reads, where the kind is not declared or
// where the account service cannot be asked.
async fn base_off(ground: &Ground, person: Option<&str>, variables: &mut BTreeMap<String, String>) {
  let Some(person) = person.and_then(|person| person.parse::<i64>().ok()) else {
    return;
  };

  match crate::chain::resolve(
    &crate::apps::service(ground),
    BASE_LAYER,
    crate::chain::Subject::Person(person),
  )
  .await
  {
    Ok(answer) if !crate::chain::flag(answer.as_ref(), true) => {
      variables.insert(BRAIN_BASE.to_owned(), "off".to_owned());
    }
    Ok(_) => {}
    Err(why) => {
      eprintln!("agency: whether account {person} reads the platform's base was not resolved: {why}")
    }
  }
}

// The record named the file its person's withheld apps stand in, written first, where it acts for a person's
// account; and for the administrator's account, the file of the MCP servers its holder is not attached to, named
// whatever was written (docs/tools.md, 「The apps a person reaches」).
async fn withhold(
  ground: &Ground,
  request: &mut SessionRequest,
  person: Option<String>,
  issued: &crate::apps::Issued,
  file: impl FnOnce() -> PathBuf,
) {
  if let Some(person) = person {
    let file = file();

    crate::apps::write_for(ground, &file, &person, issued).await;
    request.withheld_apps_file = Some(file);
    return;
  }

  let Some(account) = request.owner_account.clone() else {
    return;
  };

  if !crate::account_isolation::owners_own(&ground.db, &account).await.unwrap_or(false) {
    return;
  }

  let file = file();

  crate::apps::write_for_administrator(ground, &file, &account, issued).await;
  request.withheld_apps_file = Some(file);
}

// Whom a record is issued for: the holder its run is let in on or stands for, and its line.
async fn issued_for(
  ground: &Ground,
  line: &str,
  agent: &str,
  role: Option<&str>,
  folder: &str,
) -> crate::apps::Issued {
  let holder = crate::attachments::holder_of_run(ground, agent, role, Some(folder))
    .await
    .unwrap_or_else(|why| {
      eprintln!("agency: the holder of {agent}'s record was not read: {}", why.detail);
      None
    });

  crate::apps::Issued {
    holder,
    line: Some(line.to_owned()),
  }
}

// The record named the account its caller acts for, the runner that account's personal tools run on and the folder
// their secrets are read from (docs/tools.md, 「The owner and the runner a record names」): the account a person's
// caller acts for, and the administrator's for a supervisor holding the administrator's grant; none for the
// deployment's own callers. The administrator's tools run on this machine, under this deployment's node link; a
// person's on their PC, through its link's runner socket, where the PC's link spec names the account as the one it
// runs personal tools for; and an account on a shared PC or with no PC is named no runner.
async fn owned(ground: &Ground, request: &mut SessionRequest, caller: Caller<'_>) -> Result<(), Refusal> {
  let acting = acting_for(&ground.db, &ground.roots.data, caller).await?;
  let account = match (acting.account, acting.administrator) {
    (Some(account), _) => Some(account),
    (None, true) => crate::account_isolation::administrator(&ground.db).await?,
    (None, false) => None,
  };
  // The folder the record is handed secrets from: the unit's where the caller's agent row is filed under a
  // unit, whatever account it otherwise acts for; else the account's own space; else none, and the record
  // names no owner.
  let space = match (acting.unit, &account) {
    (Some(unit), _) => Some(unit_secret_space(ground, unit)),
    (None, Some(account)) => Some(secret_space_of(ground, account).await?),
    _ => None,
  };

  if space.is_none() && account.is_none() {
    return Ok(());
  }

  request.secret_space = space;

  if let Some(account) = &account {
    request.runner = runner_of(ground, account).await?;
    request.owner_account = Some(account.clone());
  }

  Ok(())
}

/// The folder an organisation unit's secrets stand in, which the runs of agents filed under it are handed the
/// secrets they name from (docs/accounts.md, 「The secret store」): the console's secret store's space of the
/// unit, the data root's `env/units/<unit>/`.
pub fn unit_secret_space(ground: &Ground, unit: i64) -> PathBuf {
  ground.roots.data.join("env").join("units").join(unit.to_string())
}

/// The folder an account's secrets stand in, which its personal tools are handed the secrets they name from
/// (docs/person-nodes.md, 「The network and secrets of a call」): the console's secret store's space of the person,
/// the data root's `env/users/<account>/`, and the administrator's, the data root's `env/` itself (bmo-web's
/// `docs/accounts.md`, 「The secret store」).
pub async fn secret_space_of(ground: &Ground, account: &str) -> Result<PathBuf, Refusal> {
  let env = ground.roots.data.join("env");

  if crate::account_isolation::owners_own(&ground.db, account).await? {
    return Ok(env);
  }

  Ok(env.join("users").join(account))
}

/// The runner an account's personal tools run on (docs/person-nodes.md, 「Personal tools on a PC」): the
/// administrator's on this machine, as a command of this deployment's node link over the data root's own folder of
/// them; a person's through the runner socket of their PC's link, where its spec names them as the one account it
/// runs personal tools for; none otherwise.
pub async fn runner_of(ground: &Ground, account: &str) -> Result<Option<Runner>, Refusal> {
  let roots = &ground.roots;

  if crate::account_isolation::owners_own(&ground.db, account).await? {
    let words = [
      roots.deploy.join("bin").join("node-link").to_string_lossy().into_owned(),
      "runner".to_owned(),
      "--root".to_owned(),
      roots.data.join(node_link::runner::PERSONAL_TOOLS).to_string_lossy().into_owned(),
      "--owner".to_owned(),
      account.to_owned(),
      "--kit".to_owned(),
      roots.deploy.join(node_link::runner::SCREEN_KIT).to_string_lossy().into_owned(),
    ];

    return Ok(Some(Runner::Command(words.to_vec())));
  }

  let Some(node) = crate::person_nodes::node_of(&ground.db, account).await? else {
    return Ok(None);
  };
  let spec = node_link::spec::Spec::read(&roots.spec_path(&node)).ok();

  Ok(spec.and_then(|spec| match (spec.runner, spec.runner_owner) {
    (Some(socket), Some(owner)) if owner == account => Some(Runner::Socket(socket)),
    _ => None,
  }))
}

/// How long a screen session stands (docs/api.md, 「personal-apps」).
pub const SCREEN_SESSION_MS: u64 = 10 * 60 * 1000;

/// The holder a screen session is held under, which names its account.
pub const SCREEN_HOLDER: &str = "personal-screens";

/// A short-lived session a console reads and calls `account`'s personal screens under (docs/person-nodes.md,
/// 「Personal screens」): the namespace `my` alone, no capability and no grant, the account and the runner its
/// records name, and the file of the apps the person does not reach; its token and when it lapses, or none where
/// the account has no runner.
pub async fn issue_screen_session(
  ground: &Ground,
  account: &str,
) -> Result<Option<(String, u64)>, Refusal> {
  let Some(runner) = runner_of(ground, account).await? else {
    return Ok(None);
  };
  let now = u64::try_from(crate::now_ms()).unwrap_or_default();
  let lapses = now + SCREEN_SESSION_MS;
  let mut labels = Map::new();

  labels.insert("account".to_owned(), json!(account));

  let mut request = SessionRequest {
    holder_name: format!("{SCREEN_HOLDER}:{account}"),
    namespaces: ["my".to_owned()].into(),
    capabilities: Default::default(),
    grants: Default::default(),
    shown: Default::default(),
    entries: None,
    variables: BTreeMap::new(),
    variables_file: None,
    withheld_apps_file: None,
    owner_account: Some(account.to_owned()),
    runner: Some(runner),
    secret_space: Some(secret_space_of(ground, account).await?),
    labels,
    ttl_ms: Some(SCREEN_SESSION_MS),
  };

  withhold(ground, &mut request, Some(account.to_owned()), &Default::default(), || {
    crate::apps::run_file(&ground.roots.data, lapses, now)
  })
  .await;

  issue_record(ground, &request).await.map(|token| Some((token, lapses)))
}

/// A token for a resident program of `line` standing in `place` for `agent` and working in `checkout`, or
/// none where the line needs none: one whose record holds no capability. A host that does not answer
/// within its wait, or one that refuses the record, refuses the program. `road` names the provider and model
/// its runs go out on where its caller holds one; none takes the supervisor's own row's road, and none takes
/// no `reads` label at all.
pub async fn issue(
  ground: &Ground,
  line: &str,
  agent: &str,
  place: &Path,
  checkout: &Path,
  road: Option<(&str, &str)>,
) -> Result<Option<String>, Refusal> {
  let mut reach = capabilities(ground, line, agent, &checkout.to_string_lossy(), None).await?;

  if reach.capabilities.is_empty() {
    return Ok(None);
  }

  let role = reach.role.clone();
  let standing = reach.standing.take();

  let data_root = ground.roots.data.to_string_lossy().into_owned();
  let mut variables = BTreeMap::from([("BMO_DATA_DIR".to_owned(), data_root)]);

  variables.extend(line_variables(ground, line));

  // A thrown-away session's place stands no agent row, so its calls name the supervisor its line's brain
  // tools write for, the one agency's research setting names for the agent's account, as every run of the
  // line does (docs/agent-roles.md, 「The voyager and the seeker roles」).
  let account = agent_account(ground, agent).await;

  if crate::apart_places::session_agent(&ground.roots.data, agent).is_some()
    && RUN_LINES.writes_agent_brain(line)
    && let Some(supervisor) = research_supervisor(ground, account).await?
  {
    variables.insert("BMO_TURN_AGENT".to_owned(), supervisor);
  }

  if let Some(supervisor) = brain_supervisor(ground, line, account).await? {
    variables.insert(BRAIN_AGENT.to_owned(), supervisor);
  }

  let mut labels = Map::new();

  labels.insert("line".to_owned(), json!(line));
  labels.insert("checkout".to_owned(), json!(checkout));
  labels.insert("archive".to_owned(), json!(crate::archive_place(&ground.roots.data)));
  labels.insert("place".to_owned(), json!(place));

  let folder = checkout.to_string_lossy();
  let caller = Caller {
    agent,
    role: role.as_deref(),
    checkout: Some(&folder),
  };
  let grants = granted(ground, caller).await?;
  let person = crate::apps::person_of(ground, caller).await?;

  if let Some(role) = &role {
    labels.insert("role".to_owned(), json!(role));
  }

  reads_label(ground, road, standing, &mut labels).await;
  base_off(ground, person.as_deref(), &mut variables).await;

  let mut request = record(agent, reach, grants, variables, labels, None);
  let issued = issued_for(ground, line, agent, role.as_deref(), &folder).await;

  owned(ground, &mut request, caller).await?;
  withhold(ground, &mut request, person, &issued, || crate::apps::place_file(place)).await;
  issue_record(ground, &request).await.map(Some)
}

// The administrator's grants a record for `caller` holds: the policy's where it is a supervisor of no account,
// none for any other (docs/person-nodes.md, 「Who reaches a node」).
async fn granted(ground: &Ground, caller: Caller<'_>) -> Result<Vec<String>, Refusal> {
  Ok(acting_for(&ground.db, &ground.roots.data, caller).await?.grants())
}

/// The deployment's variables the line hands every record of its runs, each the deployment sets.
pub fn line_variables(ground: &Ground, line: &str) -> Vec<(String, String)> {
  RUN_LINES
    .variables(line)
    .iter()
    .filter_map(|name| {
      let value = ground.vars().get(name).map(|value| value.trim()).filter(|value| !value.is_empty())?;

      Some((name.clone(), value.to_owned()))
    })
    .collect()
}

// The variables of a run's own, which its calls read their turn from.
fn turn_variable(name: &str) -> bool {
  ["BMO_RUN_NAME", "BMO_TASK_LOCKED", "BMO_WORK_ORDER_STEP"].contains(&name)
    || name.starts_with("BMO_TURN_")
}

/// The variable a run carries the labels its thrown-away session's caller named in, a JSON object's text,
/// which its record carries under each key (docs/api.md, 「sessions」).
pub const RECORD_LABELS: &str = "BMO_RECORD_LABELS";

/// The variable a run carries the capabilities its thrown-away session's caller lets its record keep in, a
/// JSON list's text.
pub const RECORD_TOOLS: &str = "BMO_RECORD_TOOLS";

/// The variable a role's run carries the role it holds in, whose bundles its record is given whatever the
/// name the run is carried under (docs/agent-roles.md, 「Where the role is read」). It is a turn's variable, so
/// every road hands it to the program its run stands.
pub const RUN_ROLE: &str = "BMO_TURN_ROLE";

/// The variables a role's session's turn carries of its role and of the record its caller named: its labels
/// and the capabilities it keeps, each where the start named it.
pub fn record_variables(apart: &agent_session::Apart) -> Vec<(String, String)> {
  let mut variables = Vec::new();

  if let Some(role) = apart.role.as_deref().filter(|role| !role.is_empty()) {
    variables.push((RUN_ROLE.to_owned(), role.to_owned()));
  }

  if let Some(labels) = &apart.labels {
    variables.push((RECORD_LABELS.to_owned(), labels.clone()));
  }

  if let Some(allowed) = &apart.tools_allowed {
    variables.push((RECORD_TOOLS.to_owned(), json!(allowed).to_string()));
  }

  variables
}

/// The variables a run names the record its caller asked for in, beside its role: its labels and the
/// capabilities it keeps. Every road hands them to the program its run stands.
pub const RECORD_VARIABLES: [&str; 2] = [RECORD_LABELS, RECORD_TOOLS];

/// Whether a run's variables name a record of its own: its role, or labels or the capabilities its caller lets
/// it keep.
pub fn names_record(env: &BTreeMap<String, String>) -> bool {
  [RUN_ROLE]
    .iter()
    .chain(RECORD_VARIABLES.iter())
    .any(|name| env.get(*name).is_some_and(|value| !value.is_empty()))
}

// The labels a run's caller named, an object's keys and values; none where it named none or they do not read
// as an object.
fn named_labels(env: &BTreeMap<String, String>) -> Map<String, Value> {
  env
    .get(RECORD_LABELS)
    .and_then(|text| serde_json::from_str::<Value>(text).ok())
    .and_then(|value| match value {
      Value::Object(labels) => Some(labels),
      _ => None,
    })
    .unwrap_or_default()
}

// The capabilities a run's caller lets its record keep; none where it named no list, which keeps them all.
fn named_tools(env: &BTreeMap<String, String>) -> Option<Vec<String>> {
  env.get(RECORD_TOOLS).and_then(|text| serde_json::from_str::<Vec<String>>(text).ok())
}

/// A token for one run of `line` carried in `agent`'s host, working in `cwd` on `env`, standing until
/// `deadline` at most; none where the line needs none. Its record carries its line's capabilities with its
/// agent's role's bundles, narrowed to those the run's caller lets it keep, and the labels that caller
/// named. The agent is the turn's where the run names one, and the one it is carried for otherwise. `road`
/// names the provider and model the run goes out on, for its record's `reads` label; none leaves that to the
/// standing supervisor's row and then to nothing.
pub async fn issue_for_run(
  ground: &Ground,
  line: &str,
  agent: &str,
  cwd: &str,
  env: &BTreeMap<String, String>,
  road: Option<(&str, &str)>,
  deadline: u64,
) -> Result<Option<String>, Refusal> {
  let given = |name: &str| env.get(name).filter(|value| !value.is_empty()).cloned();
  // The account the carrier's row stands on, read before the turn's agent takes its name (docs/api.md,
  // 「settings」).
  let account = agent_account(ground, agent).await;
  let agent = given("BMO_TURN_AGENT").map_or_else(|| agent.to_owned(), |named| named.trim().to_owned());
  let run_role = given(RUN_ROLE);
  let mut reach = capabilities(ground, line, &agent, cwd, run_role.as_deref()).await?;

  if let Some(allowed) = named_tools(env) {
    reach.capabilities.retain(|capability| allowed.contains(capability));
  }

  if reach.capabilities.is_empty() {
    return Ok(None);
  }

  let role = reach.role.clone();
  let standing = reach.standing.take();

  let data_root = given("BMO_DATA_DIR").unwrap_or_else(|| ground.roots.data.to_string_lossy().into_owned());
  let mut variables: BTreeMap<String, String> = env
    .iter()
    .filter(|(name, _)| turn_variable(name))
    .map(|(name, value)| (name.clone(), value.clone()))
    .collect();

  variables.insert("BMO_DATA_DIR".to_owned(), data_root);
  variables.extend(line_variables(ground, line));

  // A runner line's brain tools write the brain of the supervisor agency's research setting names for the
  // account the run stands on. The record names that supervisor as the turn its tools run for, while the run
  // itself stays the runner's (docs/agent-roles.md, 「The voyager and the seeker roles」).
  if RUN_LINES.writes_agent_brain(line)
    && let Some(supervisor) = research_supervisor(ground, account).await?
  {
    variables.insert("BMO_TURN_AGENT".to_owned(), supervisor);
  }

  if let Some(supervisor) = brain_supervisor(ground, line, account).await? {
    variables.insert(BRAIN_AGENT.to_owned(), supervisor);
  }

  let caller = Caller {
    agent: &agent,
    role: role.as_deref(),
    checkout: Some(cwd),
  };
  let grants = granted(ground, caller).await?;
  let person = crate::apps::person_of(ground, caller).await?;
  let mut labels = Map::new();
  labels.insert("line".to_owned(), json!(line));
  labels.insert("checkout".to_owned(), json!(cwd));
  labels.insert("archive".to_owned(), json!(crate::archive_place(&ground.roots.data)));

  if let Some(role) = &role {
    labels.insert("role".to_owned(), json!(role));
  }

  if let Some(place) = given("BMO_AGENT_PLACE") {
    labels.insert("place".to_owned(), json!(place));
  }

  for (key, value) in named_labels(env) {
    labels.entry(key).or_insert(value);
  }

  reads_label(ground, road, standing, &mut labels).await;
  base_off(ground, person.as_deref(), &mut variables).await;

  let now = u64::try_from(crate::now_ms()).unwrap_or_default();
  let place = given("BMO_AGENT_PLACE");
  let mut request = record(&agent, reach, grants, variables, labels, Some(deadline.saturating_sub(now)));

  let issued = issued_for(ground, line, &agent, role.as_deref(), cwd).await;

  owned(ground, &mut request, caller).await?;

  withhold(ground, &mut request, person, &issued, || match place {
    Some(place) => crate::apps::place_file(Path::new(&place)),
    None => crate::apps::run_file(&ground.roots.data, deadline, now),
  })
  .await;
  issue_record(ground, &request).await.map(Some)
}

/// The token a standing program is handed, issued as it is written into its place: where the run it stands
/// for names a record of its own — its role, labels or the capabilities its caller lets it keep — one issued
/// for that run, and otherwise the place's resident record. The run's variables of `run_env` are laid over
/// the program's own environment first, since a program's environment holds no turn's variables and the role
/// a run holds stands among them: a record issued from the program's environment alone would hold none of
/// its role's bundles. `road` names the provider and model the program's turns go out on, for the record's
/// `reads` label.
#[allow(clippy::too_many_arguments)]
pub async fn issue_for_program(
  ground: &Ground,
  line: &str,
  agent: &str,
  place: &Path,
  checkout: &Path,
  program_env: &BTreeMap<String, String>,
  run_env: &BTreeMap<String, String>,
  road: Option<(&str, &str)>,
  deadline: u64,
) -> Result<Option<String>, Refusal> {
  let mut env = program_env.clone();

  env.extend(
    run_env
      .iter()
      .filter(|(name, _)| turn_variable(name) || RECORD_VARIABLES.contains(&name.as_str()))
      .map(|(name, value)| (name.clone(), value.clone())),
  );

  if names_record(&env) {
    issue_for_run(ground, line, line, &checkout.to_string_lossy(), &env, road, deadline).await
  } else {
    issue(ground, line, agent, place, checkout, road).await
  }
}

// A token asked of the host for a record.
async fn issue_record(ground: &Ground, request: &SessionRequest) -> Result<String, Refusal> {
  let socket = host_socket(ground);

  loop {
    match issuer_token(ground) {
      None => {}
      Some(token) => match pocket(ground, token).issue(request).await {
        Ok(session) => return Ok(session.token),
        Err(Failure::Refused { status, error }) => {
          return Err(host_down(&socket, format!("{status} {} ({})", error.error, error.code.as_str())));
        }
        Err(Failure::Unreadable { status, why }) => {
          return Err(host_down(&socket, format!("{status} {why}")));
        }
        Err(Failure::Unreachable(_)) => {}
      },
    }

    tokio::time::sleep(RETRY_EVERY).await;
  }
}

fn host_down(socket: &Path, why: impl Into<String>) -> Refusal {
  fault(format!(
    "The tool host did not answer at {}, so nothing ran: {}",
    socket.display(),
    why.into()
  ))
}

/// A token taken back; a host that does not answer has lost it already.
pub async fn revoke(ground: &Ground, token: &str) {
  let Some(issuer) = issuer_token(ground) else {
    return;
  };

  let _ = pocket(ground, issuer).revoke(token).await;
}

/// The token a replaced or taken-down resident program carried, given up (docs/sessions.md, 「A restart hands the
/// session over」): taken back at once where no asynchronous call asked under its record still runs, else left
/// standing and looked at again every `look` until none does, then taken back. Such a call wakes its agent by name
/// and room, whichever session stands then, so a deployment or a wait the session going away started
/// runs on and wakes the next one. The waiting goes on beside the caller, which never waits for it.
pub async fn let_go(ground: &Ground, token: &str, look: Duration) {
  let Some(issuer) = issuer_token(ground) else {
    return;
  };
  let socket = host_socket(ground);

  if calls_running(&socket, &issuer, token).await == 0 {
    let _ = Pocket::at(&socket, issuer).revoke(token).await;
    return;
  }

  let token = token.to_owned();

  tokio::spawn(async move {
    loop {
      tokio::time::sleep(look).await;

      if calls_running(&socket, &issuer, &token).await == 0 {
        let _ = Pocket::at(&socket, issuer).revoke(&token).await;
        return;
      }
    }
  });
}

/// How many asynchronous calls asked under the record `token` names the tool host keeps running; none where the
/// host does not answer the read, which leaves nothing to wait for.
async fn calls_running(socket: &Path, issuer: &str, token: &str) -> usize {
  let record = hex::encode(Sha256::digest(token.as_bytes()));
  let read = bmo_pocket_client::transport::request(
    socket,
    reqwest::Method::GET,
    "/uses?state=running",
    Some(issuer),
    None,
  )
  .await;
  let page = read.ok().filter(|reply| reply.status.is_success()).and_then(|reply| reply.json());
  let calls = page.as_ref().and_then(|page| page["items"].as_array());

  calls.map_or(0, |calls| calls.iter().filter(|call| call["session"] == record.as_str()).count())
}

/// The checkout a program's tool session record names: its own, or for a program on a work node the shadow
/// its link keeps of that checkout here (docs/work-nodes.md, 「The link」).
pub fn record_checkout(ground: &Ground, place: &Path, cwd: &str) -> PathBuf {
  match crate::watches::links::place_node(place) {
    None => PathBuf::from(cwd),
    Some(node) => crate::watches::links::checkout_shadow(&ground.roots.data, &node, cwd),
  }
}

/// The variable a run's record names the supervisor by whose brain its brain tools read and write: the one
/// agency's `research_supervisor` setting names, none where it names none (docs/api.md, 「settings」).
pub const BRAIN_AGENT: &str = "BMO_BRAIN_AGENT";

/// The supervisor whose brain a run of `line` works for (docs/agent-roles.md, 「The bundles」): the one
/// agency's research setting names; none for a line that works for no supervisor's brain.
pub async fn brain_supervisor(
  ground: &Ground,
  line: &str,
  account: Option<i64>,
) -> Result<Option<String>, Refusal> {
  if !RUN_LINES.works_for_brain(line) {
    return Ok(None);
  }

  research_supervisor(ground, account).await
}

/// The research supervisor in force for a run standing for `account`: the nearest link of that person's path
/// on the chain names it where one says so; else the platform's word, else the settings row that stood until
/// the value moved (docs/api.md, 「settings」).
pub async fn research_supervisor(ground: &Ground, account: Option<i64>) -> Result<Option<String>, Refusal> {
  let standing =
    crate::chain_settings::of(&policy::organisation::ORGANISATION.settings.research_supervisor.key)
      .ok_or_else(|| Refusal::failed("the research supervisor does not stand on the chain"))?;

  if let Some(account) = account
    && let Some(name) = crate::chain_settings::for_person(ground, standing.key, account)
      .await
      .map_err(Refusal::failed)?
  {
    return Ok(Some(name));
  }

  crate::chain_settings::text(ground, &standing).await.map_err(Refusal::failed)
}

// The account a named agent sits on: the supervisor's or the seat's row, none where it is the platform's own.
pub async fn agent_account(ground: &Ground, agent: &str) -> Option<i64> {
  if let Ok(Some(held)) = crate::supervisors::named(&ground.db, agent).await {
    return held.owner_account;
  }

  crate::role_agents::named(&ground.db, agent)
    .await
    .ok()
    .flatten()
    .and_then(|held| held.owner_account)
}

/// The settings key the research supervisor stands under.
pub const RESEARCH_SUPERVISOR: &str = "research_supervisor";
