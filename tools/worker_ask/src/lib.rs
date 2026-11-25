//! `worker_ask`: a supervisor hands a small coding task to a run of the worker role, with no queue item: a
//! slot of the role asked and waited for however long the line takes, one run of the role at the task's
//! folder on the road the slot carries, and the words it wrote and the files it changed read back
//! (docs/tools.md, 「The worker ask」).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};
use tool_call::{Answer, Asked, Control, Handed, Session, Turn};

/// The entry the manifest declares.
pub const NAMESPACE: &str = "bmo";
pub const NAME: &str = "worker_ask";
pub const TITLE: &str = "Ask a worker run";
pub const DESCRIPTION: &str = "Give a run of the worker role a small coding task, without filing a queue item, and answer with what it wrote and the files it changed. A task worth keeping and reviewing is a queue item.";
pub const CAPABILITIES: &[&str] = &["room.tools"];

/// The role a worker ask runs as, and the source its slot is asked under (docs/role-slots.md, 「Who takes a
/// slot」).
pub const ROLE: &str = "worker";
pub const SOURCE: &str = "ask";

/// How often a waiting slot asks again for its admission: a pace, never a bound on how long the ask waits.
pub const LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

pub fn entry() -> String {
  format!("{NAMESPACE}/{NAME}")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskInput {
  pub words: String,
  #[serde(default)]
  pub folder: Option<String>,
  /// A personal app of the person the session acts for, whose folder of 'my apps' the run works in.
  #[serde(default)]
  pub app: Option<String>,
}

/// The folder under the data root the apps' sources stand in, one folder an app, which a person's PC reads as its
/// own (docs/person-nodes.md, 「Personal apps」).
pub const MY_APPS: &str = "my-apps";

/// The procedure an app's run is led by, under the tree the tool reads `prompts/` from.
pub const GUIDE: &str = "prompts/agents/chat/procedures/make-app.md";

/// An app's name: a lower-case letter, then lower-case letters, digits and `_`.
pub fn app_name(name: &str) -> bool {
  name.chars().next().is_some_and(|first| first.is_ascii_lowercase())
    && name.chars().all(|one| one.is_ascii_lowercase() || one.is_ascii_digit() || one == '_')
}

// The app's folder and the guide its run is led by, or why it cannot be asked: an app and a folder together, an app
// under a record naming no account, a name outside an app's alphabet, or a guide that cannot be read.
fn app_of(session: &Session, input: &AskInput) -> Result<Option<(String, String)>, String> {
  let Some(app) = input.app.as_deref().map(str::trim).filter(|app| !app.is_empty()) else {
    return Ok(None);
  };

  if input.folder.as_deref().is_some_and(|folder| !folder.trim().is_empty()) {
    return Err("name an app or a folder, not both".to_owned());
  }

  if session.owner_account.as_deref().is_none_or(|account| account.trim().is_empty()) {
    return Err("this session acts for no person, and makes no personal app".to_owned());
  }

  if !app_name(app) {
    return Err(format!(
      "{app:?} is no app's name: a lower-case letter, then lower-case letters, digits and `_`"
    ));
  }

  let tree = tool_call::prompts_tree(&std::env::current_dir().unwrap_or_default());
  let guide = std::fs::read_to_string(tree.join(GUIDE))
    .map_err(|why| format!("the guide {GUIDE} could not be read: {why}"))?;
  let folder = session.data_root().join(MY_APPS).join(app);

  Ok(Some((folder.to_string_lossy().into_owned(), guide)))
}

/// Agency's server as the tool reaches it (docs/tools.md, 「The worker ask」), and the pace a waiting slot asks
/// again at.
pub struct Api {
  pub control: Control,
  /// The pace a waiting slot asks again at for its admission: never a bound on how long it waits.
  pub pace: std::time::Duration,
}

impl Api {
  /// The server the call's data root describes, each variable this process holds standing first.
  pub fn from_record(session: &Session) -> Self {
    Self::read(&session.data_root(), |name| std::env::var(name).ok())
  }

  /// The server the data root's `env/agency.env` describes, each variable `variable` answers standing first.
  pub fn read(data_root: &Path, variable: impl Fn(&str) -> Option<String>) -> Self {
    Self {
      control: Control::read(data_root, variable),
      pace: LOOK_EVERY,
    }
  }

  // A read answered whenever agency answers: no timeout cuts the wait.
  async fn get(&self, path: &str) -> Result<(reqwest::StatusCode, Value), String> {
    let mut request = reqwest::Client::new().get(format!("{}{path}", self.control.base()));

    if let Some(token) = &self.control.token {
      request = request.bearer_auth(token);
    }

    let answered =
      request.send().await.map_err(|why| format!("agency could not be asked {path}: {why}"))?;
    let status = answered.status();

    Ok((status, answered.json().await.unwrap_or(Value::Null)))
  }

  // A send answered once agency answers, a turn's once the turn has ended, however long that takes.
  async fn post(&self, path: &str, body: Value) -> Result<(reqwest::StatusCode, Value), String> {
    self.send(reqwest::Method::POST, path, body).await
  }

  async fn patch(&self, path: &str, body: Value) -> Result<(reqwest::StatusCode, Value), String> {
    self.send(reqwest::Method::PATCH, path, body).await
  }

  async fn send(
    &self,
    method: reqwest::Method,
    path: &str,
    body: Value,
  ) -> Result<(reqwest::StatusCode, Value), String> {
    let mut request = reqwest::Client::new()
      .request(method, format!("{}{path}", self.control.base()))
      .json(&body);

    if let Some(token) = &self.control.token {
      request = request.bearer_auth(token);
    }

    let answered =
      request.send().await.map_err(|why| format!("agency could not be asked {path}: {why}"))?;
    let status = answered.status();

    Ok((status, answered.json().await.unwrap_or(Value::Null)))
  }
}

/// The folder a run works in: the named one, or the asking supervisor's scratch workspace under the data root.
fn workspace(session: &Session, folder: Option<&str>) -> Result<PathBuf, String> {
  if let Some(folder) = folder.filter(|folder| !folder.trim().is_empty()) {
    let path = PathBuf::from(folder);

    std::fs::create_dir_all(&path).map_err(|why| format!("{} could not be made: {why}", path.display()))?;

    return Ok(path);
  }

  let agent = session
    .variables
    .get(tool_call::AGENT_VARIABLE)
    .cloned()
    .unwrap_or_else(|| "chat".to_owned());
  let path = session.data_root().join("agent-workspaces").join(agent).join("scratch");

  std::fs::create_dir_all(&path).map_err(|why| format!("{} could not be made: {why}", path.display()))?;

  Ok(path)
}

/// Every file path a run's log lines name as changed, under the folder the turn worked in.
fn changed(lines: &[Value], folder: &Path) -> Vec<String> {
  let mut found = BTreeSet::new();
  let prefix = folder.to_string_lossy().into_owned();

  fn walk(value: &Value, prefix: &str, found: &mut BTreeSet<String>) {
    match value {
      Value::Object(map) => {
        for (key, value) in map {
          if matches!(key.as_str(), "path" | "file" | "filePath" | "file_path")
            && let Some(path) = value.as_str()
            && path.starts_with(prefix)
          {
            found.insert(path.to_owned());
          }

          walk(value, prefix, found);
        }
      }
      Value::Array(items) => items.iter().for_each(|item| walk(item, prefix, found)),
      _ => {}
    }
  }

  for line in lines {
    walk(line, &prefix, &mut found);
  }

  found.into_iter().collect()
}

/// One worker ask, answered with the run's words and the files it changed.
pub async fn ask(session: &Session, input: AskInput) -> Answer {
  ask_with(&Api::from_record(session), session, input).await
}

/// The same ask against the server the caller names, for a test of the refusals and the round trip.
pub async fn ask_with(api: &Api, session: &Session, input: AskInput) -> Answer {
  if input.words.trim().is_empty() {
    return Answer::failure("words must not be empty");
  }

  let (input, guide) = match app_of(session, &input) {
    Ok(Some((folder, guide))) => (
      AskInput {
        folder: Some(folder),
        ..input
      },
      Some(guide),
    ),
    Ok(None) => (input, None),
    Err(why) => return Answer::failure(why),
  };
  let owner = session.variables.get("BMO_TURN_PRIVILEGED").is_some_and(|flag| flag == "1");
  let named = input
    .folder
    .as_deref()
    .map(str::trim)
    .filter(|folder| !folder.is_empty())
    .map(PathBuf::from);
  // A checkout is judged before anything is made: one on a node is no folder here.
  let checkout = match &named {
    Some(folder) => checkout_of(api, folder).await,
    None => Ok(None),
  };

  match checkout {
    Ok(Some(checkout)) if checkout.node.is_some() => {
      return Answer::failure(format!(
        "{} is {}'s checkout on {}, and this tool names no folder on that node",
        named.unwrap_or_default().display(),
        checkout.repository,
        checkout.node.unwrap_or_default()
      ));
    }
    Ok(Some(checkout)) if !owner => {
      return Answer::failure(format!(
        "{} is {}'s checkout, which may be named only in the owner's turn; name another folder",
        named.unwrap_or_default().display(),
        checkout.repository
      ));
    }
    Ok(_) => {}
    Err(why) => return Answer::failure(why),
  }

  let folder = match workspace(session, input.folder.as_deref()) {
    Ok(folder) => folder,
    Err(why) => return Answer::failure(why),
  };

  let asker = asker(session);
  let key = format!("{SOURCE}:{asker}:{}:{}", now_ms(), std::process::id());
  let asked = json!({
    "requestKey": key,
    "owner": key,
    "role": ROLE,
    "source": SOURCE,
    "title": first_line(&input.words),
    "askedBy": asker,
    "path": folder.display().to_string(),
  });
  let slot = match ok(api.post("/slot-requests", asked).await, "no slot was asked") {
    Ok(slot) => slot,
    Err(why) => return Answer::failure(why),
  };
  let Some(token) = text(&slot, "token").map(str::to_owned) else {
    return Answer::failure("agency answered the slot with no token");
  };
  // The run starts on the slot once it is let in; the slot's road is agency's to read (docs/api.md, 「sessions」).
  if let Err(why) = admitted(api, &token, slot).await {
    finish(api, &token, "cancelled: never let in", true).await;

    return Answer::failure(why);
  }

  let start = json!({
    "scope": "role",
    "agentName": ROLE,
    "slot": token,
    "cwd": folder.display().to_string(),
  });
  let run_session = match ok(api.post("/sessions", start).await, "the worker run was not started") {
    Ok(started) => match text(&started, "id") {
      Some(id) => id.to_owned(),
      None => {
        finish(api, &token, "cancelled: its run was not started", true).await;

        return Answer::failure("agency answered the worker run with no session");
      }
    },
    Err(why) => {
      finish(api, &token, "cancelled: its run was not started", true).await;

      return Answer::failure(why);
    }
  };
  let run = format!("{SOURCE}-{asker}-{}", now_ms());

  if let Err(why) = ok(
    api
      .patch(&format!("/slot-requests/{token}"), json!({ "state": "bound", "childId": run }))
      .await,
    "the slot was not bound",
  ) {
    finish(api, &token, "cancelled: not bound", true).await;

    return Answer::failure(why);
  }

  let since = now_ms();
  let prompt = match (&guide, &input.app) {
    (Some(guide), Some(app)) => format!(
      "Work in {}, the folder of the personal app {}, and follow the guide below.\n\n{}\n\n{guide}",
      folder.display(),
      app.trim(),
      input.words.trim()
    ),
    _ => format!("Work in {}.\n\n{}", folder.display(), input.words.trim()),
  };
  let turned = ok(
    api
      .post(
        &format!("/sessions/{run_session}/turns"),
        json!({
          "prompt": prompt,
          "resume": { "kind": "auto" },
          "name": run,
          "title": format!("asked by {asker}"),
          "chat": Turn::of(session).chat.unwrap_or_default(),
          "hearsWords": true,
          "variables": {},
        }),
      )
      .await,
    "the run did not come back",
  );
  let answer = turned.as_ref().ok().and_then(|turned| {
    turned
      .get("answer")
      .and_then(Value::as_str)
      .or_else(|| {
        turned
          .get("text")
          .and_then(Value::as_array)
          .and_then(|text| text.first())
          .and_then(Value::as_str)
      })
      .map(str::to_owned)
  });
  let outcome = match (&turned, &answer) {
    (Ok(_), Some(_)) => "answered".to_owned(),
    (Ok(_), None) => "failed: no answer".to_owned(),
    (Err(why), _) => format!("failed: {why}"),
  };

  finish(api, &token, &outcome, false).await;

  if let Err(why) = turned {
    return Answer::failure(why);
  }

  let lines: Vec<Value> = api
    .get(&format!("/log/agents/{ROLE}/lines?since={since}"))
    .await
    .ok()
    .and_then(|(_, body)| body.get("lines").and_then(Value::as_array).cloned())
    .unwrap_or_default();
  let files = changed(&lines, &folder);
  let mut text = answer.unwrap_or_default().trim_end().to_owned();

  if !files.is_empty() {
    text.push_str("\n\nFiles changed:\n");

    for file in files {
      text.push_str(&format!("- {file}\n"));
    }
  }

  Answer::text(text)
}

/// A repository checkout a folder is: the repository's name and the node it stands on, where it is not this
/// machine.
struct Checkout {
  repository: String,
  node: Option<String>,
}

// The repository checkout the folder is, read from agency's repositories, or none.
async fn checkout_of(api: &Api, folder: &Path) -> Result<Option<Checkout>, String> {
  let said = ok(api.get("/repositories").await, "the repositories could not be read")?;

  for repository in said.get("items").and_then(Value::as_array).into_iter().flatten() {
    let name = text(repository, "name").unwrap_or_default();
    let checkouts = repository.get("checkouts").and_then(Value::as_array).cloned().unwrap_or_default();

    for place in std::iter::once(repository).chain(checkouts.iter()) {
      if text(place, "path").is_some_and(|path| Path::new(path) == folder) {
        return Ok(Some(Checkout {
          repository: name.to_owned(),
          node: text(place, "nodeName").map(str::to_owned),
        }));
      }
    }
  }

  Ok(None)
}

/// The slot let in, asked for its admission at the pace however long the line takes; or why it never will
/// be: it was given back before it was let in, or agency could not be asked.
async fn admitted(api: &Api, token: &str, mut slot: Value) -> Result<Value, String> {
  let path = format!("/slot-requests/{token}");

  loop {
    match text(&slot, "state") {
      Some("active") => return Ok(slot),
      Some(state @ ("released" | "cancelled" | "finished")) => {
        return Err(format!("the slot was given back ({state}) before it was let in"));
      }
      _ => {}
    }

    tokio::time::sleep(api.pace).await;

    let said = ok(api.patch(&path, json!({})).await, "the slot could not be let in")?;

    slot = match said.get("request").filter(|request| !request.is_null()) {
      Some(now) => now.clone(),
      None => ok(api.get(&path).await, "the slot could not be read")?,
    };

    if text(&slot, "state") == Some("active") && said.get("request").is_none_or(Value::is_null) {
      slot["state"] = json!("waiting");
    }
  }
}

// The slot given back with what its run came to, cancelled where it never ran.
async fn finish(api: &Api, token: &str, outcome: &str, cancel: bool) {
  let body = json!({ "state": "finished", "outcome": outcome, "cancel": cancel });

  if let Err(why) =
    ok(api.patch(&format!("/slot-requests/{token}"), body).await, "the slot was not given back")
  {
    eprintln!("{why}");
  }
}

// A call whose answer must be a success: its body, or the error agency gave.
fn ok(answered: Result<(reqwest::StatusCode, Value), String>, what: &str) -> Result<Value, String> {
  let (status, said) = answered?;

  if status.is_success() {
    return Ok(said);
  }

  let why = said.get("error").and_then(Value::as_str).unwrap_or("no reason given");

  Err(format!("{what}: agency answered {status}: {why}"))
}

fn text<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
  value.get(field).and_then(Value::as_str).filter(|text| !text.is_empty())
}

