//! Codex: present where the gateway's key file and its members' key files stand private under the data
//! root, and its models the members of its road (docs/provider-presence.md). Its program is Claude Code on
//! agency's gateway. The installed CLI's catalogue is still read for the chooser that declares a member.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use policy::providers::Declared;
use policy::providers::codex::CODEX;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::process::Command;

use crate::claude_code::ClaudeCode;
use crate::member;
use crate::{Machine, Offered, OfferedModel, Offering, PlaceCache, Provider, Reading, Reason};

pub struct Codex;

impl Provider for Codex {
  fn name(&self) -> &'static str {
    "codex"
  }

  // Its runs are Claude Code's, whose events name their session as Claude Code's do.
  fn session_of_event(&self, event: &serde_json::Value) -> Option<String> {
    ClaudeCode.session_of_event(event)
  }

  fn progress_of(&self, line: &str) -> Option<String> {
    ClaudeCode.progress_of(line)
  }

  fn declared(&self) -> &'static Declared {
    &CODEX.declared
  }

  // Its program is Claude Code under one of its road's homes.
  fn owns_program(&self, machine: &Machine, args: &[String], env: &BTreeMap<String, String>) -> bool {
    member::owns(machine, args, env, self.name())
  }

  // No member publishes a cache lifetime, so nothing keeps a place warm.
  fn place_cache(&self, _model: &str, _upstream: Option<&str>) -> Result<PlaceCache, String> {
    Err("Codex's members publish no cache lifetime, so there is no clock to keep one warm by.".to_owned())
  }

  fn missing(&self, machine: &Machine) -> Option<Reason> {
    member::missing(machine, self.name(), &CODEX.presence.missing_code, &CODEX.presence.missing)
  }

  fn not_registered(&self) -> Option<&'static str> {
    Some(&CODEX.presence.not_held)
  }

  fn models_on_gateway(&self) -> bool {
    true
  }

  fn holds_model(&self, machine: &Machine, model: &str) -> bool {
    member::member_named(machine, self.name(), model).ok().flatten().is_some()
  }

  fn catalogue<'a>(&'a self, machine: &'a Machine, _config_dir: Option<&'a Path>) -> Reading<'a> {
    Box::pin(async move {
      let held = member::road_members(machine, self.name())?;
      let models: Vec<Value> = held
        .iter()
        .map(|one| {
          json!({
            "id": one.member.name,
            "name": one.member.label.clone().unwrap_or_else(|| one.member.name.clone()),
            "contextLength": one.context_tokens,
            "defaultReasoningEffort": if one.thinking { "medium" } else { "none" },
            "supportedReasoningEfforts": &CODEX.declared.effort.words,
          })
        })
        .collect();

      Ok(json!({ "models": models }))
    })
  }

  fn offered<'a>(
    &'a self,
    machine: &'a Machine,
    _config_dir: Option<&'a Path>,
    _now_ms: f64,
  ) -> Offering<'a> {
    Box::pin(async move {
      let efforts: Vec<Option<String>> = CODEX
        .declared
        .effort
        .vocabulary()
        .into_iter()
        .map(|word| word.map(str::to_owned))
        .collect();
      let models = member::road_members(machine, self.name())?.into_iter().map(|one| OfferedModel {
        id: one.member.name,
        efforts: efforts.clone(),
        free: None,
        in_service: Some(true),
      });

      Ok(Offered {
        models: models.collect(),
        stale_read_at: None,
      })
    })
  }

  fn claude_effort(&self, word: &str) -> Option<&'static str> {
    CODEX.claude.as_ref().and_then(|claude| claude.word(word))
  }

  fn session_records(
    &self,
    machine: &Machine,
    _logins: &[PathBuf],
    cwd: &Path,
    session: &str,
    model: &str,
  ) -> Vec<PathBuf> {
    member::session_records(machine, self.name(), cwd, session, model)
  }

  fn home_records(&self, machine: &Machine, _logins: &[PathBuf], cwd: &Path) -> Vec<PathBuf> {
    member::home_records(machine, self.name(), cwd)
  }

  fn forget_prompts(
    &self,
    machine: &Machine,
    _logins: &[PathBuf],
    cwd: &Path,
    session: Option<&str>,
  ) -> Result<usize, String> {
    member::forget_prompts(machine, self.name(), cwd, session)
  }
}

