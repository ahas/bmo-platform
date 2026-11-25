#![allow(
  dead_code,
  unused_imports,
  unused_variables,
  clippy::needless_lifetimes,
  clippy::let_and_return,
  clippy::unnecessary_to_owned
)]
//! Two records a caller meets as two resources. The model resource is the registry's registrations
//! (docs/own-llm.md, 「Registered models」): a model is listed, made, edited and removed there, and every
//! one of those verbs reads or writes the one registration table. The model catalogue is the files the
//! deployment's own engines serve (docs/model-catalogue.md), served at `/model-catalogue`: every entry
//! with whether its engine serves it now, the latest choice for each engine with the end of its log, and a
//! change held to the policy's rules. The engine's health, the choice's unit and its log are the
//! harness-machine crate's (docs/harness-machine.md, 「A model's use on its engine」).

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use harness_machine::engine_health::EngineHealth;
use harness_machine::model_use::{self, Start};
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use policy::model_catalogue::{MODEL_CATALOGUE, NumberBounds};
use policy::registry::REGISTRY;
use regex::Regex;
use sea_orm::{ConnectionTrait, QueryResult, TransactionTrait};
use serde_json::{Map, Value, json};

use super::fault::{Fault, refused};
use super::registry::registration_health;
use super::sessions::asked_by;
use super::{Call, Refused, Route, answer};
use crate::access::Acting;
use crate::now_ms;
use crate::text::length;
use store::{all, one, run};

static NAME: LazyLock<Regex> = LazyLock::new(|| pattern(&MODEL_CATALOGUE.name_pattern));
static FILE: LazyLock<Regex> = LazyLock::new(|| pattern(&MODEL_CATALOGUE.file_pattern));
static SHA256: LazyLock<Regex> = LazyLock::new(|| pattern(&MODEL_CATALOGUE.sha256_pattern));
static PATH: LazyLock<Regex> = LazyLock::new(|| pattern(&MODEL_CATALOGUE.path_pattern));
static WORD: LazyLock<Regex> = LazyLock::new(|| pattern(&MODEL_CATALOGUE.word_pattern));

fn pattern(text: &str) -> Regex {
  Regex::new(text).expect("the policy's catalogue patterns read")
}

/// One entry as the store keeps it, in the shape a screen reads it.
#[derive(Clone, Debug)]
pub struct Entry {
  pub name: String,
  pub file: String,
  pub engine: Option<String>,
  pub registration: Option<String>,
  pub nodes: Vec<String>,
  pub source: Option<String>,
  // Every field as the screen reads it.
  pub shown: Map<String, Value>,
}

const ENTRY_COLUMNS: &str =
  "name, label, file, bytes, sha256, source, engine, registration, nodes, thinking, temperature,
  top_p, presence_penalty, max_tokens, note";

impl Entry {
  fn of(row: &QueryResult) -> Result<Self, sea_orm::DbErr> {
    let nodes: Vec<String> = serde_json::from_str::<Value>(&row.try_get::<String>("", "nodes")?)
      .ok()
      .and_then(|read| read.as_array().cloned())
      .unwrap_or_default()
      .into_iter()
      .filter_map(|node| node.as_str().map(str::to_owned))
      .collect();
    let thinking: Option<i64> = row.try_get("", "thinking")?;
    let entry = json!({
      "name": row.try_get::<String>("", "name")?,
      "label": row.try_get::<String>("", "label")?,
      "file": row.try_get::<String>("", "file")?,
      "bytes": row.try_get::<i64>("", "bytes")?,
      "sha256": row.try_get::<String>("", "sha256")?,
      "source": row.try_get::<Option<String>>("", "source")?,
      "engine": row.try_get::<Option<String>>("", "engine")?,
      "registration": row.try_get::<Option<String>>("", "registration")?,
      "nodes": nodes,
      "thinking": thinking.map(|thinking| thinking == 1),
      "temperature": row.try_get::<Option<f64>>("", "temperature")?,
      "topP": row.try_get::<Option<f64>>("", "top_p")?,
      "presencePenalty": row.try_get::<Option<f64>>("", "presence_penalty")?,
      "maxTokens": row.try_get::<Option<i64>>("", "max_tokens")?,
      "note": row.try_get::<Option<String>>("", "note")?,
    });
    let Value::Object(shown) = entry else {
      unreachable!("an entry is written as an object")
    };

    Ok(Self {
      name: row.try_get("", "name")?,
      file: row.try_get("", "file")?,
      engine: row.try_get("", "engine")?,
      registration: row.try_get("", "registration")?,
      source: row.try_get("", "source")?,
      nodes,
      shown,
    })
  }
}

/// Every entry in the order they were added.
pub(super) async fn entries<C: ConnectionTrait>(db: &C) -> Result<Vec<Entry>, Refused> {
  all(
    db,
    &format!("SELECT {ENTRY_COLUMNS} FROM model_catalogue ORDER BY added_at, name"),
    vec![],
  )
  .await?
  .iter()
  .map(|row| Entry::of(row).map_err(Refused::from))
  .collect()
}

