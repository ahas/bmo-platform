//! The base prompts as a caller reads and changes them (docs/base-prompts.md): every prompt with
//! the agents choosing it and the accounts naming it, and a save checked against the policy's bounds,
//! overwriting what it changes under who made it and starting afresh each agent it reaches
//! (docs/session-resets.md). An account asks no reset: its worker's system-prompt file is named by its
//! content, so a changed base opens a fresh session on its next turn.

use std::sync::LazyLock;

use axum::http::{Method, StatusCode};
use axum::response::Response;
use policy::base_prompts::BASE_PROMPTS;
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use regex::Regex;
use sea_orm::{ConnectionTrait, DatabaseTransaction, TransactionTrait};
use serde_json::{Map, Value, json};

use super::fault::{Fault, refused};
use super::sessions::{agent_row, ask_agent_resets, asked_by, awake_agents, carry_out_waiting};
use super::{Call, Refused, Route, answer};
use crate::now_ms;
use crate::text::{grouped, length};
use store::{all, one, run};

static NAME: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(&BASE_PROMPTS.name_pattern).expect("the policy's base prompt name pattern reads")
});

/// One prompt as the store keeps it.
struct Prompt {
  name: String,
  label: String,
  text: String,
  updated_at: Option<i64>,
  updated_by: Option<String>,
}

impl Prompt {
  fn of(row: &sea_orm::QueryResult) -> Result<Self, sea_orm::DbErr> {
    Ok(Self {
      name: row.try_get("", "name")?,
      label: row.try_get("", "label")?,
      text: row.try_get("", "text")?,
      updated_at: row.try_get("", "updated_at")?,
      updated_by: row.try_get("", "updated_by")?,
    })
  }

  async fn get<C: ConnectionTrait>(db: &C, name: &str) -> Result<Option<Self>, Refused> {
    let row = one(db, "SELECT * FROM base_prompts WHERE name = $1", vec![name.into()]).await?;

    Ok(row.as_ref().map(Self::of).transpose()?)
  }

  async fn required<C: ConnectionTrait>(db: &C, name: &str) -> Result<Self, Fault> {
    match Self::get(db, name).await? {
      Some(prompt) => Ok(prompt),
      None => refused("name", fill_marks(&CONTROL_WORDS.base_prompts.unknown, &[("name", name)])),
    }
  }
}

/// The agents that choose a prompt, in name order.
async fn choosers<C: ConnectionTrait>(db: &C, prompt: &str) -> Result<Vec<String>, Refused> {
  all(
    db,
    "SELECT agent FROM agent_base_prompts WHERE prompt = $1 ORDER BY agent",
    vec![prompt.into()],
  )
  .await?
  .iter()
  .map(|row| row.try_get("", "agent").map_err(Refused::from))
  .collect()
}

/// The accounts that name a prompt, in the order the road takes them: each one's id and name.
async fn namers<C: ConnectionTrait>(db: &C, prompt: &str) -> Result<Vec<(String, String)>, Refused> {
  all(
    db,
    "SELECT id, name FROM claude_accounts WHERE base_prompt = $1 ORDER BY place, rowid",
    vec![prompt.into()],
  )
  .await?
  .iter()
  .map(|row| Ok((row.try_get("", "id")?, row.try_get("", "name")?)))
  .collect()
}

// Why a prompt cannot be removed, naming every agent that chooses it and every account that names it.
fn in_use(choosing: &[String], naming: &[String]) -> String {
  let words = &CONTROL_WORDS.base_prompts;
  let listed = |names: &[String]| names.join(", ");

  match (choosing.is_empty(), naming.is_empty()) {
    (false, true) => fill_marks(&words.in_use, &[("agents", &listed(choosing))]),
    (true, false) => fill_marks(&words.in_use_accounts, &[("accounts", &listed(naming))]),
    (false, false) => {
      fill_marks(&words.in_use_both, &[("agents", &listed(choosing)), ("accounts", &listed(naming))])
    }
    (true, true) => String::new(),
  }
}

