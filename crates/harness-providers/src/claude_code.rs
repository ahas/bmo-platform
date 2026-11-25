//! Claude Code: always present, since whether one of its logins answers is the account walk's to say,
//! and its catalogue read off the disk where the CLI keeps it for a login (docs/model-tiers.md, 「The
//! Claude catalogue」).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use policy::providers::Declared;
use policy::providers::claude_code::CLAUDE_CODE;
use regex::Regex;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
  Headline, Judged, Machine, ModelEfforts, Offered, OfferedModel, Offering, PlaceCache, Provider, Reading,
  Reason, Window, number,
};

pub struct ClaudeCode;

// The word a model's name follows `claude-` with.
static FAMILY: LazyLock<Regex> = LazyLock::new(|| Regex::new("^claude-([a-z]+)").expect("a pattern"));

impl ClaudeCode {
  /// The program's executable, as every line of it starts it.
  pub const PROGRAM: &'static str = "claude";

  /// The family a model's name places it in: the word the short names and an account's model map go by,
  /// and the name itself where it states no family.
  pub fn family(model: &str) -> &str {
    FAMILY
      .captures(model)
      .and_then(|found| found.get(1))
      .map_or(model, |found| found.as_str())
  }

  /// The machine's own configuration directory of the program: the one the environment names, or the
  /// home's.
  pub fn machine_config_dir(machine: &Machine) -> PathBuf {
    machine
      .var("CLAUDE_CONFIG_DIR")
      .map_or_else(|| Path::new(machine.var("HOME").unwrap_or_default()).join(".claude"), PathBuf::from)
  }

  /// Every configuration directory a run of the program on this machine may have kept its records under: the
  /// machine's own, then the home of each login the caller holds, each once.
  pub fn config_dirs(machine: &Machine, logins: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = vec![Self::machine_config_dir(machine)];

    for login in logins {
      if !dirs.contains(login) {
        dirs.push(login.clone());
      }
    }

    dirs
  }

  /// The folder the program keeps a working directory's sessions in under a configuration directory: named
  /// after the working directory's real path, every link followed as the program's own working directory is,
  /// with every character but a letter or a digit turned into a hyphen; the path as given where it does not
  /// stand.
  pub fn project_folder(config_dir: &Path, cwd: &Path) -> PathBuf {
    let real = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let project: String = real
      .to_string_lossy()
      .chars()
      .map(|one| if one.is_ascii_alphanumeric() { one } else { '-' })
      .collect();

    config_dir.join("projects").join(project)
  }

  /// What the program keeps of one session under a configuration directory: the transcript, and the
  /// folder of the same name its children's transcripts and large tool answers go to, both in the
  /// working directory's project folder.
  pub fn records_under(config_dir: &Path, cwd: &Path, session: &str) -> Vec<PathBuf> {
    let folder = Self::project_folder(config_dir, cwd);

    vec![folder.join(format!("{session}.jsonl")), folder.join(session)]
  }

  /// What the program keeps of every session it ran in a working directory under a configuration directory,
  /// as `records_under` names one session's, whether any room recorded the session or not. The project
  /// folder's other files are not a session's and stay.
  pub fn every_record_under(config_dir: &Path, cwd: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(Self::project_folder(config_dir, cwd)) else {
      return vec![];
    };

    entries
      .flatten()
      .filter_map(|entry| entry.file_name().to_str()?.strip_suffix(".jsonl").map(str::to_owned))
      .flat_map(|session| Self::records_under(config_dir, cwd, &session))
      .collect()
  }