async fn entry<C: ConnectionTrait>(db: &C, name: &str) -> Result<Option<Entry>, Refused> {
  Ok(
    one(
      db,
      &format!("SELECT {ENTRY_COLUMNS} FROM model_catalogue WHERE name = $1"),
      vec![name.into()],
    )
    .await?
    .as_ref()
    .map(Entry::of)
    .transpose()?,
  )
}

/// One time an entry was chosen for its engine.
pub(super) struct Use {
  pub entry: String,
  pub shown: Value,
}

const USE_COLUMNS: &str = "id, entry, engine, chosen_by, started_at, ended_at, state, step, reason, log";

impl Use {
  fn of(row: &QueryResult) -> Result<Self, sea_orm::DbErr> {
    let state: String = row.try_get("", "state")?;
    let log: Option<String> = row.try_get("", "log")?;

    Ok(Self {
      entry: row.try_get("", "entry")?,
      shown: json!({
        "id": row.try_get::<i64>("", "id")?,
        "entry": row.try_get::<String>("", "entry")?,
        "engine": row.try_get::<String>("", "engine")?,
        "chosenBy": row.try_get::<String>("", "chosen_by")?,
        "startedAt": row.try_get::<i64>("", "started_at")?,
        "endedAt": row.try_get::<Option<i64>>("", "ended_at")?,
        "state": if state == "done" || state == "failed" { state } else { "running".to_owned() },
        "step": row.try_get::<Option<String>>("", "step")?,
        "reason": row.try_get::<Option<String>>("", "reason")?,
        "logTail": model_use::log_tail(log.as_deref().map(Path::new)),
        "log": log,
      }),
    })
  }
}

/// The latest choice for each engine, newest first, each with the end of its log.
pub(super) async fn latest_uses<C: ConnectionTrait>(db: &C) -> Result<Vec<Use>, Refused> {
  all(
    db,
    &format!(
      "SELECT {USE_COLUMNS} FROM model_uses WHERE id IN (SELECT MAX(id) FROM model_uses GROUP BY engine) ORDER BY id DESC"
    ),
    vec![],
  )
  .await?
  .iter()
  .map(|row| Use::of(row).map_err(Refused::from))
  .collect()
}

/// The choice running for an engine, or none.
async fn running_use<C: ConnectionTrait>(db: &C, engine: &str) -> Result<Option<Use>, Refused> {
  Ok(
    one(
      db,
      &format!("SELECT {USE_COLUMNS} FROM model_uses WHERE engine = $1 AND state = 'running' ORDER BY id DESC LIMIT 1"),
      vec![engine.into()],
    )
    .await?
    .as_ref()
    .map(Use::of)
    .transpose()?,
  )
}

/// The file each registration's engine serves now, asked once per registration.
async fn serving_files<C: ConnectionTrait>(
  db: &C,
  data_root: &Path,
  listed: &[Entry],
) -> Result<HashMap<String, Option<String>>, Refused> {
  let registrations: BTreeSet<String> = listed.iter().filter_map(|one| one.registration.clone()).collect();
  let mut serving = HashMap::new();

  for name in registrations {
    let health = registration_health(db, data_root, &name).await?;

    serving.insert(name, health.and_then(|health| health.model));
  }

  Ok(serving)
}

/// The catalogue as its screen shows it.
async fn view<C: ConnectionTrait>(db: &C, data_root: &Path) -> Result<Value, Refused> {
  let listed = entries(db).await?;
  let serving = serving_files(db, data_root, &listed).await?;
  let shown: Vec<Value> = listed
    .into_iter()
    .map(|one| {
      let served = one
        .registration
        .as_ref()
        .and_then(|name| serving.get(name).cloned().flatten())
        .is_some_and(|file| file == one.file);
      let mut shown = one.shown;

      shown.insert("serving".into(), served.into());

      Value::Object(shown)
    })
    .collect();
  let uses: Vec<Value> = latest_uses(db).await?.into_iter().map(|one| one.shown).collect();

  Ok(json!({ "entries": shown, "uses": uses }))
}

// The fields a save may name, by the name the body gives each.
const FIELDS: [&str; 13] = [
  "label",
  "file",
  "bytes",
  "sha256",
  "source",
  "engine",
  "registration",
  "note",
  "thinking",
  "temperature",
  "topP",
  "presencePenalty",
  "maxTokens",
];

// The fields a body names, each as given; an empty text is an empty field, a label excepted.
fn named_fields(body: &Value) -> Map<String, Value> {
  FIELDS
    .iter()
    .filter_map(|&field| {
      let value = body.get(field)?;
      let read = match value.as_str() {
        Some(text) if text.trim().is_empty() && field != "label" => Value::Null,
        Some(text) => text.trim().into(),
        None => value.clone(),
      };

      Some((field.to_owned(), read))
    })
    .collect()
}

pub fn checked_source(source: &Value) -> Result<Value, Fault> {
  let text = match source {
    Value::Null => return Ok(Value::Null),
    Value::String(text) if !text.trim().is_empty() => text,
    _ => return refused("source", CONTROL_WORDS.models.source_unread.as_str()),
  };

  if text.starts_with("https://") {
    return match url::Url::parse(text) {
      Ok(parsed) => Ok(parsed.to_string().into()),
      Err(_) => refused("source", CONTROL_WORDS.models.source_address.as_str()),
    };
  }

  if !PATH.is_match(text) || text.split('/').any(|part| part == "." || part == "..") {
    return refused("source", CONTROL_WORDS.models.source_shape.as_str());
  }

  Ok(source.clone())
}