/// The prompt an agent chooses, or none.
async fn chosen<C: ConnectionTrait>(db: &C, agent: &str) -> Result<Option<String>, Refused> {
  Ok(
    one(db, "SELECT prompt FROM agent_base_prompts WHERE agent = $1", vec![agent.into()])
      .await?
      .map(|row| row.try_get("", "prompt"))
      .transpose()?,
  )
}

/// The name an agent is shown under, or the name itself.
async fn label_of<C: ConnectionTrait>(db: &C, agent: &str) -> Result<String, Refused> {
  let row = agent_row(db, agent).await?;
  let label: Option<String> = row.as_ref().map(|row| row.try_get("", "label")).transpose()?.flatten();

  Ok(label.unwrap_or_else(|| agent.to_owned()))
}

/// Every prompt with the agents choosing it, every choice by agent, and the value names a prompt may
/// carry.
async fn view<C: ConnectionTrait>(db: &C) -> Result<Value, Refused> {
  let mut prompts = vec![];
  let mut choices = Map::new();

  for row in all(db, "SELECT * FROM base_prompts ORDER BY name", vec![]).await? {
    let prompt = Prompt::of(&row)?;
    let mut chosen_by = vec![];

    for agent in choosers(db, &prompt.name).await? {
      let label = label_of(db, &agent).await?;

      choices.insert(agent.clone(), json!(prompt.name));
      chosen_by.push(json!({ "agent": agent, "label": label }));
    }

    let named_by: Vec<Value> = namers(db, &prompt.name)
      .await?
      .into_iter()
      .map(|(id, name)| json!({ "id": id, "name": name }))
      .collect();

    prompts.push(json!({
      "name": prompt.name,
      "label": prompt.label,
      "characters": length(&prompt.text),
      "text": prompt.text,
      "updatedAt": prompt.updated_at,
      "updatedBy": prompt.updated_by,
      "choosers": chosen_by,
      "accounts": named_by,
    }));
  }

  Ok(json!({ "prompts": prompts, "choices": choices, "values": BASE_PROMPTS.values }))
}

fn checked_label(label: Option<&Value>) -> Result<String, Fault> {
  match label.and_then(Value::as_str).map(str::trim) {
    Some(label) if !label.is_empty() && length(label) <= BASE_PROMPTS.label_most => Ok(label.to_owned()),
    _ => refused(
      "label",
      fill_marks(&CONTROL_WORDS.base_prompts.label, &[("most", &BASE_PROMPTS.label_most.to_string())]),
    ),
  }
}

fn checked_text(text: Option<&Value>) -> Result<String, Fault> {
  match text.and_then(Value::as_str) {
    Some(text) if !text.trim().is_empty() && length(text) <= BASE_PROMPTS.text_most => Ok(text.to_owned()),
    _ => refused(
      "text",
      fill_marks(&CONTROL_WORDS.base_prompts.text, &[("most", &grouped(BASE_PROMPTS.text_most))]),
    ),
  }
}

// A name the body gives, or nothing.
fn named<'a>(body: &'a Value, key: &str) -> &'a str {
  body.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The change one save asks for, made in the store's one transaction with the resets it reaches.
struct Change<'a> {
  db: &'a DatabaseTransaction,
  by: String,
  at: i64,
  // Whether an ask agency carries out was written.
  tell: bool,
}