  /// The prompts the program kept in its history under a configuration directory of what was put in while it
  /// worked in a working directory, of one session where `session` names one, taken out of it; how many. The
  /// history is rewritten under the lock the program itself takes to write it, a directory beside the file of
  /// its name and `.lock`, made by whoever holds it: a program writing meanwhile waits and writes after. Every
  /// other line stays as it stands.
  pub fn forget_prompts_under(
    config_dir: &Path,
    cwd: &Path,
    session: Option<&str>,
  ) -> Result<usize, String> {
    let Ok(history) = config_dir.join(HISTORY).canonicalize() else {
      return Ok(0);
    };
    let _held = HistoryLock::take(&history)?;
    let said = |why: std::io::Error| format!("{}: {why}", history.display());
    let text = std::fs::read_to_string(&history).map_err(said)?;
    let mut kept = String::with_capacity(text.len());
    let mut forgotten = 0;

    for line in text.split_inclusive('\n') {
      let entry = serde_json::from_str::<Value>(line).ok();
      let project = entry.as_ref().and_then(|entry| entry.get("project")).and_then(Value::as_str);
      let of_session = session.is_none_or(|session| {
        entry.as_ref().and_then(|entry| entry.get("sessionId")).and_then(Value::as_str) == Some(session)
      });

      if project.is_some_and(|project| Path::new(project).starts_with(cwd)) && of_session {
        forgotten += 1;
      } else {
        kept.push_str(line);
      }
    }

    if forgotten > 0 {
      let fresh = history.with_extension(format!("jsonl.{}.forgetting", std::process::id()));

      std::fs::write(&fresh, kept)
        .and_then(|()| std::fs::set_permissions(&fresh, std::fs::metadata(&history)?.permissions()))
        .and_then(|()| std::fs::rename(&fresh, &history))
        .map_err(said)?;
    }

    Ok(forgotten)
  }

  /// The prompts taken out of the history under each of several configuration directories, as
  /// `forget_prompts_under` takes them out of one; how many in all, or what every directory that refused
  /// said.
  pub fn forget_prompts_in_each(
    config_dirs: impl IntoIterator<Item = PathBuf>,
    cwd: &Path,
    session: Option<&str>,
  ) -> Result<usize, String> {
    let mut forgotten = 0;
    let mut refused = vec![];

    for config_dir in config_dirs {
      match Self::forget_prompts_under(&config_dir, cwd, session) {
        Ok(count) => forgotten += count,
        Err(why) => refused.push(why),
      }
    }

    if refused.is_empty() { Ok(forgotten) } else { Err(refused.join("; ")) }
  }
}

/// The program's history of what was put in, in its configuration directory.
pub const HISTORY: &str = "history.jsonl";

// How often, and how long apart, the history's lock is tried before it is given up on.
const HISTORY_LOCK_TRIES: usize = 40;
const HISTORY_LOCK_PAUSE: std::time::Duration = std::time::Duration::from_millis(25);

// The program's own lock on its history, held until dropped.
struct HistoryLock(PathBuf);

impl HistoryLock {
  fn take(history: &Path) -> Result<Self, String> {
    let path = PathBuf::from(format!("{}.lock", history.display()));

    for _ in 0..HISTORY_LOCK_TRIES {
      match std::fs::create_dir(&path) {
        Ok(()) => return Ok(Self(path)),
        Err(why) if why.kind() == std::io::ErrorKind::AlreadyExists => {
          std::thread::sleep(HISTORY_LOCK_PAUSE)
        }
        Err(why) => return Err(format!("{}: {why}", path.display())),
      }
    }

    Err(format!("{}: held by another the whole while", path.display()))
  }
}

impl Drop for HistoryLock {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir(&self.0);
  }
}

