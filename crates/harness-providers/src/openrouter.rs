//! OpenRouter: present where the gateway's key file and its members' key files stand private under the data
//! root, and its models the members of its road (docs/provider-presence.md). Its program is Claude Code on
//! agency's gateway. The public listing is still read for the chooser that declares a member.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use policy::providers::Declared;
use policy::providers::openrouter::OPENROUTER;
use serde::Serialize;
use serde_json::{Value, json};

use crate::balance::{Balance, OffersBalance, Requests, figure, key_in_file};
use crate::claude_code::ClaudeCode;
use crate::member;
use crate::{Machine, Offered, OfferedModel, Offering, PlaceCache, Provider, Reading, Reason};

pub struct OpenRouter;

/// The end of a model id OpenRouter serves at no cost.
const FREE_SUFFIX: &str = ":free";

// OpenRouter's balance: asked with the member's key file, the credits bought less the credits used, in the
// currency its policy names; and beside it the day's allowance of free-model requests. OpenRouter still
// answers its free models on a spent balance, so the balance closes only a member that costs: one whose
// `free` is false, or that leaves `free` out and names no free model id.
impl OffersBalance for OpenRouter {
  fn spent_closes(&self, machine: &Machine, model: &str) -> bool {
    !member::free(machine, self.name(), model, |member| member.model.ends_with(FREE_SUFFIX))
  }

  fn declared(&self) -> &'static policy::providers::Balance {
    &OPENROUTER.balance
  }

  fn key(&self, machine: &Machine) -> Result<String, String> {
    key_in_file(&machine.data_root().join("env").join(&OPENROUTER.presence.key_file))
  }

  fn read(&self, answer: &Value) -> Result<Balance, String> {
    let data = answer.get("data").ok_or("the answer states no credits")?;
    let credits = figure(data.get("total_credits")).ok_or("the answer states no total credits")?;
    let usage = figure(data.get("total_usage")).ok_or("the answer states no total usage")?;

    Ok(Balance {
      amount: credits - usage,
      currency: OPENROUTER.balance.currency.clone().unwrap_or_default(),
    })
  }

  // The day's allowance of free-model requests, as the key's own answer states it.
  fn requests(&self, answer: &Value) -> Option<Requests> {
    let stated = answer.get("data")?.get("free_model_daily_requests")?;
    let count = |name: &str| stated.get(name).and_then(Value::as_i64);

    Some(Requests {
      used: count("used")?,
      limit: count("limit")?,
      left: count("remaining")?,
      scope: OPENROUTER.balance.requests_scope.clone(),
    })
  }
}

impl Provider for OpenRouter {
  fn name(&self) -> &'static str {
    "openrouter"
  }

  fn key_file(&self) -> Option<&'static str> {
    Some(OPENROUTER.presence.key_file.as_str())
  }

  fn balance(&self) -> Option<&'static dyn OffersBalance> {
    Some(&OpenRouter)
  }

  fn declared(&self) -> &'static Declared {
    &OPENROUTER.declared
  }

  // No member publishes a cache lifetime, so nothing keeps a place warm.
  fn place_cache(&self, _model: &str, _upstream: Option<&str>) -> Result<PlaceCache, String> {
    Err(
      "OpenRouter's members publish no cache lifetime, so there is no clock to keep one warm by."
        .to_owned(),
    )
  }

  // Its program is Claude Code under one of its road's homes.
  fn owns_program(&self, machine: &Machine, args: &[String], env: &BTreeMap<String, String>) -> bool {
    member::owns(machine, args, env, self.name())
  }

  fn missing(&self, machine: &Machine) -> Option<Reason> {
    member::missing(machine, self.name(), &OPENROUTER.presence.missing_code, &OPENROUTER.presence.missing)
  }

  fn not_registered(&self) -> Option<&'static str> {
    Some(&OPENROUTER.presence.not_held)
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
            "supportedReasoningEfforts": &OPENROUTER.declared.effort.words,
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
      let efforts: Vec<Option<String>> = OPENROUTER
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
    OPENROUTER.claude.as_ref().and_then(|claude| claude.word(word))
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

/// What a chooser needs to know about one model: which, what it is called, how much it can be told,
/// and whether it costs anything.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
  pub id: String,
  pub name: String,
  pub context_length: Option<i64>,
  pub free: bool,
}

/// A price as the listing writes it, read as a number the way a person reads one; nothing where it
/// is none.
fn price(value: Option<&Value>) -> Option<f64> {
  match value? {
    Value::Number(number) => number.as_f64(),
    Value::String(text) => text.trim().parse().ok(),
    _ => None,
  }
}

/// Whether a listed model costs nothing: a `:free` id, priced at zero for both prompt and completion.
fn free(id: &str, pricing: Option<&Value>) -> bool {
  let zero = |name: &str| price(pricing.and_then(|pricing| pricing.get(name))) == Some(0.0);

  id.ends_with(FREE_SUFFIX) && zero("prompt") && zero("completion")
}

/// The listing as the gateway answers it.
pub fn read_models(payload: &Value) -> Vec<Model> {
  payload
    .get("data")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|row| {
      let id = row.get("id")?.as_str().filter(|id| !id.is_empty())?;

      Some(Model {
        id: id.to_owned(),
        name: row.get("name").and_then(Value::as_str).unwrap_or(id).to_owned(),
        context_length: row.get("context_length").and_then(Value::as_f64).map(|length| length as i64),
        free: free(id, row.get("pricing")),
      })
    })
    .collect()
}

/// Where the listing is read from: the stand-in's address where the environment names one.
fn listing(machine: &Machine) -> &str {
  machine.var(&OPENROUTER.catalogue.url_env).unwrap_or(&OPENROUTER.catalogue.url)
}

/// Every model OpenRouter offers, for a screen to choose a stand-in from.
pub async fn models(machine: &Machine) -> Result<Vec<Model>, String> {
  let response = reqwest::Client::new()
    .get(listing(machine))
    .header(reqwest::header::ACCEPT, "application/json")
    .send()
    .await
    .map_err(|error| format!("openrouter models: {error}"))?;
  let status = response.status();

  if !status.is_success() {
    return Err(
      format!(
        "openrouter models: HTTP {} {}",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
      )
      .trim_end()
      .to_owned(),
    );
  }

  let payload: Value = response.json().await.map_err(|error| format!("openrouter models: {error}"))?;

  Ok(read_models(&payload))
}