fn checked_word(field: &str, value: &Value, what: &str) -> Result<Value, Fault> {
  match value {
    Value::Null => Ok(Value::Null),
    Value::String(word) if WORD.is_match(word) => Ok(value.clone()),
    _ => refused(field, fill_marks(&CONTROL_WORDS.models.word_unread, &[("what", what)])),
  }
}

// One default: empty, or a number in its range.
pub fn checked_default(
  field: &str,
  value: &Value,
  bounds: &NumberBounds,
  label: &str,
) -> Result<Value, Fault> {
  if value.is_null() {
    return Ok(Value::Null);
  }

  let max = bounds.max.unwrap_or(f64::INFINITY);
  let held = value.as_f64().filter(|number| {
    number.is_finite()
      && (!bounds.whole || number.fract() == 0.0)
      && (bounds.min..=max).contains(number)
      && !(bounds.above_min && *number == bounds.min)
  });

  match held {
    Some(_) => Ok(value.clone()),
    None if bounds.above_min => refused(
      field,
      fill_marks(
        &CONTROL_WORDS.models.above,
        &[("label", label), ("min", &bounds.min.to_string()), ("max", &max.to_string())],
      ),
    ),
    None => {
      let words = if bounds.whole { &CONTROL_WORDS.models.whole } else { &CONTROL_WORDS.models.number };

      refused(
        field,
        fill_marks(words, &[("label", label), ("min", &bounds.min.to_string()), ("max", &max.to_string())]),
      )
    }
  }
}

/// Every field of an entry checked together, answered as the row is written.
fn checked_fields(fields: &Map<String, Value>) -> Result<Map<String, Value>, Fault> {
  let get = |field: &str| fields.get(field).unwrap_or(&Value::Null);
  let label = match get("label").as_str() {
    Some(label) if !label.trim().is_empty() && length(label) <= MODEL_CATALOGUE.label_most => {
      label.trim().to_owned()
    }
    _ => {
      return refused(
        "label",
        fill_marks(&CONTROL_WORDS.models.label, &[("most", &MODEL_CATALOGUE.label_most.to_string())]),
      );
    }
  };

  if !get("file").as_str().is_some_and(|file| FILE.is_match(file)) {
    return refused("file", CONTROL_WORDS.models.file.as_str());
  }

  let bytes = get("bytes").as_number().and_then(|number| {
    number
      .as_i64()
      .or_else(|| number.as_f64().filter(|float| float.fract() == 0.0).map(|float| float as i64))
  });

  if !bytes.is_some_and(|bytes| bytes > 0 && bytes < (1 << 53)) {
    return refused("bytes", CONTROL_WORDS.models.bytes.as_str());
  }

  if !get("sha256").as_str().is_some_and(|sha| SHA256.is_match(sha)) {
    return refused("sha256", CONTROL_WORDS.models.sha256.as_str());
  }

  let note = match get("note") {
    Value::Null => Value::Null,
    Value::String(note) if length(note) <= MODEL_CATALOGUE.note_most => {
      if note.trim().is_empty() {
        Value::Null
      } else {
        note.trim().into()
      }
    }
    _ => {
      return refused(
        "note",
        fill_marks(
          &CONTROL_WORDS.models.note,
          &[("most", &crate::text::grouped(MODEL_CATALOGUE.note_most))],
        ),
      );
    }
  };
  let source = checked_source(get("source"))?;
  let engine = checked_word("engine", get("engine"), &CONTROL_WORDS.models.engine_word)?;
  let registration =
    checked_word("registration", get("registration"), &CONTROL_WORDS.models.registration_word)?;
  let thinking = match get("thinking") {
    Value::Null | Value::Bool(_) => get("thinking").clone(),
    _ => return refused("thinking", CONTROL_WORDS.models.thinking.as_str()),
  };
  let defaults = &MODEL_CATALOGUE.defaults;
  let temperature =
    checked_default("temperature", get("temperature"), &defaults.temperature, "temperature")?;
  let top_p = checked_default("topP", get("topP"), &defaults.top_p, "top_p")?;
  let presence_penalty = checked_default(
    "presencePenalty",
    get("presencePenalty"),
    &defaults.presence_penalty,
    "presence_penalty",
  )?;
  let max_tokens = checked_default("maxTokens", get("maxTokens"), &defaults.max_tokens, "max_tokens")?;
  let checked = json!({
    "label": label,
    "file": get("file"),
    "bytes": get("bytes"),
    "sha256": get("sha256"),
    "source": source,
    "engine": engine,
    "registration": registration,
    "note": note,
    "thinking": thinking,
    "temperature": temperature,
    "topP": top_p,
    "presencePenalty": presence_penalty,
    "maxTokens": max_tokens,
  });
  let Value::Object(checked) = checked else {
    unreachable!("the fields are written as an object")
  };

  Ok(checked)
}