impl Provider for ClaudeCode {
  fn name(&self) -> &'static str {
    "claude-code"
  }

  fn declared(&self) -> &'static Declared {
    &CLAUDE_CODE.declared
  }

  fn missing(&self, _machine: &Machine) -> Option<Reason> {
    None
  }

  fn catalogue_under_login(&self) -> bool {
    true
  }

  // A pin names one model its CLI's catalogue names.
  fn pins_models(&self) -> bool {
    true
  }

  // A login's own program only: one standing on a registration's road — its home, or the gateway's address —
  // stands for that road, while an account's own endpoint does not take a program off this one.
  fn owns_program(&self, machine: &Machine, args: &[String], env: &BTreeMap<String, String>) -> bool {
    args.first().is_some_and(|program| program == Self::PROGRAM)
      && crate::registered::road_home(machine, env).is_none()
      && !crate::registered::registered_road(machine, env)
  }

  // The account's own five-hour and seven-day windows.
  fn headline(&self, windows: &[Window]) -> Headline {
    let named = |name: &str| windows.iter().position(|one| one.name == name);

    Headline {
      session: named("five_hour"),
      week: named("seven_day"),
    }
  }

  // Its logins are the Claude accounts, walked in the owner's order.
  fn holds_logins(&self) -> bool {
    true
  }

  // A directory account's login home holds the CLI's own state file, as its policy file names it.
  fn login_home_mark(&self) -> Option<&'static str> {
    Some(CLAUDE_CODE.login.home_mark.as_str())
  }

  // Its shell, its sub-agents and its fetches from the web, as its policy file names them.
  fn long_tools(&self) -> &'static [String] {
    &CLAUDE_CODE.launch.long_tools
  }

  // The CLI names the session every one of its answers was written in.
  fn session_of_event(&self, event: &serde_json::Value) -> Option<String> {
    event.get("session_id").and_then(serde_json::Value::as_str).map(str::to_owned)
  }

  // What the model wrote, and the name of each tool it called — a call through the toolbook as the tool it
  // reached — out of a streamed response; a run printing one object at its end streams nothing that reads so.
  fn progress_of(&self, line: &str) -> Option<String> {
    let event: serde_json::Value = serde_json::from_str(line).ok()?;
    let blocks = event
      .pointer("/message/content")
      .and_then(serde_json::Value::as_array)
      .filter(|_| event.get("type").and_then(serde_json::Value::as_str) == Some("assistant"))?;
    let said: Vec<String> = blocks
      .iter()
      .map(|block| {
        let text = |key: &str| block.get(key).and_then(serde_json::Value::as_str);

        match text("type") {
          Some("text") => text("text").unwrap_or_default().to_owned(),
          Some("tool_use") => text("name").map_or_else(String::new, |name| {
            let input = block.get("input");
            let reached = input
              .filter(|input| input.get("arguments").is_some_and(serde_json::Value::is_object))
              .and_then(|input| input.get("tool").and_then(serde_json::Value::as_str))
              .and_then(|inner| crate::toolbook::reached(name, inner));

            format!("→ {}", reached.as_deref().unwrap_or(name))
          }),
          _ => String::new(),
        }
      })
      .filter(|part| !part.trim().is_empty())
      .collect();

    (!said.is_empty()).then(|| said.join(" "))
  }

  fn cache_minutes(&self, _model: &str) -> Option<i64> {
    Some(CLAUDE_CODE.cache.ttl_minutes)
  }

  // One lifetime for every model it runs.
  fn place_cache(&self, _model: &str, _upstream: Option<&str>) -> Result<PlaceCache, String> {
    Ok(PlaceCache {
      ttl_minutes: CLAUDE_CODE.cache.ttl_minutes,
      idle_minutes: CLAUDE_CODE.cache.keepalive_idle_minutes,
    })
  }

  // Its places are the account's prefixes, which a rest of the account leaves to cool.
  fn rests_with_account(&self) -> bool {
    true
  }

  // A session is written under the configuration directory of the login it went out on, the machine's own
  // where it names none, so it is looked for under each of them.
  fn session_records(
    &self,
    machine: &Machine,
    logins: &[PathBuf],
    cwd: &Path,
    session: &str,
    _model: &str,
  ) -> Vec<PathBuf> {
    Self::config_dirs(machine, logins)
      .iter()
      .flat_map(|config_dir| Self::records_under(config_dir, cwd, session))
      .collect()
  }

  fn home_records(&self, machine: &Machine, logins: &[PathBuf], cwd: &Path) -> Vec<PathBuf> {
    Self::config_dirs(machine, logins)
      .iter()
      .flat_map(|config_dir| Self::every_record_under(config_dir, cwd))
      .collect()
  }

  fn forget_prompts(
    &self,
    machine: &Machine,
    logins: &[PathBuf],
    cwd: &Path,
    session: Option<&str>,
  ) -> Result<usize, String> {
    Self::forget_prompts_in_each(Self::config_dirs(machine, logins), cwd, session)
  }

  // The words the model's own entry names; every word the harness takes where the entry names none,
  // the catalogue does not name the model, or it cannot be read at all.
  fn model_efforts<'a>(
    &'a self,
    _machine: &'a Machine,
    config_dir: Option<&'a Path>,
    model: &'a str,
  ) -> ModelEfforts<'a> {
    Box::pin(async move {
      let named = config_dir
        .and_then(|dir| catalogue(dir).ok())
        .and_then(|read| read.models.into_iter().find(|one| one.id == model))
        .map(|one| one.efforts)
        .filter(|efforts| !efforts.is_empty());

      Some(named.unwrap_or_else(|| CLAUDE_CODE.declared.effort.words.clone()))
    })
  }

  // Its program consults the advisor a road names, Anthropic's own service running it.
  fn takes_advisor(&self) -> bool {
    true
  }

  fn advisor_words(&self) -> &'static [String] {
    &CLAUDE_CODE.advisor.aliases
  }

  // A family word or a model of the catalogue, paired with the road's model as the declaration states the
  // program's pairings (docs/provider-order.md, 「The advisor」). A family word the catalogue cannot be read for
  // stands as a model the declaration's pairing judges by its own name; a model the catalogue does not name is
  // refused only where the catalogue reads.
  fn advisor_refusal<'a>(
    &'a self,
    _machine: &'a Machine,
    config_dir: Option<&'a Path>,
    model: &'a str,
    advisor: &'a str,
  ) -> Judged<'a> {
    Box::pin(async move {
      let declared = &CLAUDE_CODE.advisor;
      let read = config_dir.and_then(|dir| catalogue(dir).ok());
      let alias = declared.is_alias(advisor);
      let resolved = match (&read, alias) {
        (Some(read), true) => read.aliases.get(&advisor.to_ascii_lowercase()).cloned(),
        (Some(read), false) if !read.models.iter().any(|one| one.id == advisor) => {
          return Some(format!("Claude Code holds no model {advisor} to advise with."));
        }
        (_, false) => Some(advisor.to_owned()),
        (None, true) => None,
      };

      if declared.pair_of(model).is_none() {
        return Some(format!("Claude Code pairs no advisor with {model}."));
      }

      match resolved {
        Some(resolved) if !declared.pairs(model, &resolved) => {
          Some(advisor_below(model, advisor, &resolved))
        }
        _ => None,
      }
    })
  }

  fn catalogue<'a>(&'a self, _machine: &'a Machine, config_dir: Option<&'a Path>) -> Reading<'a> {
    Box::pin(async move {
      let dir = config_dir.ok_or("the Claude catalogue is read under a login's configuration directory")?;
      let catalogue = catalogue(dir)?;

      Ok(json!({
        "models": catalogue.models,
        "fetchedAt": number(catalogue.fetched_at),
        "staleAt": number(catalogue.stale_at),
      }))
    })
  }

  // A model whose entry names no effort — one that does not think — takes every word this harness
  // takes, since a name missing from a list is not evidence against it (docs/model-tiers.md, 「The
  // Claude catalogue」). A stale read is served as it stands and says when it was taken.
  fn offered<'a>(
    &'a self,
    _machine: &'a Machine,
    config_dir: Option<&'a Path>,
    now_ms: f64,
  ) -> Offering<'a> {
    Box::pin(async move {
      let dir = config_dir.ok_or("the Claude catalogue is read under a login's configuration directory")?;
      let read = catalogue(dir)?;
      let every = &CLAUDE_CODE.declared.effort.words;
      let models = read.models.iter().map(|model| OfferedModel {
        id: model.id.clone(),
        efforts: if model.efforts.is_empty() { every } else { &model.efforts }
          .iter()
          .cloned()
          .map(Some)
          .collect(),
        free: None,
        in_service: Some(model.in_service),
      });

      Ok(Offered {
        models: models.collect(),
        stale_read_at: read.stale(now_ms).then_some(read.fetched_at),
      })
    })
  }
}

