//! A Claude account signed in from the console (docs/cache-fallback.md, 「Signing an account in from the
//! console」): the CLI's own sign-in started with the account's configuration directory as its own, the address
//! it prints handed back, and the code the owner pastes handed to it. The CLI makes the login; nothing of the
//! exchange, and never the code, is kept, logged or answered. A person's sign-in runs on their node, in the
//! node's herdr session, its words read off the pane it runs in (docs/supply.md, 「Signing a person's login
//! in」).

use std::collections::{BTreeMap, HashMap};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use policy::providers::claude_code::CLAUDE_CODE;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;

use super::claude_account_changes::{self as changes, BasePromptGiven, CredentialGiven, Owning};
use super::claude_accounts::{CREDENTIAL_FILE, Credential, CredentialKind, Judging, accounts};
use super::fallback::accounts_answer;
use super::{Call, Refused, Route, answer};
use crate::access::Acting;

/// The variables a sign-in's CLI is never handed: a login the launching process carries would answer for the
/// sign-in in place of the one being made.
const CARRIED_LOGINS: [&str; 4] = [
  "CLAUDE_CODE_OAUTH_TOKEN",
  "ANTHROPIC_API_KEY",
  "ANTHROPIC_AUTH_TOKEN",
  "CLAUDE_CONFIG_DIR",
];

/// What a sign-in is for: an account there, signed in again in its own directory, or one added once its
/// directory holds a login.
enum Target {
  Again { name: String, home: PathBuf },
  New { name: String, home: PathBuf },
}

impl Target {
  fn home(&self) -> &Path {
    match self {
      Self::Again { home, .. } | Self::New { home, .. } => home,
    }
  }

  /// The name of the account it signs in.
  fn name(&self) -> &str {
    match self {
      Self::Again { name, .. } | Self::New { name, .. } => name,
    }
  }

  /// A sign-in that ends with no account added takes a new account's directory away whole: the sign-in made it,
  /// empty, and the CLI writes its own files there as it starts, so a name tried again finds no directory standing.
  fn abandon(&self) {
    if let Self::New { home, .. } = self {
      let _ = std::fs::remove_dir_all(home);
    }
  }
}

/// One sign-in under way: the CLI running it, what it has said so far, and what it is for. The CLI is ended
/// when the sign-in is dropped.
struct SignIn {
  cli: Child,
  input: Option<ChildStdin>,
  said: Arc<std::sync::Mutex<String>>,
  target: Target,
  started: Instant,
}

/// One person's sign-in under way on their node: the pane its CLI runs in there, and the account it adds once
/// the login took. Nothing is left of it here once its pane is closed.
struct NodeSignIn {
  pane: NodePane,
  user: i64,
  node: String,
  /// The person's own account signed in again, by its identifier, or none for a new one added once the login took.
  again: Option<String>,
  name: String,
  /// The configuration directory on the node the CLI writes the login into.
  home: String,
  started: Instant,
}

/// A sign-in held: the owner's, on this machine, or a person's, on their node.
enum Held {
  Here(SignIn),
  OnNode(NodeSignIn),
}

impl Held {
  fn started(&self) -> Instant {
    match self {
      Self::Here(sign_in) => sign_in.started,
      Self::OnNode(sign_in) => sign_in.started,
    }
  }

  /// Whether the caller may hand it its code or give it up: the platform's caller the owner's, and a person
  /// their own alone.
  fn held_by(&self, acting: Acting) -> bool {
    match (self, acting) {
      (Self::Here(_), Acting::Platform) => true,
      (Self::OnNode(sign_in), Acting::Person(user)) => sign_in.user == user,
      _ => false,
    }
  }

  /// The sign-in ended where it stands: the CLI here ended and a new account's directory taken away, and the
  /// pane on the node closed.
  fn end(self) {
    match self {
      Self::Here(SignIn { cli, target, .. }) => {
        drop(cli);
        target.abandon();
      }
      Self::OnNode(sign_in) => {
        tokio::spawn(async move { sign_in.pane.close().await });
      }
    }
  }
}

static SIGN_INS: LazyLock<Mutex<HashMap<String, Held>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn words() -> &'static policy::control_words::LoginsWords {
  &CONTROL_WORDS.logins
}

