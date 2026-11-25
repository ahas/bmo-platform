//! The supervisors' settings as a caller reads and changes them: what each one's system prompt is made of,
//! its persona, whether its turns are given their reference, how its answers are generated and the
//! registration it is pinned to, a save checked whole before anything is written and overwriting what it
//! changes. The prompt its next turn sends is composed by the harness-machine crate
//! (docs/harness-machine.md, 「A supervisor's system prompt」), its persona read from the prompt folder its
//! row names.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use harness_machine::supervisor_prompt::{self, Base, Composition, Roots, Supervisor};
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use policy::roles::ROLES;
use policy::supervisors::{GENERATION, GenerationSetting, SUPERVISORS};
use sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, QueryResult, TransactionTrait};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tool_call::account_service::AccountService;

use super::fault::{Fault, refused};
use super::models::{entries, latest_uses};
use super::registry::registration_health;
use super::registry::{PROVIDER, registration, registrations};
use super::sessions::{TELL, agent_row, ask_agent_resets, asked_by, carry_out_waiting, last_done, waiting};
use super::{Call, Refused, Route, answer};
use crate::conversational::{Settings as Conversation, Shape, prompt_folder};
use crate::hooks::Hooks;
use crate::now_ms;
use crate::places::Ground;
use crate::places::follow::RoomTitle;
use crate::text::{grouped, length};
use store::{all, one, run};

/// The settings fields a save may change, by the names a save marks as saved.
const FIELDS: [&str; 4] = ["referenceOn", "brainOn", "toolsAllowed", "persona"];

/// The settings fields whose change alters what a session is handed at its start, so a save that changes one
/// starts every room afresh.
const STARTING_FIELDS: [&str; 3] = ["referenceOn", "brainOn", "toolsAllowed"];

/// Every field a save's body may name: the supervisor, who saves, its settings, its pin and its thinking.
const BODY_FIELDS: [&str; 9] = [
  "agent",
  "by",
  "referenceOn",
  "brainOn",
  "toolsAllowed",
  "persona",
  "registration",
  "generation",
  "thinking",
];

/// One supervisor's settings as its row holds them, every default where it holds none.
#[derive(Clone, Debug)]
pub struct Settings {
  agent: String,
  /// Whether the gate's reading of the supervisor's brain reaches its turns (docs/supervisors.md, 「Its
  /// settings」).
  pub reference_on: bool,
  /// Whether the supervisor's brain reaches its turns (docs/supervisors.md, 「Its settings」).
  pub brain_on: bool,
  /// The capabilities its sessions keep, or none for everything they are given.
  pub tools_allowed: Option<Vec<String>>,
  persona: Option<String>,
  generation: Map<String, Value>,
  // The names of every setting a save or the seed has ever written on the row.
  saved: Vec<String>,
  updated_at: Option<i64>,
  updated_by: Option<String>,
}

// A JSON text a column holds, read as JSON where it is JSON: anything else is none.
fn parsed(text: Option<String>) -> Option<Value> {
  serde_json::from_str(&text?).ok()
}

impl Settings {
  pub async fn of<C: ConnectionTrait>(db: &C, agent: &str) -> Result<Self, Refused> {
    let Some(row) =
      one(db, "SELECT * FROM supervisor_settings WHERE agent = $1", vec![agent.into()]).await?
    else {
      return Ok(Self {
        agent: agent.to_owned(),
        reference_on: SUPERVISORS.reference_on,
        brain_on: true,
        tools_allowed: None,
        persona: None,
        generation: Map::new(),
        saved: vec![],
        updated_at: None,
        updated_by: None,
      });
    };
    let text = |column: &str| row.try_get::<Option<String>>("", column);
    let object = |read: Option<Value>, keep: fn(&Value) -> bool| -> Map<String, Value> {
      match read {
        Some(Value::Object(read)) => read.into_iter().filter(|(_, value)| keep(value)).collect(),
        _ => Map::new(),
      }
    };

    Ok(Self {
      agent: row.try_get("", "agent")?,
      reference_on: row
        .try_get::<Option<i64>>("", "reference_on")?
        .map_or(SUPERVISORS.reference_on, |on| on != 0),
      brain_on: row.try_get::<Option<i64>>("", "brain_on")?.is_none_or(|on| on != 0),
      tools_allowed: parsed(text("tools_allowed")?).and_then(|read| serde_json::from_value(read).ok()),
      persona: text("persona")?,
      generation: object(parsed(text("generation")?), |value| value.is_number() || value.is_boolean()),
      saved: parsed(text("saved_settings")?)
        .and_then(|read| serde_json::from_value(read).ok())
        .unwrap_or_default(),
      updated_at: row.try_get("", "updated_at")?,
      updated_by: text("updated_by")?,
    })
  }