// The sentence an advisor the model is not paired with is refused with, naming the model a family word stands for.
fn advisor_below(model: &str, advisor: &str, resolved: &str) -> String {
  let named = if advisor == resolved { advisor.to_owned() } else { format!("{advisor} ({resolved})") };

  format!(
    "Claude Code does not attach {named} as the advisor of {model}: an advisor ranks at or above the model it advises."
  )
}

/// One model as the catalogue names it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
  pub id: String,
  /// Whether the CLI serves this generation or has superseded it.
  pub in_service: bool,
  /// The efforts its own entry names, in the catalogue's order; empty where it does not think.
  pub efforts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Catalogue {
  pub models: Vec<Model>,
  /// The model each family word stands for now, as the CLI resolves its aliases.
  pub aliases: BTreeMap<String, String>,
  /// When the CLI fetched it, and when it holds it worth fetching again.
  pub fetched_at: f64,
  pub stale_at: f64,
}

impl Catalogue {
  /// Whether it is past the moment the CLI itself set for fetching it again.
  pub fn stale(&self, now_ms: f64) -> bool {
    self.stale_at <= now_ms
  }
}

fn finite(value: Option<&Value>) -> f64 {
  value.and_then(Value::as_f64).filter(|one| one.is_finite()).unwrap_or(0.0)
}