// The checked fields as the row's columns take them, in the columns' order.
fn columns(fields: &Map<String, Value>) -> Vec<sea_orm::Value> {
  let text = |field: &str| fields[field].as_str().map(str::to_owned);
  let number = |field: &str| fields[field].as_f64();

  vec![
    text("label").into(),
    text("file").into(),
    fields["bytes"].as_f64().map(|bytes| bytes as i64).into(),
    text("sha256").into(),
    text("source").into(),
    text("engine").into(),
    text("registration").into(),
    fields["thinking"].as_bool().map(i64::from).into(),
    number("temperature").into(),
    number("topP").into(),
    number("presencePenalty").into(),
    fields["maxTokens"].as_f64().map(|tokens| tokens as i64).into(),
    text("note").into(),
  ]
}

async fn save<C: ConnectionTrait>(db: &C, body: &Value, name: &str) -> Result<(), Fault> {
  let named = named_fields(body);

  match entry(db, name).await? {
    None => {
      if !NAME.is_match(name) {
        return refused("name", CONTROL_WORDS.models.name.as_str());
      }

      let mut seed: Map<String, Value> = FIELDS
        .iter()
        .filter(|&&field| !matches!(field, "file" | "bytes" | "sha256"))
        .map(|&field| (field.to_owned(), Value::Null))
        .collect();

      seed.insert("label".into(), name.into());
      seed.extend(named);

      let fields = checked_fields(&seed)?;
      let mut values = vec![name.into()];

      values.extend(columns(&fields));
      values.push((now_ms() / 1000).into());
      run(
        db,
        "INSERT INTO model_catalogue (name, label, file, bytes, sha256, source, engine, registration, thinking,
         temperature, top_p, presence_penalty, max_tokens, note, added_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        values,
      )
      .await?;
    }
    Some(held) => {
      let mut fields = held.shown;

      fields.extend(named);

      let mut values = columns(&checked_fields(&fields)?);

      values.push(name.into());
      run(
        db,
        "UPDATE model_catalogue SET label = $1, file = $2, bytes = $3, sha256 = $4, source = $5, engine = $6,
         registration = $7, thinking = $8, temperature = $9, top_p = $10, presence_penalty = $11, max_tokens = $12,
         note = $13 WHERE name = $14",
        values,
      )
      .await?;
    }
  }

  Ok(())
}

async fn required<C: ConnectionTrait>(db: &C, name: &str) -> Result<Entry, Fault> {
  match entry(db, name).await? {
    Some(held) => Ok(held),
    None => refused("name", fill_marks(&CONTROL_WORDS.models.unknown, &[("name", name)])),
  }
}

/// An entry taken out, refused for the entry a choice runs for and the one its engine serves now.
async fn remove<C: ConnectionTrait>(db: &C, data_root: &Path, name: &str) -> Result<(), Fault> {
  let held = required(db, name).await?;

  if let Some(engine) = &held.engine
    && running_use(db, engine).await?.is_some_and(|running| running.entry == name)
  {
    return refused("name", fill_marks(&CONTROL_WORDS.models.switching_to, &[("name", name)]));
  }

  let health = match &held.registration {
    Some(registration) => registration_health(db, data_root, registration).await?,
    None => None,
  };

  if health.and_then(|health| health.model).is_some_and(|model| model == held.file) {
    let engine = held.engine.as_deref().unwrap_or(&CONTROL_WORDS.models.engine_word);

    return refused(
      "name",
      fill_marks(&CONTROL_WORDS.models.serving, &[("engine", engine), ("name", name)]),
    );
  }

  run(db, "DELETE FROM model_catalogue WHERE name = $1", vec![name.into()]).await?;

  Ok(())
}

// Why the engine cannot be stood again now, or none where it can.
pub fn engine_busy(health: Option<&EngineHealth>) -> Option<String> {
  let health = health?;

  if let Some(aside) = &health.aside {
    return Some(fill_marks(&CONTROL_WORDS.models.aside, &[("reason", &aside.reason)]));
  }

  health
    .in_hand
    .filter(|in_hand| *in_hand > 0)
    .map(|in_hand| fill_marks(&CONTROL_WORDS.models.in_hand, &[("count", &in_hand.to_string())]))
}

/// What the front starts a choice's unit on: its own machine, the store's address and the trees it stands
/// in.
struct Machine {
  data_root: PathBuf,
  store_place: String,
  source_root: PathBuf,
  code_root: PathBuf,
  path: String,
}

impl Machine {
  fn of(call: &Call) -> Result<Self, Refused> {
    let data_root = call.data_root();

    Ok(Self {
      store_place: crate::archive_place(&data_root)
        .ok_or_else(|| Refused::failed("the server names no store address"))?,
      source_root: call.ground().source.clone(),
      path: call.machine().var("PATH").unwrap_or_default().to_owned(),
      code_root: call.asset_root(),
      data_root,
    })
  }
}