// What the CLI writes, kept as it comes, so the address and a refusal are read from it.
fn listen(mut from: impl AsyncRead + Unpin + Send + 'static, said: Arc<std::sync::Mutex<String>>) {
  tokio::spawn(async move {
    let mut chunk = [0u8; 4096];
    while let Ok(read) = from.read(&mut chunk).await {
      if read == 0 {
        break;
      }
      said
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push_str(&String::from_utf8_lossy(&chunk[..read]));
    }
  });
}

/// What the CLI said as a person reads it: its terminal escapes, the hyperlinks among them, taken out.
pub fn plain(said: &str) -> String {
  String::from_utf8_lossy(&strip_ansi_escapes::strip(said.as_bytes())).into_owned()
}

/// The sign-in's address among what the CLI said: the first word of it that reads as an https address. The
/// CLI's words are its own shape, no general format, and the address is checked by the URL reader.
pub fn sign_in_address(said: &str) -> Option<String> {
  plain(said)
    .split_whitespace()
    .filter_map(|word| word.find("https://").map(|at| &word[at..]))
    .find_map(|word| url::Url::parse(word).ok().filter(|address| address.scheme() == "https"))
    .map(String::from)
}

/// The CLI's last words, as a refusal says them, with the code taken out wherever it stands.
pub fn last_words(said: &str, code: Option<&str>) -> String {
  let line = plain(said)
    .lines()
    .map(str::trim)
    .rev()
    .find(|line| !line.is_empty())
    .map(str::to_owned)
    .unwrap_or_else(|| words().silent.clone());
  match code.map(str::trim).filter(|code| !code.is_empty()) {
    Some(code) => line.replace(code, "…"),
    None => line,
  }
}

/// A new account's directory under the data root's env directory, named by the account's name: its letters
/// and digits, dots, underscores and hyphens, every other character a hyphen.
pub fn new_home(data_root: &Path, name: &str) -> PathBuf {
  let named: String = name
    .trim()
    .to_lowercase()
    .chars()
    .map(|letter| if letter.is_ascii_alphanumeric() || ".-_".contains(letter) { letter } else { '-' })
    .collect();
  data_root.join("env").join(named.trim_matches(|letter| ".-_".contains(letter)))
}

// Every sign-in that has stood longer than the policy allows, ended.
fn sweep(held: &mut HashMap<String, Held>) {
  let keep = Duration::from_secs(CLAUDE_CODE.console_login.keep_minutes * 60);
  let old: Vec<String> = held
    .iter()
    .filter(|(_, sign_in)| sign_in.started().elapsed() >= keep)
    .map(|(id, _)| id.clone())
    .collect();

  for id in old {
    if let Some(sign_in) = held.remove(&id) {
      sign_in.end();
    }
  }
}

// The sign-in a call names taken out of those held, where the caller holds it.
async fn take(login: &str, acting: Acting) -> Result<Held, Refused> {
  let mut held = SIGN_INS.lock().await;

  sweep(&mut held);

  match held.get(login) {
    Some(sign_in) if sign_in.held_by(acting) => Ok(held.remove(login).expect("the sign-in is held")),
    _ => Err(no_sign_in(login)),
  }
}

// A sign-in held under a new identifier, answered as the start answers it: the identifier, the address and the
// name of the account it signs in.
async fn hold(sign_in: Held, address: String, name: &str) -> Response {
  let login = uuid::Uuid::new_v4().to_string();
  let mut held = SIGN_INS.lock().await;

  sweep(&mut held);
  held.insert(login.clone(), sign_in);

  answer(StatusCode::CREATED, json!({ "login": login, "url": address, "name": name }))
}

/// The name a new login takes where its start names none: `claude-NN`, one past the highest number among the
/// names given, which are every account's, whoever owns it.
pub fn next_login_name(names: &[&str]) -> String {
  let highest = names
    .iter()
    .filter_map(|name| name.trim().strip_prefix("claude-"))
    .filter(|number| !number.is_empty() && number.bytes().all(|digit| digit.is_ascii_digit()))
    .filter_map(|number| number.parse::<u64>().ok())
    .max()
    .unwrap_or(0);

  format!("claude-{:02}", highest + 1)
}

/// Why a new login cannot take `name` beside the names every account holds, said under the `name` field: 409
/// `name-taken` for a name another account holds, 400 for one that is no name.
pub fn name_refusal(name: &str, names: &[&str]) -> Option<Refused> {
  let fault = changes::name_fault(name, names)?;
  let refused = if names.contains(&name.trim()) {
    Refused::new(StatusCode::CONFLICT, fault).with_code("name-taken")
  } else {
    Refused::failed(fault)
  };

  Some(refused.with_field("name"))
}