  fn shown(&self) -> Value {
    json!({
      "agent": self.agent,
      "referenceOn": self.reference_on,
      "brainOn": self.brain_on,
      "toolsAllowed": self.tools_allowed,
      "persona": self.persona,
      "generation": self.generation,
      "updatedAt": self.updated_at,
      "updatedBy": self.updated_by,
    })
  }

  // One field as a save reads it.
  fn field(&self, name: &str) -> Value {
    self.shown()[name].clone()
  }

  /// The generation settings the row holds, in the order they were saved.
  pub fn generation(&self) -> &Map<String, Value> {
    &self.generation
  }

  pub fn composition(&self) -> Composition {
    Composition {
      persona: self.persona.clone(),
    }
  }
}

// A setting's value as a save compares it.
fn compared(value: &Value) -> Option<String> {
  (!value.is_null()).then(|| value.to_string())
}

/// A supervisor's row, refused 404 where the store holds none of the name.
async fn supervisor_row<C: ConnectionTrait>(db: &C, agent: &str) -> Result<QueryResult, Fault> {
  match agent_row(db, agent).await? {
    Some(row) => Ok(row),
    None => Err(Fault::Failed(Refused::new(
      StatusCode::NOT_FOUND,
      fill_marks(&CONTROL_WORDS.supervisors.not_supervisor, &[("agent", agent)]),
    ))),
  }
}

// Whether a supervisor's turns are of the pieces shape, the one whose system prompt its settings compose.
fn pieces(row: &QueryResult) -> bool {
  Conversation::of(row)
    .is_ok_and(|settings| settings.is_some_and(|settings| settings.shape == Shape::Pieces))
}

// A supervisor's one road as its row holds it, where it names one.
fn road_of(row: &QueryResult) -> Result<Option<crate::order::Entry>, Fault> {
  Ok(crate::supervisors::Supervisor::of(row)?.road)
}

/// The registration a supervisor's road runs on, where its road stands on the registry's provider.
fn pinned_registration(row: &QueryResult) -> Result<Option<String>, Fault> {
  Ok(road_of(row)?.filter(|road| road.provider == PROVIDER).map(|road| road.model))
}

fn thinking_of(row: &QueryResult) -> Result<Option<bool>, Fault> {
  Ok(road_of(row)?.as_ref().and_then(crate::order::Entry::thinks))
}

// A number as a sentence writes it: whole numbers grouped by thousands.
fn figure(number: f64) -> String {
  if number.fract() == 0.0 && number.abs() < 1e15 {
    let grouped = grouped(number.abs() as usize);

    if number < 0.0 { format!("-{grouped}") } else { grouped }
  } else {
    number.to_string()
  }
}

