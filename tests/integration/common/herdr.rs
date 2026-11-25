//! A herdr of the test's own on a socket: the arrangement a herdr keeps — workspaces in their order,
//! their tabs, the panes in them and what was reported on each — answering one request to a connection
//! as herdr does, the twin of the TypeScript side's `agents/agent/herdr-model.ts`.
//!
//! A workspace is made with a tab and a shell pane in it; a layout named by its tab replaces that tab
//! with a new one, and a layout named by its workspace adds a tab; a tab with no pane left closes, and a
//! workspace with no tab left is gone. What runs in a pane is not modelled beyond its command.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Clone, Debug)]
pub struct ModelWorkspace {
  pub id: String,
  pub label: String,
  pub tokens: Option<Map<String, Value>>,
  tabs: u32,
  panes: u32,
}

#[derive(Clone, Debug)]
pub struct ModelPane {
  pub id: String,
  pub workspace: String,
  pub tab: String,
  pub label: Option<String>,
  pub command: Option<Vec<String>>,
  pub tokens: Option<Map<String, Value>>,
  /// The folder the pane's process stands in: the one it was started in, as herdr reads it off the process.
  pub cwd: Option<String>,
}

#[derive(Default)]
struct Arrangement {
  workspaces: Vec<ModelWorkspace>,
  tabs: Vec<(String, String, String)>,
  panes: Vec<ModelPane>,
  made: u32,
  asked: Vec<String>,
  // The words put into panes and the keys typed into them, in order; and whether a run asked to stop
  // ends itself, its pane going with it.
  prompts: Vec<(String, String)>,
  keys: Vec<(String, Vec<String>)>,
  ends_when_asked: bool,
  refuses_prompts: bool,
  // What the agent in each pane is doing, for a test that plays the program; none of it answered until a test
  // says, so a herdr that reads no agent stands silent.
  agents: Vec<(String, String)>,
  // The words typed into panes rather than put as a prompt.
  typed: Vec<(String, String)>,
  // A pane whose program ends right after the next picture is read, its pane going with it.
  ends_after_snapshot: Option<String>,
}

impl Arrangement {
  fn workspace(&mut self, id: &str) -> Option<&mut ModelWorkspace> {
    self.workspaces.iter_mut().find(|workspace| workspace.id == id)
  }

  fn new_tab(&mut self, workspace: &str, label: Option<&str>) -> String {
    let Some(held) = self.workspace(workspace) else { return String::new() };

    held.tabs += 1;

    let tab = format!("{workspace}:t{}", held.tabs);
    let label = label.map_or_else(|| held.tabs.to_string(), str::to_owned);

    self.tabs.push((tab.clone(), workspace.to_owned(), label));
    tab
  }

  fn new_pane(
    &mut self,
    tab: &str,
    label: Option<String>,
    command: Option<Vec<String>>,
    cwd: Option<String>,
  ) -> String {
    let Some(workspace) = self.tabs.iter().find(|held| held.0 == tab).map(|held| held.1.clone()) else {
      return String::new();
    };
    let Some(held) = self.workspace(&workspace) else { return String::new() };

    held.panes += 1;

    let pane = format!("{workspace}:p{}", held.panes);

    self.panes.push(ModelPane {
      id: pane.clone(),
      workspace,
      tab: tab.to_owned(),
      label,
      command,
      tokens: None,
      cwd,
    });
    pane
  }

  fn tidy(&mut self) {
    let panes = self.panes.clone();

    self.tabs.retain(|tab| panes.iter().any(|pane| pane.tab == tab.0));

    let tabs = self.tabs.clone();

    self.workspaces.retain(|workspace| tabs.iter().any(|tab| tab.1 == workspace.id));
  }

  fn snapshot(&self) -> Value {
    json!({
      "workspaces": self.workspaces.iter().enumerate().map(|(at, workspace)| json!({
        "workspace_id": workspace.id, "number": at + 1, "label": workspace.label, "tokens": workspace.tokens,
      })).collect::<Vec<_>>(),
      "panes": self.panes.iter().map(|pane| json!({
        "pane_id": pane.id, "workspace_id": pane.workspace, "tab_id": pane.tab, "label": pane.label, "tokens": pane.tokens,
        "cwd": pane.cwd,
      })).collect::<Vec<_>>(),
    })
  }