/// An entry chosen for its engine (docs/model-catalogue.md, 「Choosing a model for an engine」):
/// checked, recorded as running, and handed to a user unit of its own that outlives the screen; the
/// engine and the choice's log.
async fn start_use<C: ConnectionTrait + TransactionTrait>(
  db: &C,
  machine: &Machine,
  name: &str,
  chosen_by: &str,
) -> Result<(String, PathBuf), Fault> {
  let entry = required(db, name).await?;
  let Some(engine) = entry.engine.clone() else {
    return refused("engine", fill_marks(&CONTROL_WORDS.models.no_engine, &[("name", name)]));
  };

  if entry.nodes.is_empty() && !entry.source.as_deref().unwrap_or_default().starts_with("https://") {
    return refused("source", fill_marks(&CONTROL_WORDS.models.nowhere, &[("name", name)]));
  }

  if let Some(registration) = &entry.registration {
    let health = registration_health(db, &machine.data_root, registration).await?;

    if let Some(busy) = engine_busy(health.as_ref()) {
      return refused("engine", busy);
    }
  }

  let at = now_ms();
  let log = machine.data_root.join("model-uses").join(format!("{engine}-{at}.log"));

  if let Some(folder) = log.parent() {
    std::fs::create_dir_all(folder).map_err(Refused::failed)?;
  }

  let written = db.begin().await?;

  if let Some(running) = running_use(&written, &engine).await? {
    return refused(
      "engine",
      fill_marks(
        &CONTROL_WORDS.models.engine_switching,
        &[("engine", &engine), ("entry", &running.entry)],
      ),
    );
  }

  let id: i64 = one(
    &written,
    "INSERT INTO model_uses (entry, engine, chosen_by, started_at, state, log) VALUES ($1, $2, $3, $4, 'running', $5)
     RETURNING id",
    vec![
      name.into(),
      engine.clone().into(),
      chosen_by.into(),
      (at / 1000).into(),
      log.display().to_string().into(),
    ],
  )
  .await?
  .ok_or_else(|| Refused::failed("the choice was not recorded"))?
  .try_get("", "id")?;

  written.commit().await?;

  let start = Start {
    engine,
    use_id: id,
    archive: machine.store_place.clone(),
    path: machine.path.clone(),
    source_root: machine.source_root.clone(),
    code_root: machine.code_root.clone(),
  };

  if let Err(said) = start.run() {
    let said = if said.is_empty() { "systemd-run failed".to_owned() } else { said };
    let reason = fill_marks(&CONTROL_WORDS.models.unit_failed, &[("why", &said)]);

    run(
      db,
      "UPDATE model_uses SET state = 'failed', reason = $1, ended_at = $2 WHERE id = $3 AND state = 'running'",
      vec![reason.clone().into(), (now_ms() / 1000).into(), id.into()],
    )
    .await?;

    return refused("engine", reason);
  }

  Ok((start.engine, log))
}

/// What the catalogue verb asks of the same catalogue the routes keep (docs/model-catalogue.md, 「Where it
/// is set」), each refusal in the words a route answers it with.
pub mod verb {
  use std::path::{Path, PathBuf};

  use sea_orm::DatabaseConnection;
  use serde_json::Value;

  use policy::control_words::CONTROL_WORDS;

  use super::super::fault::Fault;
  use super::{Machine, WORD, entry, refused, remove, required, run, save, start_use, view};
  use crate::config::SOURCE_DIR;

  fn said(fault: Fault) -> String {
    match fault {
      Fault::Field(_, error) => error,
      Fault::Failed(failed) => failed.error,
    }
  }

  /// The catalogue as its screen shows it: its entries, each with whether its engine serves it, and the
  /// latest choice for each engine.
  pub async fn shown(db: &DatabaseConnection, data_root: &Path) -> Result<Value, String> {
    view(db, data_root).await.map_err(|refused| refused.error)
  }

  /// One entry whole, as an installer reads it; none where the catalogue holds none of that name.
  pub async fn entry_shown(db: &DatabaseConnection, name: &str) -> Result<Option<Value>, String> {
    Ok(
      entry(db, name)
        .await
        .map_err(|refused| refused.error)?
        .map(|held| Value::Object(held.shown)),
    )
  }

  /// An entry saved — made where none of its name stands, changed where one does — or taken out.
  pub async fn change(
    db: &DatabaseConnection,
    data_root: &Path,
    action: &str,
    name: &str,
    body: &Value,
  ) -> Result<(), String> {
    match action {
      "remove" => remove(db, data_root, name).await.map_err(said),
      _ => save(db, body, name).await.map_err(said),
    }
  }

  /// A node an entry's file lies on written down, once.
  pub async fn place(db: &DatabaseConnection, name: &str, node: &str) -> Result<(), String> {
    let held = required(db, name).await.map_err(said)?;

    if !WORD.is_match(node) {
      return refused::<()>("nodes", CONTROL_WORDS.models.node_unread.as_str()).map_err(said);
    }

    if held.nodes.iter().any(|placed| placed == node) {
      return Ok(());
    }

    let mut nodes = held.nodes;

    nodes.push(node.to_owned());
    run(
      db,
      "UPDATE model_catalogue SET nodes = $1 WHERE name = $2",
      vec![serde_json::to_string(&nodes).unwrap_or_default().into(), name.into()],
    )
    .await
    .map(drop)
    .map_err(|refusal| refusal.detail)
  }