/// Why a value is not one the setting takes, or none where it takes it. The window is the
/// registration's, which `max_tokens` stays under.
pub fn generation_refusal(
  setting: &GenerationSetting,
  value: &Value,
  window: Option<i64>,
) -> Option<String> {
  let Some(bounds) = &setting.bounds else {
    return (!value.is_boolean())
      .then(|| fill_marks(&CONTROL_WORDS.generation.switch, &[("name", &setting.name)]));
  };
  let max = bounds.max.or_else(|| window.map(|window| (window - 1) as f64));
  let name = &setting.name;
  let least = if bounds.above_min {
    fill_marks(&CONTROL_WORDS.generation.above, &[("min", &bounds.min.to_string())])
  } else {
    fill_marks(&CONTROL_WORDS.generation.least, &[("min", &figure(bounds.min))])
  };
  let span = match max {
    Some(max) => fill_marks(&CONTROL_WORDS.generation.most, &[("least", &least), ("max", &figure(max))]),
    None => least,
  };
  let Some(number) = value.as_f64().filter(|number| number.is_finite()) else {
    return Some(fill_marks(&CONTROL_WORDS.generation.number, &[("name", name)]));
  };

  if bounds.whole && number.fract() != 0.0 {
    return Some(fill_marks(&CONTROL_WORDS.generation.whole, &[("name", name)]));
  }

  if number < bounds.min
    || (bounds.above_min && number == bounds.min)
    || max.is_some_and(|max| number > max)
  {
    return Some(fill_marks(&CONTROL_WORDS.generation.range, &[("name", name), ("span", &span)]));
  }

  None
}

/// Every supervisor's generation settings with their bounds, `max_tokens` held under the window.
pub fn generation_settings(window: Option<i64>) -> Vec<Value> {
  GENERATION
    .settings
    .iter()
    .map(|setting| {
      let mut shown = json!({ "name": setting.name });

      if setting.template {
        shown["template"] = true.into();
      }

      let Some(bounds) = &setting.bounds else {
        shown["switch"] = true.into();
        return shown;
      };

      shown["min"] = bounds.min.into();
      shown["max"] = bounds.max.or_else(|| window.map(|window| (window - 1) as f64)).into();

      if bounds.above_min {
        shown["aboveMin"] = true.into();
      }

      if bounds.whole {
        shown["whole"] = true.into();
      }

      shown
    })
    .collect()
}

/// The roots a supervisor's prompt is composed out of on the front: agency's assets and its data root.
pub fn roots(call: &Call) -> Roots {
  Roots {
    assets: call.asset_root(),
    data: call.data_root(),
  }
}