impl Change<'_> {
  async fn create(&mut self, body: &Value) -> Result<(), Fault> {
    let name = named(body, "name");

    if !NAME.is_match(name) || length(name) > BASE_PROMPTS.name_most {
      return refused(
        "name",
        fill_marks(
          &CONTROL_WORDS.base_prompts.name_shape,
          &[("most", &BASE_PROMPTS.name_most.to_string())],
        ),
      );
    }

    let label = checked_label(body.get("label"))?;
    let text = checked_text(body.get("text"))?;

    if Prompt::get(self.db, name).await?.is_some() {
      return refused("name", fill_marks(&CONTROL_WORDS.base_prompts.taken, &[("name", name)]));
    }

    run(
      self.db,
      "INSERT INTO base_prompts (name, label, text, updated_at, updated_by) VALUES ($1, $2, $3, $4, $5)",
      vec![name.into(), label.into(), text.into(), self.at.into(), self.by.clone().into()],
    )
    .await?;

    Ok(())
  }

  async fn save(&mut self, body: &Value) -> Result<(), Fault> {
    let name = named(body, "name");
    let label = body.get("label").map(|label| checked_label(Some(label))).transpose()?;
    let text = body.get("text").map(|text| checked_text(Some(text))).transpose()?;
    let held = Prompt::required(self.db, name).await?;
    let label = label.filter(|label| *label != held.label);
    let text = text.filter(|text| *text != held.text);

    if label.is_none() && text.is_none() {
      return Ok(());
    }

    let text_changed = text.is_some();

    run(
      self.db,
      "UPDATE base_prompts SET label = $1, text = $2, updated_at = $3, updated_by = $4 WHERE name = $5",
      vec![
        label.unwrap_or(held.label).into(),
        text.unwrap_or(held.text).into(),
        self.at.into(),
        self.by.clone().into(),
        name.into(),
      ],
    )
    .await?;

    // Only an agent's choice reaches sessions; an account naming the prompt asks none, since the next
    // run on it is handed a file named by this text and opens a fresh session of its own.
    if text_changed {
      for agent in choosers(self.db, name).await? {
        self.reset_reached(&agent).await?;
      }
    }

    Ok(())
  }

  async fn remove(&mut self, body: &Value) -> Result<(), Fault> {
    let name = named(body, "name");
    Prompt::required(self.db, name).await?;
    let choosing = choosers(self.db, name).await?;
    let naming: Vec<String> = namers(self.db, name).await?.into_iter().map(|(_, named)| named).collect();

    if !choosing.is_empty() || !naming.is_empty() {
      return refused("name", in_use(&choosing, &naming));
    }

    run(self.db, "DELETE FROM base_prompts WHERE name = $1", vec![name.into()]).await?;

    Ok(())
  }

  async fn choose(&mut self, body: &Value) -> Result<(), Fault> {
    let agent = named(body, "agent");

    if !self.may_choose(agent).await? {
      return refused("agent", fill_marks(&CONTROL_WORDS.base_prompts.not_chooser, &[("agent", agent)]));
    }

    let prompt = match body.get("prompt") {
      Some(Value::Null) => None,
      Some(Value::String(prompt)) => Some(prompt.as_str()),
      _ => return refused("prompt", CONTROL_WORDS.base_prompts.choice.as_str()),
    };

    if let Some(prompt) = prompt {
      Prompt::required(self.db, prompt).await?;
    }

    let held = chosen(self.db, agent).await?;

    if held.as_deref() == prompt {
      return Ok(());
    }

    match prompt {
      None => run(self.db, "DELETE FROM agent_base_prompts WHERE agent = $1", vec![agent.into()]).await?,
      Some(prompt) => {
        run(
          self.db,
          "INSERT INTO agent_base_prompts (agent, prompt) VALUES ($1, $2)
           ON CONFLICT (agent) DO UPDATE SET prompt = excluded.prompt",
          vec![agent.into(), prompt.into()],
        )
        .await?
      }
    };
    self.reset_reached(agent).await?;

    Ok(())
  }

  // Which agents may choose a prompt: a worker of its own and an agent whose turns its settings' pieces
  // make. A support worker takes its base's choice.
  async fn may_choose(&self, agent: &str) -> Result<bool, Refused> {
    let Some(row) = agent_row(self.db, agent).await? else {
      return Ok(false);
    };
    let kind: String = row.try_get("", "kind")?;
    let support_of: Option<String> = row.try_get("", "support_of")?;
    let pieces = crate::conversational::Settings::of(&row)
      .map_err(Refused::from)?
      .is_some_and(|settings| settings.shape == crate::conversational::Shape::Pieces);

    Ok((kind == "worker" && support_of.is_none()) || pieces)
  }

  // Every session a choice reaches: the agent's own, and a worker's support workers', which run on its
  // choice.
  async fn reset_reached(&mut self, agent: &str) -> Result<(), Refused> {
    let mut reached = vec![agent.to_owned()];

    for row in awake_agents(self.db).await? {
      if row.try_get::<Option<String>>("", "support_of")?.as_deref() == Some(agent) {
        reached.push(row.try_get("", "name")?);
      }
    }

    for one in reached {
      self.tell |=
        ask_agent_resets(self.db, &one, &self.by, &CONTROL_WORDS.base_prompts.reset_reason).await?;
    }

    Ok(())
  }
}