  /// An entry chosen for its engine, handed to a user unit of its own that runs the verbs' binary out of
  /// `code_root`.
  pub async fn choose(
    db: &DatabaseConnection,
    data_root: &Path,
    store_place: &str,
    code_root: &Path,
    name: &str,
    chosen_by: &str,
  ) -> Result<(String, PathBuf), String> {
    let machine = Machine {
      data_root: data_root.to_path_buf(),
      store_place: store_place.to_owned(),
      source_root: std::env::var_os(SOURCE_DIR)
        .filter(|named| !named.is_empty())
        .map_or_else(|| code_root.to_path_buf(), PathBuf::from),
      code_root: code_root.to_path_buf(),
      path: std::env::var("PATH").unwrap_or_default(),
    };

    start_use(db, &machine, name, chosen_by).await.map_err(said)
  }
}

/// The catalogue as a caller shows it.
pub struct Catalogue;

impl Route for Catalogue {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/model-catalogue";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    Ok(answer(StatusCode::OK, view(call.store(), &call.data_root()).await?))
  }
}

crate::moved!(Catalogue);

/// One change: an entry saved or removed, or chosen for its engine, answered with the catalogue as it
/// then stands.
pub struct CatalogueChange;

impl Route for CatalogueChange {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/model-catalogue";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;

    if !body.is_object() && !body.is_array() {
      return Err(Refused::new(StatusCode::BAD_REQUEST, CONTROL_WORDS.request.action.as_str()));
    }

    let db = call.store();
    let data_root = call.data_root();
    let name = body.get("name").and_then(Value::as_str).map(str::trim).unwrap_or_default().to_owned();
    let made = match body.get("action").and_then(Value::as_str) {
      Some("use") => {
        let named = body.get("name").and_then(Value::as_str).unwrap_or_default();

        start_use(db, &Machine::of(&call)?, named, &asked_by(&body)).await.map(drop)
      }
      Some("save") => save(db, &body, &name).await,
      Some("remove") => remove(db, &data_root, &name).await,
      _ => refused("action", CONTROL_WORDS.request.action.as_str()),
    };

    if let Err(fault) = made {
      return fault.answer();
    }

    Ok(answer(StatusCode::OK, view(db, &data_root).await?))
  }
}

crate::moved!(CatalogueChange);

// A model changed, so every program standing on a pin that names it is checked against the pin and the
// registration's addresses now: a worker's is stood again between its turns, and a supervisor's stale one has its
// sessions replaced through their handoff while it is quiet (docs/supervisors.md, 「Where a supervisor's turn
// runs」).
pub(super) fn restand(call: &crate::control::Call) {
  if let Some(places) = call.places().cloned() {
    let sessions = call.sessions();

    tokio::spawn(async move {
      crate::places::standby::restand_all(&places.ground, false).await;

      for agent in crate::places::standby::stale_supervisors(&places.ground).await {
        if let Some(sessions) = &sessions {
          sessions.road_moved(&agent).await;
        }
      }
    });
  }
}

fn faulted(fault: Fault) -> Refused {
  match fault {
    Fault::Field(field, error) => Refused::new(StatusCode::BAD_REQUEST, format!("{field}: {error}")),
    Fault::Failed(refused) => refused,
  }
}

/// Every model the boundary holds: the registry's registrations, each with the orders and pins that name it.
/// A provider the query names is the registry's alone, since every other provider's models are its own
/// catalogue. A member sees the registrations filed under their own account and those filed under the units
/// they head (docs/api.md, 「Who a caller acts for」), narrowed further by the list's `ownerAccount`, `unit`
/// and `unitSubtree` parameters.
pub struct ListModels;

impl Route for ListModels {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/models";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let asked = call
      .query("provider")
      .filter(|provider| !provider.trim().is_empty() && provider.as_str() != super::registry::PROVIDER);

    if let Some(provider) = asked {
      return Err(Refused::new(
        StatusCode::NOT_FOUND,
        format!("{provider}'s models are its own catalogue, and none of them is a registration"),
      ));
    }

    let mut reach = crate::organisation::resources::Reach::listed(
      call.ground(),
      Acting::of(&call)?,
      call.query("ownerAccount").as_deref(),
      call.query("unit").as_deref(),
      call.query("unitSubtree").as_deref(),
    )
    .await?;
    let mut items = vec![];

    for model in super::registry::listed_models(call.store(), &call.data_root()).await? {
      if !reach.reaches_row(model["account"].as_i64(), model["unit"].as_i64()).await? {
        continue;
      }

      items.push(model);
    }

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": false })))
  }
}

crate::moved!(ListModels);

/// A model registered: its name from the key, the model behind it, and the registry's smallest window,
/// answer and wait until the owner sizes it.
pub struct CreateModel;