/// The system prompt a supervisor's next turn sends on the provider and model named, or an empty one where it
/// cannot be composed: its persona read out of the prompt folder its row names, never by its name, and its layers
/// as its account's standing and the delegation chain give them (docs/prompt-layers.md).
async fn system<C: ConnectionTrait>(
  db: &C,
  (roots, service): (&Roots, &AccountService),
  (supervisor, owner): (Supervisor<'_>, Option<i64>),
  settings: &Settings,
  (provider, model): (&str, &str),
) -> Result<String, Refused> {
  let chosen = one(
    db,
    "SELECT p.text FROM agent_base_prompts c JOIN base_prompts p ON p.name = c.prompt WHERE c.agent = $1",
    vec![supervisor.name.into()],
  )
  .await?
  .map(|row| row.try_get::<String>("", "text"))
  .transpose()?;
  let base = chosen.map(|text| Base {
    text,
    model: model.to_owned(),
  });

  let harnessed = crate::lines::pieces::takes_harness(provider);
  let Ok(layers) = crate::prompt_layers::of(db, service, owner).await else {
    return Ok(String::new());
  };

  Ok(
    supervisor_prompt::system(
      roots,
      supervisor,
      &settings.composition(),
      base.as_ref(),
      harnessed,
      &layers,
    )
    .unwrap_or_default(),
  )
}

// The provider and model of a supervisor's road.
async fn route_of(db: &DatabaseConnection, agent: &str) -> Option<(String, String)> {
  let road = crate::supervisors::named(db, agent).await.ok()??.road?;

  Some((road.provider, road.model))
}

/// One supervisor as a screen shows it, each room by the title the room's owner answers.
async fn view(
  db: &DatabaseConnection,
  hooks: &Hooks,
  (roots, service): (&Roots, &AccountService),
  agent: &str,
) -> Result<Value, Fault> {
  let row = supervisor_row(db, agent).await?;
  let prompts = prompt_folder(db, agent).await?;
  let registration_name = pinned_registration(&row)?;
  let pinned_provider: Option<String> = None;
  let method = "route";
  let route = match method {
    "route" => route_of(db, agent).await,
    _ => None,
  };
  let health = match &registration_name {
    Some(name) => registration_health(db, &roots.data, name).await?,
    None => None,
  };
  let listed = entries(db).await?;
  let serving = health
    .and_then(|health| health.model)
    .and_then(|model| listed.iter().find(|one| one.file == model).map(|one| one.name.clone()));
  let behind: Vec<_> = match &registration_name {
    Some(name) => listed.iter().filter(|one| one.registration.as_ref() == Some(name)).collect(),
    None => vec![],
  };
  let engine = behind.iter().find_map(|one| one.engine.clone());
  let model_use = match &engine {
    Some(engine) => latest_uses(db)
      .await?
      .into_iter()
      .map(|one| one.shown)
      .find(|shown| shown["engine"] == *engine),
    None => None,
  };
  let settings = Settings::of(db, agent).await?;
  let window = match &registration_name {
    Some(name) => registration(db, name).await?.map(|held| held.context_tokens),
    None => None,
  };
  let pinned_model: Option<String> = None;
  let (provider_now, model_now) = match &route {
    Some((provider, model)) => (provider.clone(), model.clone()),
    None => (pinned_provider.clone().unwrap_or_default(), pinned_model.clone().unwrap_or_default()),
  };
  let text = if pieces(&row) {
    let held = crate::supervisors::Supervisor::of(&row)?;
    let call_names = held.call_names();
    let supervisor = Supervisor {
      name: agent,
      label: held.label.as_deref(),
      call_names: &call_names,
      prompts: prompts.as_deref(),
    };

    Some(
      system(
        db,
        (roots, service),
        (supervisor, held.owner_account),
        &settings,
        (&provider_now, &model_now),
      )
      .await?,
    )
  } else {
    None
  };
  let mut sessions = vec![];

  for session in
    all(db, "SELECT * FROM room_sessions WHERE agent = $1 ORDER BY chat_id", vec![agent.into()]).await?
  {
    let chat_id: i64 = session.try_get("", "chat_id")?;
    let title = hooks.ask::<RoomTitle>(chat_id).await;

    sessions.push(json!({
      "chatId": chat_id,
      "title": title,
      "generation": session.try_get::<i64>("", "generation")?,
      "systemResetAt": session.try_get::<Option<i64>>("", "system_reset_at")?,
      "resetWaiting": waiting(db, agent, Some(chat_id)).await?.is_some(),
      "lastResetAt": last_done(db, agent, Some(chat_id)).await?.and_then(|done| done.done_at),
    }));
  }

  let base_prompt = one(db, "SELECT prompt FROM agent_base_prompts WHERE agent = $1", vec![agent.into()])
    .await?
    .map(|row| row.try_get::<String>("", "prompt"))
    .transpose()?;
  let base_prompts: Vec<Value> = all(db, "SELECT name, label FROM base_prompts ORDER BY name", vec![])
    .await?
    .iter()
    .map(|row| {
      Ok(
        json!({ "name": row.try_get::<String>("", "name")?, "label": row.try_get::<String>("", "label")? }),
      )
    })
    .collect::<Result<_, sea_orm::DbErr>>()?;
  let offered: Vec<Value> = registrations(db)
    .await?
    .into_iter()
    .filter(|one| one.offered())
    .map(|one| json!({ "name": one.name, "label": one.label }))
    .collect();
  let label: Option<String> = row.try_get("", "label")?;

  Ok(json!({
    "agent": agent,
    "label": label.unwrap_or_else(|| agent.to_owned()),
    "icon": ROLES.icon("chat").map(str::to_owned).or(row.try_get::<Option<String>>("", "icon")?),
    "builtin": row.try_get::<i64>("", "builtin")? == 1,
    "method": method,
    "provider": match (&pinned_provider, &route) {
      (Some(provider), _) => Some(provider.as_str()),
      (None, Some((provider, _))) => Some(provider.as_str()),
      (None, None) => None,
    },
    "model": match (&pinned_model, &route) {
      (Some(model), _) => Some(model.clone()),
      (None, Some((_, model))) => Some(model.clone()),
      (None, None) => None,
    },
    "registration": registration_name,
    "registrations": offered,
    "entries": behind.iter().map(|one| json!({
      "name": one.name,
      "label": one.shown["label"],
      "file": one.file,
      "engine": one.engine,
    })).collect::<Vec<_>>(),
    "serving": serving,
    "modelUse": model_use,
    "settings": settings.shown(),
    "generationSettings": generation_settings(window),
    "thinking": thinking_of(&row)?,
    "personaFile": roots.persona(prompts.as_deref(), None).unwrap_or_default(),
    "basePrompt": base_prompt,
    "basePrompts": base_prompts,
    "turnShape": if pieces(&row) { Shape::Pieces.word() } else { Shape::Window.word() },
    "system": text.map(|text| json!({
      "characters": length(&text),
      "hash": format!("{:x}", Sha256::digest(text.as_bytes())),
      "text": text,
    })),
    "sessions": sessions,
  }))
}

/// The registration a body names, where it has an address; none otherwise.
async fn offered<C: ConnectionTrait>(db: &C, named: &Value) -> Result<Option<String>, Refused> {
  let Some(name) = named.as_str() else {
    return Ok(None);
  };

  Ok(registration(db, name).await?.filter(|held| held.offered()).map(|held| held.name))
}

/// What a save changes of the settings row, each field it names.
#[derive(Default)]
struct Change {
  fields: Map<String, Value>,
}

impl Change {
  fn set(&mut self, field: &str, value: Value) {
    self.fields.insert(field.to_owned(), value);
  }

  fn get(&self, field: &str) -> Option<&Value> {
    self.fields.get(field)
  }
}

/// The generation settings a save names, laid over the held ones and each checked against its bounds.
fn generation_change(
  body: &Value,
  held: &Map<String, Value>,
  window: Option<i64>,
) -> Result<Map<String, Value>, Fault> {
  let Some(named) = body.as_object() else {
    return refused("generation", CONTROL_WORDS.supervisors.generation_pairs.as_str());
  };
  let mut next = held.clone();

  for (name, value) in named {
    let Some(setting) = GENERATION.setting(name) else {
      return refused(name, fill_marks(&CONTROL_WORDS.supervisors.not_generation, &[("name", name)]));
    };

    if value.is_null() {
      next.remove(name);
      continue;
    }

    if let Some(refusal) = generation_refusal(setting, value, window) {
      return refused(name, refusal);
    }

    next.insert(name.clone(), value.clone());
  }

  Ok(next)
}

/// A save checked whole: the change of the settings row, the registration it pins and the thinking it
/// sets. Nothing is written until every part has passed.
struct Checked {
  change: Change,
  /// What the save does to the pin: kept where it names none, cleared by a null, set by a registration.
  pin: Option<Option<String>>,
  thinking: Option<bool>,
}

async fn checked<C: ConnectionTrait>(
  db: &C,
  row: &QueryResult,
  body: &Value,
  agent: &str,
) -> Result<Checked, Fault> {
  let mut change = Change::default();

  if let Some(field) = body
    .as_object()
    .and_then(|named| named.keys().find(|key| !BODY_FIELDS.contains(&key.as_str())))
  {
    return refused(field, fill_marks(&CONTROL_WORDS.supervisors.not_field, &[("field", field)]));
  }

  if let Some(reference) = body.get("referenceOn") {
    if !reference.is_boolean() {
      return refused("referenceOn", CONTROL_WORDS.supervisors.reference.as_str());
    }

    change.set("referenceOn", reference.clone());
  }

  if let Some(brain) = body.get("brainOn") {
    if !brain.is_boolean() {
      return refused("brainOn", CONTROL_WORDS.supervisors.brain.as_str());
    }

    change.set("brainOn", brain.clone());
  }

  if let Some(tools) = body.get("toolsAllowed") {
    let offered = crate::supervisor_limits::offered();
    let named = |tools: &Vec<Value>| {
      tools
        .iter()
        .all(|tool| tool.as_str().is_some_and(|tool| offered.iter().any(|one| one == tool)))
    };

    match tools {
      Value::Null => change.set("toolsAllowed", Value::Null),
      Value::Array(listed) if named(listed) => change.set("toolsAllowed", tools.clone()),
      _ => {
        return refused(
          "toolsAllowed",
          fill_marks(&CONTROL_WORDS.supervisors.tools, &[("offered", &offered.join(", "))]),
        );
      }
    }
  }

  let held = Settings::of(db, agent).await?;

  if let Some(persona) = body.get("persona") {
    match persona {
      Value::Null => change.set("persona", Value::Null),
      Value::String(text) if length(text) <= SUPERVISORS.persona_most => {
        change.set("persona", if text.trim().is_empty() { Value::Null } else { persona.clone() });
      }
      _ => {
        return refused(
          "persona",
          fill_marks(&CONTROL_WORDS.supervisors.persona, &[("most", &grouped(SUPERVISORS.persona_most))]),
        );
      }
    }
  }

  let mut pinned = pinned_registration(row)?;
  let mut pin: Option<Option<String>> = None;

  if let Some(named) = body.get("registration") {
    if named.is_null() {
      pinned = None;
      pin = Some(None);
    } else {
      let Some(name) = offered(db, named).await? else {
        return refused("registration", CONTROL_WORDS.supervisors.registration.as_str());
      };

      pinned = Some(name.clone());
      pin = Some(Some(name));
    }
  }

  if let Some(generation) = body.get("generation") {
    let window = match &pinned {
      Some(name) => registration(db, name).await?.map(|held| held.context_tokens),
      None => None,
    };

    change.set("generation", Value::Object(generation_change(generation, &held.generation, window)?));
  }

  let thinking = match body.get("thinking") {
    None => thinking_of(row)?,
    Some(Value::Null) => None,
    Some(Value::Bool(thinking)) => Some(*thinking),
    Some(_) => return refused("thinking", CONTROL_WORDS.supervisors.thinking.as_str()),
  };

  Ok(Checked {
    change,
    pin,
    thinking,
  })
}

/// What a save asks of agency once it is written: the resets it asked to carry out, and whether it moved the
/// supervisor's road, whose sessions are then replaced through their handoff.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Saved {
  pub resets: bool,
  pub road_moved: bool,
}