// The refusal of a sign-in whose CLI gave no address within the bound, saying what it said.
fn no_link(said: &str) -> Refused {
  Refused::new(
    StatusCode::UNPROCESSABLE_ENTITY,
    fill_marks(&words().no_link, &[("said", &last_words(said, None))]),
  )
  .with_code("no-link")
}

fn text<'a>(body: &'a Value, field: &str) -> Option<&'a str> {
  body.get(field).and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty())
}

// The refusal of a start naming both an account there and a new one's name.
fn which() -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, words().which.clone()).with_code("account-or-name")
}

// The refusal of a start naming an account the caller has none of.
fn unknown_account(id: &str) -> Refused {
  Refused::new(StatusCode::NOT_FOUND, fill_marks(&CONTROL_WORDS.accounts.unknown, &[("id", id)]))
    .with_code("unknown-account")
}

// The refusal of a start naming an account that keeps no login of its own.
fn not_directory(name: &str) -> Refused {
  Refused::new(StatusCode::UNPROCESSABLE_ENTITY, fill_marks(&words().not_directory, &[("name", name)]))
    .with_code("not-a-directory")
}

// The name a new login takes: the one the start gives, where no other account holds it and it can be a name, or
// the next free `claude-NN` past every account's names whose directory under `data_root`, where one is given, holds
// nothing yet.
pub fn new_login_name(
  given: Option<&str>,
  names: &[&str],
  data_root: Option<&Path>,
) -> Result<String, Refused> {
  if let Some(name) = given {
    return match name_refusal(name, names) {
      Some(refused) => Err(refused),
      None => Ok(name.to_owned()),
    };
  }

  let mut passed: Vec<&str> = names.to_vec();
  let mut tried = Vec::new();

  loop {
    let name = next_login_name(&passed);
    let free = data_root.is_none_or(|root| !holds_files(&new_home(root, &name)));

    if free {
      return Ok(name);
    }

    tried.push(name);
    passed = names.iter().copied().chain(tried.iter().map(String::as_str)).collect();
  }
}

fn holds_files(directory: &Path) -> bool {
  std::fs::read_dir(directory)
    .map(|mut entries| entries.next().is_some())
    .unwrap_or(false)
}

// What a start's body names: an account there, which must be a directory account, or a new one's name, whose
// directory must be empty or not yet made, or neither, for a new one agency names.
async fn target_of(db: &DatabaseConnection, data_root: &Path, body: &Value) -> Result<Target, Refused> {
  let held = accounts(db).await?;

  match (text(body, "account"), text(body, "name")) {
    (Some(_), Some(_)) => Err(which()),
    (Some(id), None) => {
      let account = held.iter().find(|one| one.id == id).ok_or_else(|| unknown_account(id))?;
      match &account.credential {
        // A person's login stands on their node, where alone it is signed in again.
        Credential::Directory { home } if account.node.is_none() => Ok(Target::Again {
          name: account.name.clone(),
          home: home.clone(),
        }),
        _ => Err(not_directory(&account.name)),
      }
    }
    (None, given) => {
      let names: Vec<&str> = held.iter().map(|one| one.name.as_str()).collect();
      let name = new_login_name(given, &names, Some(data_root))?;
      let home = new_home(data_root, &name);
      if holds_files(&home) || home == data_root.join("env") {
        return Err(
          Refused::new(
            StatusCode::CONFLICT,
            fill_marks(&words().directory_taken, &[("path", &home.to_string_lossy())]),
          )
          .with_code("directory-taken"),
        );
      }
      Ok(Target::New { name, home })
    }
  }
}

/// `POST /claude-logins`: a sign-in started, answered with its identifier and the address to approve it at.
pub struct StartClaudeLogin;

impl Route for StartClaudeLogin {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/claude-logins";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let acting = Acting::of(&call)?;
    let body = call.body().await?;

    if let Acting::Person(user) = acting {
      return start_on_node(call.ground(), user, &body).await;
    }

    let target = target_of(call.store(), &call.data_root(), &body).await?;
    std::fs::DirBuilder::new()
      .recursive(true)
      .mode(0o700)
      .create(target.home())
      .map_err(Refused::failed)?;