// The model each family word stands for now: the catalogue's own alias targets, each its default.
fn aliases(payload: &Value) -> BTreeMap<String, String> {
  let Some(targets) = payload.pointer("/catalog/config/provider_alias_targets").and_then(Value::as_object)
  else {
    return BTreeMap::new();
  };

  targets
    .iter()
    .filter_map(|(alias, target)| {
      let model = target.get("default")?.as_str().filter(|model| !model.is_empty())?;

      Some((alias.clone(), model.to_owned()))
    })
    .collect()
}

fn models(payload: &Value) -> Vec<Model> {
  let Some(rows) = payload.pointer("/catalog/config/models").and_then(Value::as_array) else {
    return Vec::new();
  };

  rows
    .iter()
    .filter_map(|row| {
      let id = row.get("id")?.as_str().filter(|id| !id.is_empty())?;
      let efforts = row
        .pointer("/thinking/effort_options")
        .and_then(Value::as_array)
        .map(|options| {
          options
            .iter()
            .filter_map(|option| option.get("id")?.as_str().filter(|id| !id.is_empty()).map(str::to_owned))
            .collect()
        })
        .unwrap_or_default();

      Some(Model {
        id: id.to_owned(),
        in_service: row.get("section").and_then(Value::as_str) == Some("main"),
        efforts,
      })
    })
    .collect()
}

/// The catalogue standing under one configuration directory. Where it holds more than one file of the
/// surface the newest fetch answers and an unreadable neighbour is passed over; a directory that is
/// not there, holds no such file, holds nothing that parses, or holds a catalogue naming no model is a
/// refusal rather than an empty list.
pub fn catalogue(config_dir: &Path) -> Result<Catalogue, String> {
  let declared = &CLAUDE_CODE.catalogue;
  let dir = config_dir.join(&declared.dir);
  let shown = dir.display();
  let entries =
    std::fs::read_dir(&dir).map_err(|error| format!("no model catalogue under {shown}: {error}"))?;
  let mut names: Vec<String> = entries
    .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
    .filter(|name| name.ends_with(&declared.suffix))
    .collect();

  if names.is_empty() {
    return Err(format!("no model catalogue file under {shown}"));
  }

  names.sort();

  let mut read = Vec::new();
  let mut refused = None;

  for name in names {
    let path = dir.join(&name);
    let parsed = std::fs::read_to_string(&path)
      .map_err(|error| error.to_string())
      .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|error| error.to_string()));

    match parsed {
      Ok(payload) => read.push(Catalogue {
        models: models(&payload),
        aliases: aliases(&payload),
        fetched_at: finite(payload.get("fetchedAt")),
        stale_at: finite(payload.get("staleAt")),
      }),
      Err(error) => {
        refused
          .get_or_insert_with(|| format!("model catalogue {} could not be read: {error}", path.display()));
      }
    }
  }

  // The newest fetch, and of two fetched at once the first by name, as a stable sort leaves it.
  read.sort_by(|one, other| other.fetched_at.total_cmp(&one.fetched_at));

  let newest = read
    .into_iter()
    .next()
    .ok_or_else(|| refused.unwrap_or_else(|| format!("no model catalogue under {shown}")))?;

  if newest.models.is_empty() {
    return Err(format!("the model catalogue under {shown} names no model"));
  }

  Ok(newest)
}