/// A checked save written in one transaction: the pin, the thinking, and the settings, each overwriting the
/// value it held with the names of those it changed marked as saved, and a reset of every room where it
/// changed how answers are generated. A pin or a thinking it moves moves the road, as the row's route moves it
/// (docs/supervisors.md, 「Where a supervisor's turn runs」).
async fn write(
  db: &DatabaseTransaction,
  row: &QueryResult,
  agent: &str,
  saved: Checked,
  by: &str,
) -> Result<Saved, Fault> {
  let at = now_ms() / 1000;
  let mut fields_changed = vec![];
  let mut road = road_of(row)?;
  let held_road = road.clone();
  let held_registration = pinned_registration(row)?;

  // A registration named moves the supervisor's road to it on the registry's provider; none leaves the road as it
  // stands, since a supervisor always names a road.
  if let Some(Some(registration)) = &saved.pin
    && held_registration.as_ref() != Some(registration)
  {
    road = Some(crate::order::Entry {
      provider: PROVIDER.to_owned(),
      model: registration.clone(),
      effort: None,
      thinking: None,
      answer_tokens: None,
      advisor: None,
    });
  }

  if saved.thinking != thinking_of(row)?
    && let Some(road) = road.as_mut()
  {
    road.thinking = saved.thinking.map(|on| if on { "on" } else { "off" }.to_owned());
  }

  let road_moved = road != held_road;

  run(
    db,
    r#"UPDATE "supervisors" SET "provider" = $1, "model" = $2, "effort" = $3, "thinking" = $4,
         "answer_tokens" = $5, "advisor" = $7
       WHERE "name" = $6"#,
    vec![
      road.as_ref().map(|road| road.provider.clone()).into(),
      road.as_ref().map(|road| road.model.clone()).into(),
      road.as_ref().and_then(|road| road.effort.clone()).into(),
      road.as_ref().and_then(|road| road.thinking.clone()).into(),
      road.as_ref().and_then(|road| road.answer_tokens).into(),
      row.try_get::<String>("", "name")?.into(),
      road.as_ref().and_then(|road| road.advisor.clone()).into(),
    ],
  )
  .await?;

  let held = Settings::of(db, agent).await?;
  let next = |field: &str| saved.change.get(field).cloned().unwrap_or_else(|| held.field(field));
  let changed: Vec<&str> = FIELDS
    .iter()
    .copied()
    .filter(|field| compared(&held.field(field)) != compared(&next(field)))
    .collect();
  let next_generation = match next("generation") {
    Value::Object(generation) => generation,
    _ => Map::new(),
  };
  let mut names: Vec<&String> = held.generation.keys().collect();

  names.extend(next_generation.keys().filter(|name| !held.generation.contains_key(*name)));

  let generation: Vec<String> = names
    .into_iter()
    .filter(|name| held.generation.get(*name) != next_generation.get(*name))
    .cloned()
    .collect();

  if !changed.is_empty() || !generation.is_empty() {
    let mut marked: Vec<String> = held.saved.clone();

    marked.extend(changed.iter().map(|field| (*field).to_owned()));
    marked.extend(generation.iter().cloned());
    marked.sort();
    marked.dedup();
    run(
      db,
      "INSERT INTO supervisor_settings (agent, reference_on, brain_on, tools_allowed, pieces, own_pieces, persona,
       generation, saved_settings, updated_at, updated_by) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
       ON CONFLICT (agent) DO UPDATE SET reference_on = excluded.reference_on, brain_on = excluded.brain_on,
       tools_allowed = excluded.tools_allowed, pieces = excluded.pieces,
       own_pieces = excluded.own_pieces, persona = excluded.persona, generation = excluded.generation,
       saved_settings = excluded.saved_settings, updated_at = excluded.updated_at, updated_by = excluded.updated_by",
      vec![
        agent.into(),
        i64::from(next("referenceOn") != Value::Bool(false)).into(),
        i64::from(next("brainOn") != Value::Bool(false)).into(),
        match next("toolsAllowed") {
          Value::Null => Option::<String>::None,
          listed => Some(listed.to_string()),
        }
        .into(),
        Option::<String>::None.into(),
        "{}".to_owned().into(),
        next("persona").as_str().map(str::to_owned).into(),
        Value::Object(next_generation.clone()).to_string().into(),
        json!(marked).to_string().into(),
        at.into(),
        by.into(),
      ],
    )
    .await?;

    fields_changed.extend(generation);
    fields_changed.extend(
      changed
        .iter()
        .filter(|field| STARTING_FIELDS.contains(field))
        .map(|field| (*field).to_owned()),
    );
  }

  // A change to how its answers are generated starts every room afresh, so no conversation goes on
  // under settings it was not begun with.
  let resets = !fields_changed.is_empty()
    && ask_agent_resets(db, agent, by, &CONTROL_WORDS.supervisors.reset_reason).await?;

  Ok(Saved { resets, road_moved })
}