    let policy = &CLAUDE_CODE.console_login;
    let (program, arguments) =
      policy.command.split_first().ok_or_else(|| Refused::failed("no sign-in command"))?;
    let mut command = Command::new(program);
    command
      .args(arguments)
      .env("CLAUDE_CONFIG_DIR", target.home())
      // The browser is the owner's own, wherever they are: none is opened on this machine.
      .env("BROWSER", "true")
      .env_remove("DISPLAY")
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true);
    for carried in CARRIED_LOGINS.iter().filter(|name| **name != "CLAUDE_CONFIG_DIR") {
      command.env_remove(carried);
    }
    let mut cli = command.spawn().map_err(Refused::failed)?;
    let said = Arc::new(std::sync::Mutex::new(String::new()));
    if let Some(out) = cli.stdout.take() {
      listen(out, Arc::clone(&said));
    }
    if let Some(err) = cli.stderr.take() {
      listen(err, Arc::clone(&said));
    }
    let input = cli.stdin.take();

    let bound = Instant::now() + Duration::from_secs(policy.link_seconds);
    let address = loop {
      let heard = said.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
      if let Some(address) = sign_in_address(&heard) {
        break address;
      }
      let ended = cli.try_wait().map_err(Refused::failed)?.is_some();
      if ended || Instant::now() >= bound {
        drop(cli);
        target.abandon();
        return Err(no_link(&heard));
      }
      tokio::time::sleep(Duration::from_millis(200)).await;
    };

    let name = target.name().to_owned();
    let sign_in = SignIn {
      cli,
      input,
      said,
      target,
      started: Instant::now(),
    };

    Ok(hold(Held::Here(sign_in), address, &name).await)
  }
}

crate::moved!(StartClaudeLogin);

fn no_sign_in(login: &str) -> Refused {
  Refused::new(StatusCode::NOT_FOUND, fill_marks(&words().none, &[("login", login)]))
    .with_code("no-sign-in")
}

/// `POST /claude-logins/{login}/code`: the code handed to the sign-in's CLI, the login it writes checked, and a
/// new account added once it is there.
pub struct ClaudeLoginCode;

impl Route for ClaudeLoginCode {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/claude-logins/{login}/code";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let acting = Acting::of(&call)?;
    let login = call.param("login").unwrap_or_default().to_owned();
    let body = call.body().await?;
    let code = text(&body, "code").map(str::to_owned).ok_or_else(|| {
      Refused::new(StatusCode::BAD_REQUEST, words().code_needed.clone()).with_field("code")
    })?;

    // The sign-in is taken out of the held ones: it ends here, whatever the CLI makes of the code.
    let mut sign_in = match take(&login, acting).await? {
      Held::Here(sign_in) => sign_in,
      Held::OnNode(sign_in) => return code_on_node(call.ground(), sign_in, &code).await,
    };
    let refused = |said: &str| {
      sign_in.target.abandon();
      Refused::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        fill_marks(&words().refused, &[("said", &last_words(said, Some(&code)))]),
      )
      .with_code("sign-in-refused")
    };
    let heard = || sign_in.said.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();

    let Some(mut input) = sign_in.input.take() else {
      return Err(refused(&heard()));
    };
    if input.write_all(format!("{code}\n").as_bytes()).await.is_err() {
      return Err(refused(&heard()));
    }
    drop(input);

    let finish = Duration::from_secs(CLAUDE_CODE.console_login.finish_seconds);
    let ended = tokio::time::timeout(finish, sign_in.cli.wait()).await;
    let well = matches!(ended, Ok(Ok(status)) if status.success());
    if !well {
      // A CLI still running past the bound is ended before its directory can be taken away.
      let _ = sign_in.cli.start_kill();
      let _ = sign_in.cli.wait().await;
      return Err(refused(&heard()));
    }

    let home = sign_in.target.home().to_path_buf();
    if !home.join(CREDENTIAL_FILE).is_file() {
      sign_in.target.abandon();
      return Err(
        Refused::new(
          StatusCode::UNPROCESSABLE_ENTITY,
          fill_marks(&words().no_login, &[("path", &home.to_string_lossy())]),
        )
        .with_code("sign-in-refused"),
      );
    }

    if let Target::New { name, home } = &sign_in.target {
      let given = CredentialGiven {
        kind: Some(CredentialKind::Directory),
        path: Some(home.to_string_lossy().into_owned()),
        ..CredentialGiven::default()
      };
      changes::add(call.store(), name, &given, BasePromptGiven::Keep).await?;
    }

    accounts_answer(Judging::of(call.ground()), acting).await
  }
}