impl Route for CreateModel {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/models";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let key = body
      .get("key")
      .and_then(Value::as_str)
      .map(str::trim)
      .filter(|key| !key.is_empty())
      .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "key (a string) is required"))?;
    let (provider, name) = super::walk_reads::model_key(key, None);

    let disagreed = body.get("provider").and_then(Value::as_str).filter(|declared| *declared != provider);

    if let Some(declared) = disagreed {
      return Err(Refused::new(
        StatusCode::BAD_REQUEST,
        format!("the key names {provider}, not {declared}"),
      ));
    }

    let model = body
      .get("model")
      .and_then(Value::as_str)
      .map(str::trim)
      .filter(|model| !model.is_empty())
      .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "model (a string) is required"))?
      .to_owned();

    if provider != super::registry::PROVIDER {
      return Err(Refused::new(
        StatusCode::NOT_FOUND,
        format!("{provider}'s models are its own catalogue, and none of them is a registration"),
      ));
    }

    let db = call.store();
    let data_root = call.data_root();

    if super::registry::registered_model(db, &data_root, &name).await?.is_some() {
      return Err(Refused::new(StatusCode::CONFLICT, format!("a model is already registered as {name}")));
    }

    // A registration filed under a unit is filed there by the platform's call or by a head of that unit;
    // heading the unit is the whole of who may file there (docs/api.md, 「Who a caller acts for」).
    if let Some(unit) =
      super::registry::checked_unit(body.get("unit").unwrap_or(&Value::Null)).map_err(faulted)?
      && !crate::organisation::resources::heads(call.ground(), Acting::of(&call)?, unit).await?
    {
      return Err(
        Refused::new(
          StatusCode::FORBIDDEN,
          format!(
            "The signed-in person may not file the model {name} under the unit {unit}: they do not head it"
          ),
        )
        .with_code("forbidden"),
      );
    }

    let mut change = json!({
      "action": "save",
      "name": name,
      "label": name,
      "model": model,
      "contextTokens": REGISTRY.context_tokens.min,
      "answerTokens": REGISTRY.answer_tokens.min,
    });

    for field in ["cardService", "roleFiles", "languages", "unit"] {
      if let Some(value) = body.get(field) {
        change[field] = value.clone();
      }
    }

    match super::registry::change(db, &data_root, &change).await? {
      Ok(()) => {}
      Err(sentence) => return Err(Refused::new(StatusCode::BAD_REQUEST, sentence)),
    }

    restand(&call);

    match super::registry::registered_model(db, &data_root, &name).await? {
      Some(view) => Ok(answer(StatusCode::CREATED, view)),
      None => Err(Refused::failed("the model was made but not read")),
    }
  }
}

crate::moved!(CreateModel);

/// A model's registration changed: the model name it is registered under, the card service its engine is,
/// whether that service's instances keep role files, the languages it writes results in, the account it
/// belongs to and the unit it stands under, each where the body names it. The account a registration belongs
/// to is the owner's to set: a member's call naming it is refused (docs/supply.md, 「Owned supply」). A unit
/// it stands under is the platform's or a head of that unit's to set, and a registration a unit owns is that
/// unit's heads' to change (docs/api.md, 「Who a caller acts for」).
pub struct ChangeModel;

// The fields of a registration a change hands to the registry beside its model name.
const CHANGED_FIELDS: [&str; 5] = ["cardService", "roleFiles", "languages", "account", "unit"];

impl Route for ChangeModel {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/models/{modelKey}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let key = call.param("modelKey").unwrap_or_default().to_owned();
    let (provider, name) = super::walk_reads::model_key(&key, call.query("provider").as_deref());

    if provider != super::registry::PROVIDER {
      return Err(Refused::new(
        StatusCode::NOT_FOUND,
        format!("{provider}'s models are its own catalogue, and none of them is a registration"),
      ));
    }

    let body = call.body().await?;
    let model = match body.get("model") {
      None => None,
      Some(model) => Some(
        model
          .as_str()
          .map(str::trim)
          .filter(|model| !model.is_empty())
          .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "model is a string"))?
          .to_owned(),
      ),
    };
    let fields = CHANGED_FIELDS.into_iter().any(|field| body.get(field).is_some());

    if model.is_none() && !fields {
      return Err(Refused::new(
        StatusCode::BAD_REQUEST,
        "model, cardService, roleFiles, languages, account or unit is required",
      ));
    }

    if let Some(account) = body.get("account") {
      if Acting::of(&call)? != Acting::Platform {
        return Err(
          Refused::new(StatusCode::FORBIDDEN, CONTROL_WORDS.registry.owner_only.as_str())
            .with_code("forbidden"),
        );
      }

      super::registry::checked_account(account)
        .map_err(|_| Refused::failed(&CONTROL_WORDS.registry.account).with_field("account"))?;
    }

    let db = call.store();
    let data_root = call.data_root();

    let held = super::registry::registration(db, &name)
      .await?
      .ok_or_else(|| Refused::new(StatusCode::NOT_FOUND, format!("no model is registered as {name}")))?;

    // A registration a unit owns is its heads' to change, heading the unit being the whole of who may; the
    // console section asks none of such a call (docs/api.md, 「Who a caller acts for」).
    if let Some(unit) = held.owner_unit
      && !crate::organisation::resources::heads(call.ground(), Acting::of(&call)?, unit).await?
    {
      return Err(crate::organisation::resources::refused(&format!("the model {name}")));
    }

    // Refiling a registration under a unit is the platform's call's or a head of that unit's, and a row
    // carries one owner at most.
    if let Some(unit) = super::registry::checked_unit(body.get("unit").unwrap_or(&Value::Null))
      .map_err(|_| Refused::failed(&CONTROL_WORDS.registry.unit).with_field("unit"))?
      && !crate::organisation::resources::heads(call.ground(), Acting::of(&call)?, unit).await?
    {
      return Err(
        Refused::new(
          StatusCode::FORBIDDEN,
          format!(
            "The signed-in person may not file the model {name} under the unit {unit}: they do not head it"
          ),
        )
        .with_code("forbidden"),
      );
    }

    if let Some(model) = &model {
      super::registry::change_model(db, &name, model).await.map_err(faulted)?;
    }

    if fields {
      let mut change = json!({ "action": "save", "name": name });

      for field in CHANGED_FIELDS {
        if let Some(value) = body.get(field) {
          change[field] = value.clone();
        }
      }

      if let Err(sentence) = super::registry::change(db, &data_root, &change).await? {
        return Err(Refused::new(StatusCode::BAD_REQUEST, sentence));
      }
    }

    restand(&call);

    match super::registry::registered_model(db, &data_root, &name).await? {
      Some(view) => Ok(answer(StatusCode::OK, view)),
      None => Err(Refused::failed("the model was changed but not read")),
    }
  }
}

