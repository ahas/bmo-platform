//! The model catalogue from the shell (docs/model-catalogue.md, 「Where it is set」): the model files the
//! deployment's own engines serve, read, made, changed and removed, and one chosen for its engine. An
//! engine's installer reads an entry and records where its file lies through the same verb, and a choice's
//! unit carries the choice out through it. `--db` names another store's address — a worker's workbench
//! copy, say.

use std::path::{Path, PathBuf};

use clap::Args;
use harness_machine::model_use::Install;
use policy::fill_marks;
use policy::verb_words::VERB_WORDS;
use sea_orm::DatabaseConnection;
use serde_json::{Map, Value};

use store::{one, run};

use super::{Answer, VerbError, deploy_root, store_at};
use crate::config::SOURCE_DIR;
use crate::control::models::verb as catalogue;

/// The verb's arguments: a step and an entry's name, or one of the installers' two flags, with the fields
/// a save names.
#[derive(Args, Debug, Default, Clone)]
pub struct ModelsArgs {
  /// add, set, remove, use, or run-use for a choice's unit.
  pub step: Option<String>,
  /// The entry's name, or the choice's number for run-use.
  pub name: Option<String>,
  #[arg(hide = true)]
  pub rest: Vec<String>,
  /// Another store's address.
  #[arg(long)]
  pub db: Option<String>,
  #[arg(long)]
  pub json: bool,
  /// One entry whole, for an installer.
  #[arg(long)]
  pub entry: Option<String>,
  /// The node an entry's file lies on, for an installer: <name> <node>.
  #[arg(long, num_args = 2, value_names = ["NAME", "NODE"])]
  pub placed: Option<Vec<String>>,
  #[arg(long)]
  pub label: Option<String>,
  #[arg(long)]
  pub file: Option<String>,
  #[arg(long)]
  pub bytes: Option<String>,
  #[arg(long)]
  pub sha256: Option<String>,
  #[arg(long)]
  pub source: Option<String>,
  #[arg(long)]
  pub engine: Option<String>,
  #[arg(long)]
  pub registration: Option<String>,
  #[arg(long)]
  pub note: Option<String>,
  #[arg(long)]
  pub thinking: Option<String>,
  #[arg(long)]
  pub temperature: Option<String>,
  #[arg(long = "top-p")]
  pub top_p: Option<String>,
  #[arg(long = "presence-penalty")]
  pub presence_penalty: Option<String>,
  #[arg(long = "max-tokens")]
  pub max_tokens: Option<String>,
}

// How a field the shell wrote is read into the body.
type Reading = dyn Fn(&str) -> Value;

// A refusal as the verb says one: its name before it, ending with code 2.
fn fail(line: impl Into<String>) -> VerbError {
  VerbError::Ended {
    code: 2,
    said: format!("models: {}", line.into()),
  }
}

// A number as the shell wrote it: a number where it reads as one, the text otherwise, for the save to
// refuse.
fn number(value: &str) -> Value {
  let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
  let unsigned = value.strip_prefix('-').unwrap_or(value);

  match unsigned.split_once('.') {
    None if digits(unsigned) => value.parse::<i64>().map_or_else(|_| value.into(), Value::from),
    Some((whole, fraction)) if digits(whole) && digits(fraction) => {
      value.parse::<f64>().map_or_else(|_| value.into(), Value::from)
    }
    _ => value.into(),
  }
}

fn on_off(value: &str) -> Value {
  match value {
    "on" => true.into(),
    "off" => false.into(),
    other => other.into(),
  }
}

impl ModelsArgs {
  /// The body a save names: each field given, empty where the shell said default or none.
  pub fn body(&self, action: &str) -> Value {
    let mut body = Map::new();
    let as_text = |value: &str| Value::from(value);
    let fields: [(&str, &Option<String>, &Reading); 13] = [
      ("label", &self.label, &as_text),
      ("file", &self.file, &as_text),
      ("bytes", &self.bytes, &number),
      ("sha256", &self.sha256, &as_text),
      ("source", &self.source, &as_text),
      ("engine", &self.engine, &as_text),
      ("registration", &self.registration, &as_text),
      ("note", &self.note, &as_text),
      ("thinking", &self.thinking, &on_off),
      ("temperature", &self.temperature, &number),
      ("topP", &self.top_p, &number),
      ("presencePenalty", &self.presence_penalty, &number),
      ("maxTokens", &self.max_tokens, &number),
    ];

    body.insert("action".into(), action.into());
    body.insert("name".into(), self.name.clone().unwrap_or_default().into());

    for (name, given, read) in fields {
      if let Some(value) = given {
        body.insert(
          name.into(),
          if value == "default" || value == "none" { Value::Null } else { read(value) },
        );
      }
    }

    Value::Object(body)
  }
}

fn shown(value: &Value) -> String {
  match value {
    Value::Null => "-".to_owned(),
    Value::String(text) => text.clone(),
    other => other.to_string(),
  }
}