crate::moved!(ClaudeLoginCode);

/// `DELETE /claude-logins/{login}`: a sign-in given up, its CLI ended and a new account's empty directory taken
/// away.
pub struct GiveUpClaudeLogin;

impl Route for GiveUpClaudeLogin {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/claude-logins/{login}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let acting = Acting::of(&call)?;
    let login = call.param("login").unwrap_or_default().to_owned();

    // The CLI is ended before its directory is taken away, so it writes nothing there after; a pane on a node
    // is closed before the answer.
    match take(&login, acting).await? {
      Held::OnNode(sign_in) => sign_in.pane.close().await,
      here => here.end(),
    }

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(GiveUpClaudeLogin);

/// The pane a person's sign-in runs in on their node, in that node's herdr session.
struct NodePane {
  herdr: herdr::Herdr,
  workspace: String,
  pane: String,
}

// How many of the pane's latest lines a look reads: the sign-in's whole exchange.
const SCREEN_LINES: u32 = 200;

impl NodePane {
  /// The workspace made on the node's herdr and the launch laid out alone in its tab; a workspace whose pane
  /// cannot be laid out is closed rather than left behind.
  async fn open(herdr: herdr::Herdr, label: &str, launch: &NodeLaunch) -> Result<Self, Refused> {
    let unopened = |why: String| Refused::new(StatusCode::BAD_GATEWAY, why);
    let made = herdr
      .call(
        "workspace.create",
        json!({ "label": label, "cwd": launch.cwd, "env": launch.env }),
        None,
      )
      .await
      .map_err(|fault| unopened(format!("The node's herdr made no workspace: {}", fault.message)))?;
    let (Some(workspace), Some(tab)) = (
      made.pointer("/workspace/workspace_id").and_then(Value::as_str).map(str::to_owned),
      made.pointer("/tab/tab_id").and_then(Value::as_str),
    ) else {
      return Err(unopened("The node's herdr named no workspace and tab".to_owned()));
    };
    let root = json!({ "type": "pane", "command": launch.command, "cwd": launch.cwd, "env": launch.env });
    let applied = herdr.call("layout.apply", json!({ "tab_id": tab, "root": root }), None).await;
    let pane = applied
      .as_ref()
      .ok()
      .and_then(|applied| applied.pointer("/layout/root/pane_id"))
      .and_then(Value::as_str)
      .map(str::to_owned);
    let Some(pane) = pane else {
      let _ = herdr.call("workspace.close", json!({ "workspace_id": workspace }), None).await;

      return Err(unopened("The node's herdr laid out no pane for the sign-in".to_owned()));
    };

    Ok(Self {
      herdr,
      workspace,
      pane,
    })
  }

  /// What the pane has written lately, its soft-wrapped lines joined again by herdr itself, or herdr's words where
  /// it read none.
  async fn read(&self) -> Result<String, String> {
    self
      .herdr
      .read_pane(&self.pane, herdr::ReadSource::RecentUnwrapped, Some(SCREEN_LINES))
      .await
      .map_err(|fault| fault.message)
  }

  /// What the pane has written lately, or nothing where herdr read none.
  async fn screen(&self) -> String {
    self.read().await.unwrap_or_default()
  }

  /// The pane and its workspace closed, the CLI and the shell holding it ended with them.
  async fn close(&self) {
    let _ = self.herdr.call("pane.close", json!({ "pane_id": self.pane }), None).await;
    let _ = self
      .herdr
      .call("workspace.close", json!({ "workspace_id": self.workspace }), None)
      .await;
  }
}

/// What a person's sign-in pane is started with on their node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeLaunch {
  pub command: Vec<String>,
  pub cwd: String,
  pub env: BTreeMap<String, String>,
}