  fn answer(&mut self, method: &str, params: &Value) -> Value {
    let text = |key: &str| params.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();

    match method {
      "workspace.create" => {
        self.made += 1;

        let id = format!("w{}", self.made);

        self.workspaces.push(ModelWorkspace {
          id: id.clone(),
          label: text("label"),
          tokens: None,
          tabs: 0,
          panes: 0,
        });

        let tab = self.new_tab(&id, None);
        let cwd = params.get("cwd").and_then(Value::as_str).map(str::to_owned);
        let pane = self.new_pane(&tab, None, None, cwd);

        json!({ "workspace": { "workspace_id": id }, "tab": { "tab_id": tab }, "root_pane": { "pane_id": pane } })
      }
      "layout.apply" => {
        let root = params.get("root").cloned().unwrap_or(Value::Null);
        let replaced = params
          .get("tab_id")
          .and_then(Value::as_str)
          .and_then(|tab| self.tabs.iter().find(|held| held.0 == tab).cloned());
        let workspace = replaced.as_ref().map_or_else(|| text("workspace_id"), |held| held.1.clone());

        if self.workspace(&workspace).is_none() {
          return Value::Null;
        }

        if let Some((tab, _, _)) = &replaced {
          self.panes.retain(|pane| pane.tab != *tab);
          self.tabs.retain(|held| held.0 != *tab);
        }

        let label = params.get("tab_label").and_then(Value::as_str).map(str::to_owned);
        let tab = self.new_tab(&workspace, label.as_deref());
        let command: Option<Vec<String>> =
          root.get("command").and_then(|command| serde_json::from_value(command.clone()).ok());
        let pane = self.new_pane(
          &tab,
          root.get("label").and_then(Value::as_str).map(str::to_owned),
          command.clone(),
          root.get("cwd").and_then(Value::as_str).map(str::to_owned),
        );

        json!({ "layout": { "workspace_id": workspace, "tab_id": tab, "root": { "type": "pane", "pane_id": pane, "command": command } } })
      }
      "layout.export" => {
        let Some(pane) = self.panes.iter().find(|pane| pane.id == text("pane_id")) else {
          return Value::Null;
        };
        let tab = pane.tab.clone();
        let in_tab: Vec<&ModelPane> = self.panes.iter().filter(|one| one.tab == tab).collect();
        let node = |one: &ModelPane| match &one.command {
          Some(command) => json!({ "type": "pane", "pane_id": one.id, "command": command }),
          None => json!({ "type": "pane", "pane_id": one.id }),
        };
        let mut tree = node(in_tab[in_tab.len() - 1]);

        for one in in_tab.iter().rev().skip(1) {
          tree = json!({ "type": "split", "first": node(one), "second": tree });
        }

        json!({ "layout": { "tab_id": tab, "root": tree } })
      }
      "workspace.report_metadata" => {
        let tokens = params.get("tokens").and_then(Value::as_object).cloned().unwrap_or_default();
        let Some(workspace) = self.workspace(&text("workspace_id")) else { return Value::Null };

        workspace.tokens.get_or_insert_with(Map::new).extend(tokens);
        json!({ "type": "ok" })
      }
      "pane.report_metadata" => {
        let tokens = params.get("tokens").and_then(Value::as_object).cloned().unwrap_or_default();
        let id = text("pane_id");
        let Some(pane) = self.panes.iter_mut().find(|pane| pane.id == id) else {
          return Value::Null;
        };

        pane.tokens.get_or_insert_with(Map::new).extend(tokens);
        json!({ "type": "ok" })
      }
      "workspace.rename" => {
        let label = text("label");
        let Some(workspace) = self.workspace(&text("workspace_id")) else { return Value::Null };

        workspace.label = label;
        json!({ "type": "workspace_info" })
      }
      "tab.rename" | "pane.rename" => json!({ "type": "ok" }),
      "agent.prompt" => {
        let pane = text("target");

        if self.refuses_prompts || !self.panes.iter().any(|one| one.id == pane) {
          return Value::Null;
        }

        self.prompts.push((pane.clone(), text("text")));

        if self.ends_when_asked {
          self.panes.retain(|one| one.id != pane);
          self.tidy();
        }

        json!({ "type": "ok" })
      }
      "pane.send_text" => {
        let pane = text("pane_id");

        if !self.panes.iter().any(|one| one.id == pane) {
          return Value::Null;
        }

        self.typed.push((pane, text("text")));
        json!({ "type": "ok" })
      }
      "agent.list" if !self.agents.is_empty() => json!({
        "agents": self.agents.iter().map(|(pane, status)| json!({ "pane_id": pane, "agent_status": status })).collect::<Vec<_>>(),
      }),
      "pane.list" if !self.agents.is_empty() => json!({
        "panes": self.panes.iter().map(|pane| json!({ "pane_id": pane.id })).collect::<Vec<_>>(),
      }),
      "pane.send_keys" => {
        let keys: Vec<String> = params
          .get("keys")
          .and_then(|keys| serde_json::from_value(keys.clone()).ok())
          .unwrap_or_default();

        self.keys.push((text("pane_id"), keys));
        json!({ "type": "ok" })
      }
      "workspace.move" => {
        let id = text("workspace_id");
        let Some(at) = self.workspaces.iter().position(|workspace| workspace.id == id) else {
          return Value::Null;
        };
        let held = self.workspaces.remove(at);
        let to = params.get("insert_index").and_then(Value::as_u64).unwrap_or(0) as usize;

        self.workspaces.insert(to.min(self.workspaces.len()), held);
        json!({ "type": "ok" })
      }
      "pane.close" => {
        let before = self.panes.len();
        let id = text("pane_id");

        self.panes.retain(|pane| pane.id != id);

        if self.panes.len() == before {
          return Value::Null;
        }

        self.tidy();
        json!({ "type": "ok" })
      }
      "workspace.close" => {
        let id = text("workspace_id");

        if self.workspace(&id).is_none() {
          return Value::Null;
        }

        self.panes.retain(|pane| pane.workspace != id);
        self.tabs.retain(|tab| tab.1 != id);
        self.workspaces.retain(|workspace| workspace.id != id);
        json!({ "type": "ok" })
      }
      "session.snapshot" => {
        let seen = json!({ "snapshot": self.snapshot() });

        if let Some(pane) = self.ends_after_snapshot.take() {
          self.panes.retain(|one| one.id != pane);
          self.tidy();
        }

        seen
      }
      _ => Value::Null,
    }
  }
}