/// One model the installed CLI offers a chooser.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
  pub id: String,
  pub name: String,
  pub description: String,
  pub context_length: Option<i64>,
  pub default_reasoning_effort: String,
  pub supported_reasoning_efforts: Vec<String>,
}

/// The catalogue as the CLI prints it. Hidden compatibility aliases stay hidden: a chooser offers the
/// models the CLI offers a person, not every slug its parser still understands.
pub fn read_models(text: &str) -> Result<Vec<Model>, String> {
  let payload: Value = serde_json::from_str(text).map_err(|error| format!("codex models: {error}"))?;
  let rows = payload
    .get("models")
    .and_then(Value::as_array)
    .ok_or("codex models: the CLI returned no model list")?;

  Ok(
    rows
      .iter()
      .filter(|row| row.get("visibility").and_then(Value::as_str) != Some("hide"))
      .filter_map(|row| {
        let id = row.get("slug")?.as_str().filter(|slug| !slug.is_empty())?;
        let text = |name: &str| row.get(name).and_then(Value::as_str).map(str::to_owned);

        Some(Model {
          id: id.to_owned(),
          name: text("display_name").unwrap_or_else(|| id.to_owned()),
          description: text("description").unwrap_or_default(),
          context_length: row.get("context_window").and_then(Value::as_f64).map(|length| length as i64),
          default_reasoning_effort: text("default_reasoning_level").unwrap_or_else(|| "medium".to_owned()),
          supported_reasoning_efforts: row
            .get("supported_reasoning_levels")
            .and_then(Value::as_array)
            .map(|levels| {
              levels
                .iter()
                .filter_map(|level| level.get("effort")?.as_str().map(str::to_owned))
                .collect()
            })
            .unwrap_or_default(),
        })
      })
      .collect(),
  )
}

/// What a failed run of the CLI said: the last error its event stream reported, or else the last line
/// of its standard error, or else its exit status.
pub fn failure(stdout: &str, stderr: &str, status: Option<i32>) -> String {
  let mut reported = stdout
    .lines()
    .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
    .filter_map(|event| {
      let message = match event.get("type").and_then(Value::as_str) {
        Some("error") => event.get("message"),
        Some("turn.failed") => event.pointer("/error/message"),
        _ => None,
      };

      message.and_then(Value::as_str).filter(|message| !message.is_empty()).map(str::to_owned)
    });

  reported
    .next_back()
    .or_else(|| stderr.lines().map(str::trim).rfind(|line| !line.is_empty()).map(str::to_owned))
    .unwrap_or_else(|| {
      format!("exit {}", status.map_or_else(|| "null".to_owned(), |code| code.to_string()))
    })
}

/// Every model the installed executable offers, its answer waited for until the run ends.
pub async fn models(machine: &Machine) -> Result<Vec<Model>, String> {
  let program = machine
    .program(&CODEX.catalogue.program)
    .ok_or_else(|| CODEX.presence.missing.clone())?;
  let mut command = Command::new(program);

  command
    .args(&CODEX.catalogue.args)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

  if let Some(path) = machine.var("PATH") {
    command.env("PATH", path);
  }

  let child = command.spawn().map_err(|error| format!("codex models: {error}"))?;
  let output = child.wait_with_output().await.map_err(|error| format!("codex models: {error}"))?;
  let stdout = String::from_utf8_lossy(&output.stdout);

  if !output.status.success() {
    return Err(failure(&stdout, &String::from_utf8_lossy(&output.stderr), output.status.code()));
  }

  read_models(&stdout)
}