/// The variables a sign-in's shell takes away before the CLI starts, since a pane is handed variables by name
/// and cannot be told to drop one: every login the node's session carries but the directory it is given.
const NODE_CARRIED_LOGINS: [&str; 3] =
  ["CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

/// The launch of a person's sign-in on their node: the CLI's own sign-in, the same words as the owner's, run
/// by the workplace's own Claude Code in place of the word `claude` (docs/work-nodes.md, 「The workplace on a
/// node」), as the node's user under the node's workplace, its configuration directory `home`, its updater off,
/// no browser opened there, and every other login taken away first. Where that Claude Code does not stand, the
/// pane says the policy's line for it in place of starting anything. A shell holds the pane after the CLI ends, so
/// what it said last stays to be read; the CLI's words are handed to it as arguments, never spliced into its script.
/// The shell holds the pane in the `wait` builtin over a sleep in the background, never over a sleep in the
/// foreground: a shell runs a trap only once its foreground command ends, so a hangup that came as the sleep was
/// being started, the sleep itself not yet there to end by it, would leave the shell and the directory an hour.
/// A `fresh` sign-in, a new login's, takes its directory away as its pane closes where no login was written into it,
/// so a new login given up, refused or never shown an address leaves nothing on the node.
pub fn node_launch(node_root: &str, node_home: &str, home: &str, lang: &str, fresh: bool) -> NodeLaunch {
  let policy = &CLAUDE_CODE.console_login;
  let leave = if fresh {
    format!(
      r#"gone() {{ kill $! 2>/dev/null; [ -f "$CLAUDE_CONFIG_DIR/{CREDENTIAL_FILE}" ] || rm -rf "$CLAUDE_CONFIG_DIR"; exit 0; }}; trap gone HUP INT TERM; "#
    )
  } else {
    String::new()
  };
  let script = format!(
    r#"{leave}unset {}; mkdir -p -m 700 "$CLAUDE_CONFIG_DIR" && if [ -x "$1" ]; then "$@"; else printf '%s\n' {}; fi; while :; do sleep 3600 & wait $!; done"#,
    NODE_CARRIED_LOGINS.join(" "),
    crate::verbs::work_node_setup::shell_word(&policy.missing),
  );
  let mut command = vec!["/bin/sh".to_owned(), "-c".to_owned(), script, "claude-sign-in".to_owned()];
  let mut words = policy.command.iter().cloned();

  command.extend(words.next().map(|_| crate::watches::links::own_claude(node_root)));
  command.extend(words);

  let path = crate::watches::links::path_head(node_home, node_root)
    .into_iter()
    .chain(["/usr/local/bin", "/usr/bin", "/bin"].map(str::to_owned))
    .collect::<Vec<_>>()
    .join(":");

  NodeLaunch {
    command,
    cwd: node_home.to_owned(),
    env: BTreeMap::from([
      ("HOME".to_owned(), node_home.to_owned()),
      ("PATH".to_owned(), path),
      ("LANG".to_owned(), lang.to_owned()),
      ("CLAUDE_CONFIG_DIR".to_owned(), home.to_owned()),
      // The browser is the person's own, wherever they are: none is opened on the node.
      ("BROWSER".to_owned(), "true".to_owned()),
    ])
    .into_iter()
    .chain(CLAUDE_CODE.workplace_copy.env.iter().cloned())
    .collect(),
  }
}

/// Whether a sign-in's pane says the workplace's own Claude Code does not stand, by the policy's line for it.
pub fn says_missing(screen: &str) -> bool {
  plain(screen).lines().any(|line| line.trim() == CLAUDE_CODE.console_login.missing)
}

/// What a sign-in's pane says of the login once its code was typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeOutcome {
  /// The CLI said the login took.
  Took,
  /// The CLI said it refused the code.
  Refused,
  /// It has said neither yet.
  Pending,
}

/// What the pane's words after the code say, by the policy's words for a login that took and for a code
/// refused, matched without regard to case; the words said before the code was typed are not read again.
pub fn node_outcome(after_code: &str) -> NodeOutcome {
  let said = plain(after_code).to_lowercase();
  let says = |words: &[String]| words.iter().any(|word| said.contains(&word.to_lowercase()));
  let policy = &CLAUDE_CODE.console_login;

  if says(&policy.refused) {
    NodeOutcome::Refused
  } else if says(&policy.took) {
    NodeOutcome::Took
  } else {
    NodeOutcome::Pending
  }
}

/// The words a pane shows past what it showed before: the part of `now` after the longest head it shares
/// with `before`, or the whole of it where the screen moved under it.
pub fn said_since(before: &str, now: &str) -> String {
  let shared = before.chars().zip(now.chars()).take_while(|(one, other)| one == other).count();

  now.chars().skip(shared).collect()
}