/// The roots a supervisor's settings are read under in a process on agency's ground: the assets of the
/// code tree it runs, and its data root.
pub fn process_roots(ground: &Ground) -> Roots {
  Roots {
    assets: ground.code_root(),
    data: ground.roots.data.clone(),
  }
}

// What stops a verb's read or save, as the sentence it prints.
fn said(fault: Fault) -> String {
  match fault {
    Fault::Field(_, error) => error,
    Fault::Failed(failed) => failed.error,
  }
}

/// One supervisor as its screen shows it, read by a verb outside a route; the sentence of what stopped it
/// otherwise.
pub async fn shown(
  db: &DatabaseConnection,
  hooks: &Hooks,
  roots: &Roots,
  agent: &str,
) -> Result<Value, String> {
  let service = AccountService::read(&roots.data, |name| std::env::var(name).ok());

  view(db, hooks, (roots, &service), agent).await.map_err(said)
}

/// A save of one supervisor's settings, read as a route's body is read and written whole or refused, by a
/// verb outside a route: whether agency is to carry out the resets it asked, or the sentence of what
/// stopped it.
pub async fn saved(db: &DatabaseConnection, body: &Value, by: &str) -> Result<bool, String> {
  let agent = body.get("agent").and_then(Value::as_str).unwrap_or_default().to_owned();
  let written = db.begin().await.map_err(|why| why.to_string())?;
  let made = async {
    let row = supervisor_row(&written, &agent).await?;
    let checked = checked(&written, &row, body, &agent).await?;

    write(&written, &row, &agent, checked, by).await
  }
  .await
  .map_err(said)?;

  written.commit().await.map_err(|why| why.to_string())?;

  Ok(made.resets)
}