/// Every base prompt with its choosers.
pub struct BasePrompts;

impl Route for BasePrompts {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/prompt-documents";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let view = view(call.store()).await?;
    let items: Vec<Value> = view
      .get("prompts")
      .and_then(Value::as_array)
      .map(|prompts| {
        prompts
          .iter()
          .map(|prompt| {
            let mut document = prompt.clone();
            document["kind"] = json!("base");
            document["version"] = prompt.get("updatedAt").cloned().unwrap_or(json!(""));
            document
          })
          .collect()
      })
      .unwrap_or_default();

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": false })))
  }
}

crate::moved!(BasePrompts);

/// One change: a prompt made, saved or removed, or an agent's choice, answered with the prompts as
/// they then stand.
pub struct BasePromptChange;

impl Route for BasePromptChange {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/prompt-documents/{documentName}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let mut body = call.body().await?;
    let name = call.param("documentName").unwrap_or_default().to_owned();

    if !body.is_object() && !body.is_array() {
      return Err(Refused::new(StatusCode::BAD_REQUEST, CONTROL_WORDS.request.action.as_str()));
    }
    if body.get("name").is_none() {
      body["name"] = json!(name);
    }
    if body.get("action").is_none() {
      body["action"] = json!("save");
    }

    let db = call.store();
    let written = db.begin().await?;
    let mut change = Change {
      db: &written,
      by: asked_by(&body),
      at: now_ms() / 1000,
      tell: false,
    };
    let made = match body.get("action").and_then(Value::as_str) {
      Some("create") => change.create(&body).await,
      Some("save") => change.save(&body).await,
      Some("remove") => change.remove(&body).await,
      Some("choose") => change.choose(&body).await,
      _ => refused("action", CONTROL_WORDS.request.action.as_str()),
    };
    let tell = change.tell;

    match made {
      Ok(()) => written.commit().await?,
      Err(fault) => return fault.answer(),
    }

    if tell {
      carry_out_waiting(&call).await;
    }

    Ok(answer(StatusCode::OK, view(db).await?))
  }
}

crate::moved!(BasePromptChange);

/// One prompt document, by its name.
pub struct PromptDocument;

impl Route for PromptDocument {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/prompt-documents/{documentName}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("documentName").unwrap_or_default();
    let row = one(call.store(), "SELECT * FROM base_prompts WHERE name = $1", vec![name.into()]).await?;

    if let Some(row) = row {
      let prompt = Prompt::of(&row)?;

      return Ok(answer(
        StatusCode::OK,
        json!({
          "name": prompt.name,
          "kind": "base",
          "agentName": Value::Null,
          "version": prompt.updated_at,
          "text": prompt.text,
        }),
      ));
    }

    let trees = crate::lines::Trees::of(call.ground());
    let text = std::fs::read_to_string(trees.rulebook_path(name))
      .map_err(|_| Refused::new(StatusCode::NOT_FOUND, format!("agency keeps no document named {name}")))?;

    Ok(answer(
      StatusCode::OK,
      json!({ "name": name, "kind": "rulebook", "agentName": Value::Null, "version": Value::Null, "text": text.trim() }),
    ))
  }
}

crate::moved!(PromptDocument);