// A count with its thousands set apart, as an English reader writes them.
fn grouped(number: i64) -> String {
  let digits = number.unsigned_abs().to_string();
  let mut out = String::new();

  for (at, digit) in digits.chars().enumerate() {
    if at > 0 && (digits.len() - at).is_multiple_of(3) {
      out.push(',');
    }

    out.push(digit);
  }

  if number < 0 { format!("-{out}") } else { out }
}

// A moment in unix seconds as an instant to the millisecond.
fn instant(seconds: i64) -> String {
  chrono::DateTime::from_timestamp(seconds, 0)
    .map_or_else(|| seconds.to_string(), |at| at.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
}

/// The catalogue as the bare verb prints it: the latest choice for each engine, then every entry.
pub fn listing(view: &Value) -> Vec<String> {
  let words = &VERB_WORDS.models.listing;
  let text = |value: &Value, key: &str| value.get(key).map(shown).unwrap_or_else(|| "-".to_owned());
  let mut lines = Vec::new();
  let uses = view.get("uses").and_then(Value::as_array).cloned().unwrap_or_default();

  for one in &uses {
    let optional = |key: &str, opening: &str| {
      one
        .get(key)
        .and_then(Value::as_str)
        .map(|said| format!("{opening}{said}"))
        .unwrap_or_default()
    };
    let step = one
      .get("step")
      .and_then(Value::as_str)
      .map(|step| format!(" ({step})"))
      .unwrap_or_default();

    lines.push(fill_marks(
      &words.use_line,
      &[
        ("engine", &text(one, "engine")),
        ("entry", &text(one, "entry")),
        ("chosen_by", &text(one, "chosenBy")),
        ("at", &instant(one.get("startedAt").and_then(Value::as_i64).unwrap_or(0))),
        ("state", &text(one, "state")),
        ("step", &step),
        ("reason", &optional("reason", " · ")),
      ],
    ));
  }

  if !uses.is_empty() {
    lines.push(String::new());
  }

  for entry in view.get("entries").and_then(Value::as_array).cloned().unwrap_or_default() {
    let serving = entry.get("serving").and_then(Value::as_bool) == Some(true);
    let nodes: Vec<String> = entry
      .get("nodes")
      .and_then(Value::as_array)
      .into_iter()
      .flatten()
      .filter_map(Value::as_str)
      .map(str::to_owned)
      .collect();
    let thinking = match entry.get("thinking").and_then(Value::as_bool) {
      None => "-".to_owned(),
      Some(true) => words.thinking_on.clone(),
      Some(false) => words.thinking_off.clone(),
    };
    let sha256 = text(&entry, "sha256");

    lines.push(format!(
      "{}{}: {}",
      text(&entry, "name"),
      if serving { words.serving.as_str() } else { "" },
      text(&entry, "label")
    ));
    lines.push(fill_marks(
      &words.file,
      &[
        ("file", &text(&entry, "file")),
        ("bytes", &grouped(entry.get("bytes").and_then(Value::as_i64).unwrap_or(0))),
        ("sha256", &sha256.chars().take(12).collect::<String>()),
      ],
    ));
    lines.push(fill_marks(
      &words.where_line,
      &[
        ("source", &text(&entry, "source")),
        ("engine", &text(&entry, "engine")),
        ("registration", &text(&entry, "registration")),
        ("nodes", &if nodes.is_empty() { "-".to_owned() } else { nodes.join(", ") }),
      ],
    ));
    lines.push(fill_marks(
      &words.defaults,
      &[
        ("thinking", &thinking),
        ("temperature", &text(&entry, "temperature")),
        ("top_p", &text(&entry, "topP")),
        ("presence_penalty", &text(&entry, "presencePenalty")),
        ("max_tokens", &text(&entry, "maxTokens")),
      ],
    ));

    if let Some(note) = entry.get("note").and_then(Value::as_str) {
      lines.push(fill_marks(&words.note, &[("note", note)]));
    }
  }

  lines
}

// The tree the binary runs out of: the deployed one's `bin/bmo-agency`, or the deploy tree where it runs
// from anywhere else.
fn code_root() -> PathBuf {
  std::env::current_exe()
    .ok()
    .and_then(|exe| {
      let bin = exe.parent()?;

      (bin.file_name()? == "bin").then(|| bin.parent().map(Path::to_path_buf)).flatten()
    })
    .unwrap_or_else(deploy_root)
}

/// A recorded choice carried out, as its unit runs it: the engine's installer in the source checkout run
/// with the entry's name, handed the store the choice was made in, its output written into the choice's
/// log; ended done or failed with why.
pub async fn run_use(
  db: &DatabaseConnection,
  data_root: &Path,
  place: &str,
  id: i64,
) -> Result<Answer, VerbError> {
  let words = &VERB_WORDS.models.run;
  let held =
    one(db, "SELECT entry, engine, state, log FROM model_uses WHERE id = $1", vec![id.into()]).await?;
  let Some(held) =
    held.filter(|row| row.try_get::<String>("", "state").is_ok_and(|state| state == "running"))
  else {
    return Err(fail(fill_marks(&words.not_running, &[("id", &id.to_string())])));
  };
  let entry: String = held.try_get("", "entry")?;
  let engine: String = held.try_get("", "engine")?;
  let log: Option<String> = held.try_get("", "log")?;

  run(
    db,
    "UPDATE model_uses SET step = $1 WHERE id = $2 AND state = 'running'",
    vec![words.installing.clone().into(), id.into()],
  )
  .await?;

  let install = Install {
    engine: engine.clone(),
    entry: entry.clone(),
    log: log.map_or_else(|| data_root.join("model-uses").join(format!("{engine}-{id}.log")), PathBuf::from),
    archive: place.to_owned(),
    source_root: std::env::var_os(SOURCE_DIR)
      .filter(|named| !named.is_empty())
      .map_or_else(code_root, PathBuf::from),
    bun: harness_providers::Machine::of_process(data_root)
      .program("bun")
      .unwrap_or_else(|| PathBuf::from("bun")),
  };
  let (state, reason) = match install.run().await {
    Ok(done) if done.ok => ("done", None),
    Ok(done) => ("failed", Some(if done.last.is_empty() { words.failed.clone() } else { done.last })),
    Err(why) => ("failed", Some(why.to_string())),
  };

  run(
    db,
    "UPDATE model_uses SET state = $1, reason = $2, ended_at = $3 WHERE id = $4 AND state = 'running'",
    vec![state.into(), reason.clone().into(), chrono::Utc::now().timestamp().into(), id.into()],
  )
  .await?;

  Ok(Answer {
    lines: vec![format!(
      "{engine} ← {entry}: {state}{}",
      reason.map(|reason| format!(" · {reason}")).unwrap_or_default()
    )],
    code: u8::from(state != "done"),
  })
}

/// One run of `bmo-agency model`.
pub async fn models(data_root: &Path, args: ModelsArgs) -> Result<Answer, VerbError> {
  let words = &VERB_WORDS.models;
  let place = args.db.clone().or_else(|| crate::archive_place(data_root)).unwrap_or_default();
  let db = store_at(&place).await?;

  if let Some(name) = &args.entry {
    let entry = catalogue::entry_shown(&db, name)
      .await
      .map_err(fail)?
      .ok_or_else(|| fail(fill_marks(&words.not_held, &[("name", name)])))?;

    return Ok(Answer::from(vec![if args.json { entry.to_string() } else { shown(&entry["file"]) }]));
  }

  if let Some(placed) = &args.placed {
    let (name, node) = (&placed[0], &placed[1]);

    catalogue::place(&db, name, node).await.map_err(fail)?;

    return Ok(Answer::from(vec![fill_marks(&words.placed, &[("name", name), ("node", node)])]));
  }

  let Some(step) = args.step.clone() else {
    return Ok(Answer::from(listing(&catalogue::shown(&db, data_root).await.map_err(fail)?)));
  };
  let Some(name) = args.name.clone().filter(|_| args.rest.is_empty() || step == "run-use") else {
    return Err(fail(words.usage.clone()));
  };

  match step.as_str() {
    "add" | "set" => {
      let held = catalogue::entry_shown(&db, &name).await.map_err(fail)?;

      if step == "add" && held.is_some() {
        return Err(fail(fill_marks(&words.already_held, &[("name", &name)])));
      }

      if step == "set" && held.is_none() {
        return Err(fail(fill_marks(&words.not_held, &[("name", &name)])));
      }

      catalogue::change(&db, data_root, "save", &name, &args.body("save"))
        .await
        .map_err(fail)?;

      Ok(Answer::from(listing(&catalogue::shown(&db, data_root).await.map_err(fail)?)))
    }
    "remove" => {
      catalogue::change(&db, data_root, "remove", &name, &args.body("remove"))
        .await
        .map_err(fail)?;

      Ok(Answer::from(vec![fill_marks(&words.removed, &[("name", &name)])]))
    }
    "use" => {
      let chosen_by = std::env::var("USER").unwrap_or_default();
      let (engine, log) = catalogue::choose(&db, data_root, &place, &code_root(), &name, &chosen_by)
        .await
        .map_err(fail)?;

      Ok(Answer::from(vec![fill_marks(
        &words.choosing,
        &[("engine", &engine), ("entry", &name), ("log", &log.to_string_lossy())],
      )]))
    }
    "run-use" => {
      let id = name
        .parse::<i64>()
        .map_err(|_| fail(fill_marks(&words.run.not_running, &[("id", &name)])))?;

      run_use(&db, data_root, &place, id).await
    }
    _ => Err(fail(words.usage.clone())),
  }
}