/// The herdr stand-in and its socket.
#[derive(Clone)]
pub struct HerdrModel {
  pub socket: PathBuf,
  held: Arc<Mutex<Arrangement>>,
}

impl HerdrModel {
  /// A herdr answering on the socket, for as long as the test's runtime stands. A request it has
  /// nothing to say about is answered as a failure, as herdr answers a pane that is not there.
  pub fn stand(socket: PathBuf) -> Self {
    let held = Arc::new(Mutex::new(Arrangement::default()));
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let answering = held.clone();

    tokio::spawn(async move {
      while let Ok((stream, _)) = listener.accept().await {
        let answering = answering.clone();

        tokio::spawn(async move {
          let mut reader = BufReader::new(stream);
          let mut line = String::new();

          if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
            return;
          }

          let request: Value = serde_json::from_str(&line).unwrap();
          let method = request["method"].as_str().unwrap_or_default().to_owned();
          let result = {
            let mut arrangement = answering.lock().unwrap();

            arrangement.asked.push(method.clone());
            arrangement.answer(&method, &request["params"])
          };
          let frame = if result.is_null() {
            json!({ "id": request["id"], "error": { "code": "not_found", "message": method } })
          } else {
            json!({ "id": request["id"], "result": result })
          };

          let _ = reader.into_inner().write_all(format!("{frame}\n").as_bytes()).await;
        });
      }
    });