// The refusal of a sign-in on a node whose workplace holds no Claude Code of its own yet, the node's setup started
// to lay it, so a start a few minutes later finds it.
async fn no_cli(ground: &crate::places::Ground, node: &str) -> Refused {
  use crate::watches::links::LinkUnits;

  if !crate::watches::links::Systemd(&ground.roots).set_up(node).await {
    eprintln!("{node}: the setup that lays the workplace's own Claude Code did not start");
  }

  Refused::new(StatusCode::CONFLICT, fill_marks(&words().no_cli, &[("node", node)])).with_code("no-cli")
}

// The refusal of a sign-in whose pane herdr would not read until the bound, in herdr's words.
fn unread(node: &str, why: &str) -> Refused {
  Refused::new(StatusCode::BAD_GATEWAY, fill_marks(&words().unread, &[("node", node), ("said", why)]))
    .with_code("pane-unread")
}

fn not_attached(node: &str, words_of: &str) -> Refused {
  Refused::new(StatusCode::CONFLICT, fill_marks(words_of, &[("node", node)])).with_code("node-not-attached")
}

// The node a person's sign-in runs on, its workplace's root and home as the node reads them, or the refusal of
// one not attached now.
async fn person_workplace(
  ground: &crate::places::Ground,
  user: i64,
) -> Result<(String, String, String), Refused> {
  let db = &ground.db;
  let node = crate::person_nodes::node_of(db, &user.to_string()).await?.ok_or_else(|| {
    Refused::new(StatusCode::CONFLICT, words().no_node.clone()).with_code("node-not-attached")
  })?;

  if !crate::watches::links::link_heard(&ground.roots.data, &node) {
    return Err(not_attached(&node, &words().node_not_attached));
  }

  let workplace = store::one(
    db,
    r#"SELECT "root", "home" FROM "node_workplaces" WHERE "node" = $1 AND "state" = 'ready'"#,
    vec![node.as_str().into()],
  )
  .await?;
  let unready = || {
    Refused::new(StatusCode::CONFLICT, fill_marks(&words().node_unready, &[("node", &node)]))
      .with_code("node-not-ready")
  };
  let Some(workplace) = workplace else {
    return Err(unready());
  };
  let (root, home): (String, Option<String>) =
    (workplace.try_get("", "root")?, workplace.try_get("", "home")?);
  let Some(home) = home else { return Err(unready()) };

  Ok((node, crate::watches::links::node_path(&root, Some(&home)), home))
}

// A person's sign-in started on their node: their own login there signed in again in its own directory, or a new
// account by the name the start gives or the next free one agency names, its directory under the node's data root;
// the address read off the pane the CLI runs in.
async fn start_on_node(
  ground: &crate::places::Ground,
  user: i64,
  body: &Value,
) -> Result<Response, Refused> {
  let held = accounts(&ground.db).await?;
  // The person's own login signed in again: its identifier, its name, the node it stands on and its directory there.
  let again = match (text(body, "account"), text(body, "name")) {
    (Some(_), Some(_)) => return Err(which()),
    (Some(id), None) => {
      let account = held
        .iter()
        .find(|one| one.id == id && one.user_id == Some(user))
        .ok_or_else(|| unknown_account(id))?;
      let Credential::Directory { home } = &account.credential else {
        return Err(not_directory(&account.name));
      };

      Some((
        account.id.clone(),
        account.name.clone(),
        account.node.clone(),
        home.to_string_lossy().into_owned(),
      ))
    }
    (None, _) => None,
  };
  let name = match &again {
    Some((_, name, ..)) => name.clone(),
    None => {
      let names: Vec<&str> = held.iter().map(|one| one.name.as_str()).collect();

      new_login_name(text(body, "name"), &names, None)?
    }
  };

  let (node, node_root, node_home) = person_workplace(ground, user).await?;
  let (again, home) = match again {
    Some((id, _, on, home)) if on.as_deref() == Some(node.as_str()) => (Some(id), home),
    Some((_, name, on, _)) => {
      let elsewhere = on.unwrap_or_else(|| "the server".to_owned());

      return Err(
        Refused::new(
          StatusCode::CONFLICT,
          fill_marks(&words().login_elsewhere, &[("name", &name), ("node", &elsewhere)]),
        )
        .with_code("login-elsewhere"),
      );
    }
    None => (
      None,
      new_home(&Path::new(&node_root).join("data"), &name).to_string_lossy().into_owned(),
    ),
  };
  if !crate::watches::links::holds_own_claude(&ground.roots, &node) {
    return Err(no_cli(ground, &node).await);
  }

  let lang = ground.vars().get("LANG").cloned().unwrap_or_else(|| "C.UTF-8".to_owned());
  let launch = node_launch(&node_root, &node_home, &home, &lang, again.is_none());
  let herdr = herdr::Herdr::there(ground.roots.data.join("nodes").join(&node).join("herdr.sock"))
    .within(crate::places::node_herdr_ceiling());
  let pane = NodePane::open(herdr, &format!("sign-in · {name}"), &launch).await?;

  let bound = Instant::now() + Duration::from_secs(CLAUDE_CODE.console_login.link_seconds);
  let address = loop {
    let read = pane.read().await;
    let screen = read.as_deref().unwrap_or_default();

    if let Some(address) = sign_in_address(screen) {
      break address;
    }

    if says_missing(screen) {
      pane.close().await;

      return Err(no_cli(ground, &node).await);
    }

    if Instant::now() >= bound {
      pane.close().await;

      return Err(match read {
        Err(why) => unread(&node, &why),
        Ok(screen) => no_link(&screen),
      });
    }

    tokio::time::sleep(Duration::from_millis(500)).await;
  };
  let answer_name = name.clone();
  let sign_in = NodeSignIn {
    pane,
    user,
    node,
    again,
    name,
    home,
    started: Instant::now(),
  };

  Ok(hold(Held::OnNode(sign_in), address, &answer_name).await)
}