// The ask's first line, which its slot is listed by.
fn first_line(words: &str) -> String {
  words.trim().lines().next().unwrap_or_default().chars().take(120).collect()
}

// The supervisor the asking turn is, else the record's own, else the chat supervisor.
fn asker(session: &Session) -> String {
  Turn::of(session)
    .agent
    .or_else(|| session.variables.get(tool_call::AGENT_VARIABLE).cloned())
    .map(|agent| agent.trim().to_owned())
    .filter(|agent| !agent.is_empty())
    .unwrap_or_else(|| "chat".to_owned())
}

pub async fn call(session: &Session, arguments: Value) -> Result<Answer, String> {
  let input: AskInput =
    serde_json::from_value(arguments).map_err(|why| format!("invalid arguments: {NAME}: {why}"))?;

  Ok(ask(session, input).await)
}

pub async fn answer(handed: Handed) -> Result<Value, String> {
  let answered = match handed.asked {
    Asked::Call {
      entry: asked,
      arguments,
    } if asked == entry() => call(&handed.session, arguments).await.unwrap_or_else(Answer::failure),
    Asked::Call { entry: asked, .. } => Answer::failure(format!("this executable holds no entry {asked}")),
    Asked::ResourcesList => return Ok(json!({ "resources": [] })),
    Asked::ResourcesRead { .. } => return Ok(json!({ "error": "this executable serves no resources" })),
    Asked::Other => return Err("this executable answers calls alone".to_owned()),
  };

  serde_json::to_value(answered).map_err(|why| format!("the answer could not be written: {why}"))
}

fn now_ms() -> i64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map_or(0, |since| since.as_millis() as i64)
}