    Self { socket, held }
  }

  /// One request answered as the socket would answer it, for a test setting the arrangement up.
  pub fn call(&self, method: &str, params: Value) -> Value {
    self.held.lock().unwrap().answer(method, &params)
  }

  /// A workspace made under a label with one pane running a command, carrying the tokens given;
  /// answers the workspace and the pane.
  pub fn workspace(&self, label: &str, tokens: Value, command: Option<&[&str]>) -> (String, String) {
    let made = self.call("workspace.create", json!({ "label": label }));
    let workspace = made["workspace"]["workspace_id"].as_str().unwrap().to_owned();
    let tab = made["tab"]["tab_id"].as_str().unwrap().to_owned();
    let pane = match command {
      None => made["root_pane"]["pane_id"].as_str().unwrap().to_owned(),
      Some(command) => {
        let applied = self
          .call("layout.apply", json!({ "tab_id": tab, "root": { "type": "pane", "command": command } }));

        applied["layout"]["root"]["pane_id"].as_str().unwrap().to_owned()
      }
    };

    if !tokens.is_null() {
      self.call("workspace.report_metadata", json!({ "workspace_id": workspace, "tokens": tokens }));
    }

    (workspace, pane)
  }

  /// One more tab in a workspace running a command; answers its pane.
  pub fn tab(&self, workspace: &str, command: Option<&[&str]>) -> String {
    let root = match command {
      Some(command) => json!({ "type": "pane", "command": command }),
      None => json!({ "type": "pane" }),
    };

    self.call("layout.apply", json!({ "workspace_id": workspace, "root": root }))["layout"]["root"]["pane_id"]
      .as_str()
      .unwrap()
      .to_owned()
  }

  /// A pane's process standing in another folder from here on, as one an earlier deployment started.
  pub fn stands_in(&self, pane: &str, cwd: &Path) {
    let mut held = self.held.lock().unwrap();

    if let Some(one) = held.panes.iter_mut().find(|one| one.id == pane) {
      one.cwd = Some(cwd.to_string_lossy().into_owned());
    }
  }

  /// The folder each pane of the workspace carrying this token was started in.
  pub fn folders_of(&self, token: &str, value: &str) -> Vec<Option<String>> {
    let held = self.held.lock().unwrap();
    let Some(workspace) = held.workspaces.iter().find(|workspace| {
      workspace.tokens.as_ref().and_then(|tokens| tokens.get(token)).and_then(Value::as_str) == Some(value)
    }) else {
      return vec![];
    };

    held
      .panes
      .iter()
      .filter(|pane| pane.workspace == workspace.id)
      .map(|pane| pane.cwd.clone())
      .collect()
  }

  pub fn workspaces(&self) -> Vec<ModelWorkspace> {
    self.held.lock().unwrap().workspaces.clone()
  }

  pub fn panes(&self) -> Vec<ModelPane> {
    self.held.lock().unwrap().panes.clone()
  }

  /// The panes of the workspace carrying this token, each by its command, a shell's as empty.
  pub fn commands_of(&self, token: &str, value: &str) -> Vec<Vec<String>> {
    let held = self.held.lock().unwrap();
    let Some(workspace) = held.workspaces.iter().find(|workspace| {
      workspace.tokens.as_ref().and_then(|tokens| tokens.get(token)).and_then(Value::as_str) == Some(value)
    }) else {
      return vec![];
    };

    held
      .panes
      .iter()
      .filter(|pane| pane.workspace == workspace.id)
      .map(|pane| pane.command.clone().unwrap_or_default())
      .collect()
  }

  /// A run asked to stop ends itself from here on, its pane going with it.
  pub fn ends_when_asked(&self) {
    self.held.lock().unwrap().ends_when_asked = true;
  }

  /// Words put into a pane are refused from here on, as a pane waiting on a person refuses them.
  pub fn refuses_prompts(&self) {
    self.held.lock().unwrap().refuses_prompts = true;
  }

  /// A pane carrying a run's token.
  pub fn carry(&self, pane: &str, run: &str) {
    self.call("pane.report_metadata", json!({ "pane_id": pane, "tokens": { "run": run } }));
  }

  /// What the agent in a pane is doing, as a test playing the program says.
  pub fn agent_status(&self, pane: &str, status: &str) {
    let mut held = self.held.lock().unwrap();

    held.agents.retain(|(one, _)| one != pane);
    held.agents.push((pane.to_owned(), status.to_owned()));
  }

  /// The words typed into panes rather than put as a prompt, each with its pane.
  pub fn typed(&self) -> Vec<(String, String)> {
    self.held.lock().unwrap().typed.clone()
  }

  /// The words put into panes, each with its pane.
  pub fn prompts(&self) -> Vec<(String, String)> {
    self.held.lock().unwrap().prompts.clone()
  }

  /// The keys typed into panes, each with its pane.
  pub fn keys(&self) -> Vec<(String, Vec<String>)> {
    self.held.lock().unwrap().keys.clone()
  }

  /// A pane whose program ends right after the next picture is read, as a head pane's loop that stops
  /// between two readings takes its pane, and the workspace whose one pane it is, with it.
  pub fn ends_after_next_snapshot(&self, pane: &str) {
    self.held.lock().unwrap().ends_after_snapshot = Some(pane.to_owned());
  }

  /// How many times a method was asked.
  pub fn asked(&self, method: &str) -> usize {
    self.held.lock().unwrap().asked.iter().filter(|asked| *asked == method).count()
  }
}