// The code typed into a person's sign-in on their node, the pane read until the CLI says the login took or
// refused it, the pane closed, and the account added, pinned to the person and naming the node, once it took.
async fn code_on_node(
  ground: &crate::places::Ground,
  sign_in: NodeSignIn,
  code: &str,
) -> Result<Response, Refused> {
  let pane = &sign_in.pane;
  let before = pane.screen().await;
  let typed = pane
    .herdr
    .call("pane.send_text", json!({ "pane_id": pane.pane, "text": code }), None)
    .await
    .and(crate::door::looks::send_keys(&pane.herdr, &pane.pane, &["enter"]).await);
  let refused = |said: &str| {
    Refused::new(
      StatusCode::UNPROCESSABLE_ENTITY,
      fill_marks(&words().refused, &[("said", &last_words(said, Some(code)))]),
    )
    .with_code("sign-in-refused")
  };

  if typed.is_err() {
    pane.close().await;

    return Err(refused(&before));
  }

  let bound = Instant::now() + Duration::from_secs(CLAUDE_CODE.console_login.finish_seconds);
  loop {
    let said = said_since(&before, &pane.screen().await);

    match node_outcome(&said) {
      NodeOutcome::Took => break,
      NodeOutcome::Refused => {
        pane.close().await;

        return Err(refused(&said));
      }
      NodeOutcome::Pending if Instant::now() >= bound => {
        pane.close().await;

        return Err(refused(&said));
      }
      NodeOutcome::Pending => tokio::time::sleep(Duration::from_millis(500)).await,
    }
  }

  pane.close().await;

  // The person's own login signed in again stands where its account names it; its mark of a login waiting for its
  // person goes, read off nobody's file here since the login stands on the node, and its windows are read afresh.
  if let Some(account) = &sign_in.again {
    store::run(
      &ground.db,
      r#"UPDATE "claude_accounts" SET "needs_login_at" = NULL WHERE "id" = $1"#,
      vec![account.as_str().into()],
    )
    .await?;
    super::claude_accounts::forget_windows(&ground.db, account).await?;

    return accounts_answer(Judging::of(ground), Acting::Person(sign_in.user)).await;
  }

  let given = CredentialGiven {
    kind: Some(CredentialKind::Directory),
    path: Some(sign_in.home.clone()),
    ..CredentialGiven::default()
  };
  let owning = Owning {
    user_id: Some(sign_in.user),
    unit_id: None,
    node: Some(sign_in.node.clone()),
  };

  changes::add_owned(&ground.db, &sign_in.name, &given, BasePromptGiven::Keep, &owning).await?;

  accounts_answer(Judging::of(ground), Acting::Person(sign_in.user)).await
}