/// The word a save that asked resets is told to the server by.
pub const RESETS_TOLD: &str = TELL;

/// `GET /supervisors/{supervisorName}/settings`: one supervisor as its settings page shows it.
pub struct SupervisorSettingsRead;

impl Route for SupervisorSettingsRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/supervisors/{supervisorName}/settings";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let agent = call.param("supervisorName").unwrap_or_default().to_owned();

    let service = crate::apps::service(call.ground());

    match view(call.store(), &call.ground().hooks, (&roots(&call), &service), &agent).await {
      Ok(shown) => Ok(answer(StatusCode::OK, shown)),
      Err(fault) => fault.answer(),
    }
  }
}

crate::moved!(SupervisorSettingsRead);

/// A save of one supervisor's settings, answered with the supervisor as it then stands.
pub struct SupervisorSettingsChange;

impl Route for SupervisorSettingsChange {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/supervisors/{supervisorName}/settings";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let mut body = call.body().await?;

    if !body.is_object() && !body.is_array() {
      return Err(Refused::new(StatusCode::BAD_REQUEST, CONTROL_WORDS.request.action.as_str()));
    }

    let roots = roots(&call);
    let db = call.store();
    if body.get("agent").is_none() {
      body["agent"] = json!(call.param("supervisorName").unwrap_or_default());
    }
    let agent = body.get("agent").and_then(Value::as_str).unwrap_or_default().to_owned();
    let by = asked_by(&body);
    let written = db.begin().await?;
    let made = async {
      let row = supervisor_row(&written, &agent).await?;
      let saved = checked(&written, &row, &body, &agent).await?;

      write(&written, &row, &agent, saved, &by).await
    }
    .await;
    let made = match made {
      Ok(made) => made,
      Err(fault) => return fault.answer(),
    };

    written.commit().await?;

    // A pin or a thinking moved replaces what the supervisor's sessions held through their handoff, made while
    // it is quiet, so the next turn stands the road the row names now (docs/supervisors.md, 「Where a
    // supervisor's turn runs」).
    if made.road_moved
      && let Some(sessions) = call.sessions()
    {
      // Made behind the answer: a handoff takes as long as a session's note does, and the save waits on none.
      let agent = agent.clone();

      tokio::spawn(async move { sessions.road_moved(&agent).await });
    }

    if made.resets {
      carry_out_waiting(&call).await;
    }

    let service = crate::apps::service(call.ground());

    match view(db, &call.ground().hooks, (&roots, &service), &agent).await {
      Ok(shown) => Ok(answer(StatusCode::OK, shown)),
      Err(fault) => fault.answer(),
    }
  }
}

crate::moved!(SupervisorSettingsChange);
