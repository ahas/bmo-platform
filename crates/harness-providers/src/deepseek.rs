//! DeepSeek: present on this machine where the gateway's key file and the key files of its members stand
//! private under the data root, and its models the members of its road (docs/provider-presence.md). Its
//! program is Claude Code on agency's gateway.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use policy::providers::Declared;
use policy::providers::deepseek::DEEPSEEK;
use serde_json::json;

use crate::balance::{Balance, OffersBalance, figure, key_in_file};
use crate::claude_code::ClaudeCode;
use crate::member;
use crate::{Machine, Offered, OfferedModel, Offering, PlaceCache, Provider, Reading, Reason};

pub struct DeepSeek;

impl DeepSeek {
  /// Where the road's key file stands, for the balance's reading.
  pub fn key_file(machine: &Machine) -> PathBuf {
    machine.data_root().join("env").join(&DEEPSEEK.presence.key_file)
  }
}

// DeepSeek's balance: asked with the road's key file, the first of the answer's balances the reading. It
// closes every member but one whose `free` says it costs nothing.
impl OffersBalance for DeepSeek {
  fn spent_closes(&self, machine: &Machine, model: &str) -> bool {
    !member::free(machine, self.name(), model, |_| false)
  }

  fn declared(&self) -> &'static policy::providers::Balance {
    &DEEPSEEK.balance
  }

  fn key(&self, machine: &Machine) -> Result<String, String> {
    key_in_file(&Self::key_file(machine))
  }

  fn read(&self, answer: &serde_json::Value) -> Result<Balance, String> {
    let first = answer
      .get("balance_infos")
      .and_then(serde_json::Value::as_array)
      .and_then(|infos| infos.first())
      .ok_or("the answer states no balance")?;
    let amount = figure(first.get("total_balance")).ok_or("the balance states no total")?;
    let currency = first
      .get("currency")
      .and_then(serde_json::Value::as_str)
      .filter(|currency| !currency.is_empty())
      .ok_or("the balance states no currency")?;

    Ok(Balance {
      amount,
      currency: currency.to_owned(),
    })
  }
}

impl Provider for DeepSeek {
  fn name(&self) -> &'static str {
    "deepseek"
  }

  fn key_file(&self) -> Option<&'static str> {
    Some(DEEPSEEK.presence.key_file.as_str())
  }

  fn balance(&self) -> Option<&'static dyn OffersBalance> {
    Some(&DeepSeek)
  }

  fn declared(&self) -> &'static Declared {
    &DEEPSEEK.declared
  }

  fn missing(&self, machine: &Machine) -> Option<Reason> {
    member::missing(machine, self.name(), &DEEPSEEK.presence.missing_code, &DEEPSEEK.presence.missing)
  }

  fn not_registered(&self) -> Option<&'static str> {
    Some(&DEEPSEEK.presence.not_held)
  }

  fn models_on_gateway(&self) -> bool {
    true
  }

  fn holds_model(&self, machine: &Machine, model: &str) -> bool {
    member::member_named(machine, self.name(), model).ok().flatten().is_some()
  }

  // Its catalogue is the members its road holds: the window and the answer are the member's, and its
  // efforts are the words the deployment gives the road.
  fn catalogue<'a>(&'a self, machine: &'a Machine, _config_dir: Option<&'a Path>) -> Reading<'a> {
    Box::pin(async move {
      let held = member::road_members(machine, self.name())?;
      let models: Vec<serde_json::Value> = held
        .iter()
        .map(|one| {
          json!({
            "id": one.member.name,
            "name": one.member.label.clone().unwrap_or_else(|| one.member.name.clone()),
            "contextLength": one.context_tokens,
            "defaultReasoningEffort": if one.thinking { "high" } else { "none" },
            "supportedReasoningEfforts": &DEEPSEEK.declared.effort.words,
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
      let efforts: Vec<Option<String>> = DEEPSEEK
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

  // DeepSeek publishes no cache lifetime, so nothing keeps a place warm.
  fn place_cache(&self, _model: &str, _upstream: Option<&str>) -> Result<PlaceCache, String> {
    Err("DeepSeek publishes no cache lifetime, so there is no clock to keep it warm by.".to_owned())
  }

  // A level the run chose reaches it where its model takes the scale's level; no thinking is DeepSeek's
  // own word `none`, and a mapping's own word stands otherwise.
  fn run_effort(
    &self,
    mapped: Option<&str>,
    thinking: Option<bool>,
    level: Option<&str>,
    levels: &[String],
  ) -> Option<String> {
    if thinking == Some(false) {
      return Some("none".to_owned());
    }

    match level {
      Some(level) if levels.iter().any(|one| one == level) => Some(level.to_owned()),
      _ => mapped.map(str::to_owned),
    }
  }

  fn claude_effort(&self, word: &str) -> Option<&'static str> {
    DEEPSEEK.claude.as_ref().and_then(|claude| claude.word(word))
  }

  // Its program is Claude Code under one of its road's homes.
  fn owns_program(&self, machine: &Machine, args: &[String], env: &BTreeMap<String, String>) -> bool {
    member::owns(machine, args, env, self.name())
  }

  fn session_of_event(&self, event: &serde_json::Value) -> Option<String> {
    ClaudeCode.session_of_event(event)
  }

  fn progress_of(&self, line: &str) -> Option<String> {
    ClaudeCode.progress_of(line)
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