crate::moved!(ChangeModel);

/// A model's registration taken away.
pub struct DeleteModel;

impl Route for DeleteModel {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/models/{modelKey}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let key = call.param("modelKey").unwrap_or_default();
    let (provider, name) = super::walk_reads::model_key(key, call.query("provider").as_deref());

    if provider != super::registry::PROVIDER {
      return Err(Refused::new(
        StatusCode::NOT_FOUND,
        format!("{provider}'s models are its own catalogue, and none of them is a registration"),
      ));
    }

    let db = call.store();
    let data_root = call.data_root();

    let held = super::registry::registration(db, &name)
      .await?
      .ok_or_else(|| Refused::new(StatusCode::NOT_FOUND, format!("no model is registered as {name}")))?;

    // A registration a unit owns is its heads' to take away (docs/api.md, 「Who a caller acts for」).
    if let Some(unit) = held.owner_unit
      && !crate::organisation::resources::heads(call.ground(), Acting::of(&call)?, unit).await?
    {
      return Err(crate::organisation::resources::refused(&format!("the model {name}")));
    }

    match super::registry::change(db, &data_root, &json!({ "action": "remove", "name": name })).await? {
      Ok(()) => {
        restand(&call);

        Ok(StatusCode::NO_CONTENT.into_response())
      }
      Err(sentence) => Err(Refused::new(StatusCode::BAD_REQUEST, sentence)),
    }
  }
}

crate::moved!(DeleteModel);

/// One address of a model's registration set, as the command's `--register-address` sets it, its key named by a file
/// in the platform's space of the console's secret store rather than given (docs/own-llm.md, 「Where it is set」 and
/// 「The keys」). Where a registration is served is the owner's to set: a member's call is refused.
pub struct SetModelAddress;

impl Route for SetModelAddress {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/models/{modelKey}/addresses/{node}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let key = call.param("modelKey").unwrap_or_default().to_owned();
    let node = call.param("node").unwrap_or_default().to_owned();
    let (provider, name) = super::walk_reads::model_key(&key, call.query("provider").as_deref());

    if Acting::of(&call)? != Acting::Platform {
      return Err(
        Refused::new(StatusCode::FORBIDDEN, CONTROL_WORDS.registry.owner_only.as_str())
          .with_code("forbidden"),
      );
    }

    if provider != super::registry::PROVIDER {
      return Err(Refused::new(
        StatusCode::NOT_FOUND,
        format!("{provider}'s models are its own catalogue, and none of them is a registration"),
      ));
    }

    let body = call.body().await?;
    let db = call.store();
    let data_root = call.data_root();

    if super::registry::registration(db, &name).await?.is_none() {
      return Err(Refused::new(StatusCode::NOT_FOUND, format!("no model is registered as {name}")));
    }

    let mut change = json!({
      "action": "set-address",
      "name": name,
      "node": node,
      "url": body.get("url").cloned().unwrap_or(Value::Null),
    });

    // No key file named is no key: the address answers with none, and a key agency wrote for it goes.
    match body.get("keyFile") {
      None | Some(Value::Null) => change["key"] = Value::Null,
      Some(file) => change["keyFile"] = file.clone(),
    }

    match super::registry::made(db, &data_root, &change).await {
      Ok(()) => {}
      Err(Fault::Field(field, error)) => return Err(Refused::failed(error).with_field(field)),
      Err(Fault::Failed(failed)) => return Err(failed),
    }

    restand(&call);

    match super::registry::registered_model(db, &data_root, &name).await? {
      Some(view) => Ok(answer(StatusCode::OK, view)),
      None => Err(Refused::failed("the address was set but the model was not read")),
    }
  }
}

crate::moved!(SetModelAddress);
