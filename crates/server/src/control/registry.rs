//! The registry of models as a caller reads and changes it (docs/own-llm.md, 「Where it is set」): every
//! registration with each address's key said in one word and what names it, and a change held to the
//! policy's rules. A key given in a save is written by the harness-machine crate into a file of agency's
//! own and never answered back (docs/harness-machine.md, 「The keys」). A registration as
//! the service a chat with no tool asks, and the settings a work key asks it for, stand here for every
//! route that reads them.

use std::path::Path;
use std::sync::LazyLock;

use harness_machine::card_master::{Master, MasterAddress};
use harness_machine::engine_health::{self, Aside, EngineHealth};
use harness_machine::{gateway, keys};
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use policy::providers::registered::REGISTERED;
use policy::registry::{REGISTRY, WholeBounds};
use regex::Regex;
use sea_orm::{ConnectionTrait, QueryResult, TransactionTrait};
use serde_json::{Value, json};

use super::Refused;
use super::fault::{Fault, refused};
use crate::now_ms;
use crate::text::{grouped, length};
use store::{all, one, run};

/// The registry's provider, the one a registration is offered and pinned under.
pub const PROVIDER: &str = "registered";

static NAME: LazyLock<Regex> = LazyLock::new(|| pattern(&REGISTRY.name_pattern));
static MODEL: LazyLock<Regex> = LazyLock::new(|| pattern(&REGISTRY.model_pattern));
static KEY_FILE: LazyLock<Regex> = LazyLock::new(|| pattern(&REGISTRY.key_file_pattern));
static LANGUAGE: LazyLock<Regex> = LazyLock::new(|| pattern(&REGISTRY.language_pattern));

fn pattern(text: &str) -> Regex {
  Regex::new(text).expect("the policy's registry patterns read")
}

/// One address of a registration, as its row keeps it.
#[derive(Clone, Debug)]
pub struct Address {
  pub node: String,
  pub url: String,
  pub key_file: Option<String>,
  pub key_owned: bool,
  /// Whether the instance behind it keeps role files (docs/own-llm.md, 「Role files」).
  pub role_files: bool,
  /// The port of this machine's loopback the address is asked at through its owner's PC, none where it is asked
  /// as it stands (docs/own-llm.md, 「A person's local AI」).
  pub reach_port: Option<i64>,
}

/// One registration with its addresses in their order.
#[derive(Clone, Debug)]
pub struct Registration {
  pub name: String,
  pub label: String,
  pub model: String,
  pub context_tokens: i64,
  pub answer_tokens: i64,
  pub thinking: bool,
  pub walked: bool,
  pub keeps_research: bool,
  /// The arbiter service its engine is, none where it is served at addresses of its own
  /// (docs/own-llm.md, 「An engine the arbiter stands」).
  pub card_service: Option<String>,
  /// Whether the card service's instances keep role files (docs/own-llm.md, 「Role files」).
  pub role_files: bool,
  /// The languages a run on it writes a result for a person in, each by its English name; empty for every
  /// language (docs/own-llm.md, 「What a registration holds」).
  pub languages: Vec<String>,
  /// The account it belongs to, none for the platform's (docs/supply.md, 「Owned supply」).
  pub account: Option<i64>,
  /// Whether it is on: a person's local AI they switched off offers no supply (docs/supply.md, 「A person's local
  /// AI」).
  pub enabled: bool,
  pub addresses: Vec<Address>,
}

impl Registration {
  /// Whether a run may name it: it holds an address, or names a card service the master places.
  pub fn offered(&self) -> bool {
    !self.addresses.is_empty() || self.card_service.is_some()
  }

  /// Whether a session placed on `node` may be resumed there: a node among its addresses, or any node on
  /// a registration naming a card service, whose session goes where its demand is granted.
  pub fn holds_node(&self, node: &str) -> bool {
    self.card_service.is_some() || self.addresses.iter().any(|address| address.node == node)
  }
}

fn flag(row: &QueryResult, column: &str) -> Result<bool, sea_orm::DbErr> {
  Ok(row.try_get::<i64>("", column)? == 1)
}

// The languages a registration's row names, kept as a JSON list; none, or a list that does not read, for every
// language.
fn languages(row: &QueryResult) -> Result<Vec<String>, sea_orm::DbErr> {
  Ok(
    row
      .try_get::<Option<String>>("", "languages")?
      .and_then(|text| serde_json::from_str(&text).ok())
      .unwrap_or_default(),
  )
}

/// Every registration in the order they were added, each with its addresses.
pub async fn registrations<C: ConnectionTrait>(db: &C) -> Result<Vec<Registration>, Refused> {
  let rows = all(db, "SELECT * FROM registered_models ORDER BY added_at, name", vec![]).await?;
  let addresses = all(
    db,
    "SELECT * FROM registered_model_addresses ORDER BY registration, position, node",
    vec![],
  )
  .await?;
  let mut read = vec![];

  for row in rows {
    let name: String = row.try_get("", "name")?;
    let mut held = vec![];

    for address in &addresses {
      if address.try_get::<String>("", "registration")? == name {
        held.push(Address {
          node: address.try_get("", "node")?,
          url: address.try_get("", "url")?,
          key_file: address.try_get("", "key_file")?,
          key_owned: flag(address, "key_owned")?,
          role_files: flag(address, "role_files")?,
          reach_port: address.try_get("", "reach_port")?,
        });
      }
    }

    read.push(Registration {
      label: row.try_get("", "label")?,
      model: row.try_get("", "model")?,
      context_tokens: row.try_get("", "context_tokens")?,
      answer_tokens: row.try_get("", "answer_tokens")?,
      thinking: flag(&row, "thinking")?,
      walked: flag(&row, "walked")?,
      keeps_research: flag(&row, "keeps_research")?,
      card_service: row.try_get("", "card_service")?,
      role_files: flag(&row, "role_files")?,
      languages: languages(&row)?,
      account: row.try_get("", "account")?,
      enabled: flag(&row, "enabled")?,
      addresses: held,
      name,
    });
  }

  Ok(read)
}

/// One registration, or none.
pub async fn registration<C: ConnectionTrait>(db: &C, name: &str) -> Result<Option<Registration>, Refused> {
  Ok(registrations(db).await?.into_iter().find(|one| one.name == name))
}

/// The health of the engine behind a registration, asked at its addresses, or none where none
/// answers or the registry holds no such registration. An engine that answers at all is written down as
/// having answered now, so a screen shows when it last did even after it stops answering
/// (docs/own-llm.md, 「The registry」). A registration naming a card service is asked through the
/// arbiter's master: a stopped engine is no outage, so it stands while the service is placed on a node no
/// claim and no lower tier holds, its details those of an instance standing where one answers, and stands
/// aside while every node of its placement is held (「An engine the arbiter stands」).
pub async fn registration_health<C: ConnectionTrait>(
  db: &C,
  data_root: &Path,
  name: &str,
) -> Result<Option<EngineHealth>, Refused> {
  let Some(held) = registration(db, name).await? else {
    return Ok(None);
  };
  let health = match &held.card_service {
    Some(service) => card_health(data_root, service).await,
    None => {
      let addresses: Vec<engine_health::Address> = held
        .addresses
        .into_iter()
        .map(|address| engine_health::Address {
          url: address.url,
          key_file: address.key_file,
        })
        .collect();

      engine_health::health(data_root, &addresses).await
    }
  };

  if health.is_some() {
    let _ = run(
      db,
      r#"UPDATE "registered_models" SET "last_answered_at" = $1 WHERE "name" = $2"#,
      vec![(now_ms() / 1000).into(), name.into()],
    )
    .await;
  }

  Ok(health)
}

// The health of a card service as the arbiter's master shows it, none where the master is named nowhere,
// cannot be read or places no such service.
async fn card_health(data_root: &Path, service: &str) -> Option<EngineHealth> {
  let master = Master::new(MasterAddress::of_process().ok().flatten()?);
  let standing = master.standing(service).await.ok().flatten()?;
  let addresses: Vec<engine_health::Address> = standing
    .instances
    .iter()
    .filter_map(|instance| {
      Some(engine_health::Address {
        url: instance.address.clone()?,
        key_file: instance.key_file.clone(),
      })
    })
    .collect();
  let standing_health = engine_health::health(data_root, &addresses).await;
  let mut health = standing_health.unwrap_or(EngineHealth {
    context: None,
    model: None,
    in_hand: None,
    aside: None,
  });

  if !standing.open {
    health.aside = Some(Aside {
      reason: format!("every node {service} is placed on is held by a claim or a lower tier"),
      until: Value::Null,
    });
  }

  Some(health)
}

/// One registration as the boundary's model resource answers it: its name as the key, its model, the
/// first tier it is mapped at in the ladder's order and whether its addresses answer, or none where the
/// registry holds no registration by that name.
pub async fn registered_model<C: ConnectionTrait>(
  db: &C,
  data_root: &Path,
  name: &str,
) -> Result<Option<Value>, Refused> {
  let Some(row) = one(
    db,
    r#"SELECT "model", "card_service", "role_files", "context_tokens", "answer_tokens", "thinking",
         "languages", "account" FROM "registered_models" WHERE "name" = $1"#,
    vec![name.into()],
  )
  .await?
  else {
    return Ok(None);
  };
  let model: String = row.try_get("", "model")?;
  let card_service: Option<String> = row.try_get("", "card_service")?;
  let role_files = flag(&row, "role_files")?;
  let healthy = registration_health(db, data_root, name)
    .await?
    .is_some_and(|health| health.aside.is_none());
  // The probe above wrote an answer down; the row read now carries the moment it last stood at.
  let last_answered_at = one(
    db,
    r#"SELECT "last_answered_at" FROM "registered_models" WHERE "name" = $1"#,
    vec![name.into()],
  )
  .await?
  .and_then(|row| row.try_get::<Option<i64>>("", "last_answered_at").ok().flatten());

  Ok(Some(json!({
    "key": format!("{PROVIDER}::{name}"),
    "name": name,
    "provider": PROVIDER,
    "model": model,
    "cardService": card_service,
    "roleFiles": role_files,
    "usedBy": referrers(db, name).await?,
    "healthy": healthy,
    "lastAnsweredAt": last_answered_at,
    "contextTokens": row.try_get::<i64>("", "context_tokens")?,
    "answerTokens": row.try_get::<i64>("", "answer_tokens")?,
    "thinking": flag(&row, "thinking")?,
    "languages": languages(&row)?,
    "account": row.try_get::<Option<i64>>("", "account")?,
  })))
}

/// Every registration as the model resource lists it.
pub async fn listed_models<C: ConnectionTrait>(db: &C, data_root: &Path) -> Result<Vec<Value>, Refused> {
  let mut listed = vec![];

  for held in registrations(db).await? {
    if let Some(model) = registered_model(db, data_root, &held.name).await? {
      listed.push(model);
    }
  }

  Ok(listed)
}

fn refused_languages<T>() -> Result<T, Fault> {
  refused(
    "languages",
    fill_marks(&CONTROL_WORDS.registry.languages, &[("most", &REGISTRY.languages_most.to_string())]),
  )
}

/// The languages a change names, held to the registry's rule: each a name the policy's pattern takes, trimmed,
/// no more than its most, a name named twice in any case kept once, in the order named.
pub fn checked_languages(named: &[Value]) -> Result<Vec<String>, Fault> {
  let mut kept: Vec<String> = vec![];

  for one in named {
    let Some(language) = one.as_str().map(str::trim).filter(|language| LANGUAGE.is_match(language)) else {
      return refused_languages();
    };

    if !kept.iter().any(|held| held.eq_ignore_ascii_case(language)) {
      kept.push(language.to_owned());
    }
  }

  if kept.len() > REGISTRY.languages_most {
    return refused_languages();
  }

  Ok(kept)
}

/// The owner a change names for a registration or a member: a user's id, a whole number, or null for the
/// platform's (docs/supply.md, 「Owned supply」).
pub fn checked_account(named: &Value) -> Result<Option<i64>, Fault> {
  match named {
    Value::Null => Ok(None),
    Value::Number(number) if number.as_i64().is_some() => Ok(number.as_i64()),
    _ => refused("account", CONTROL_WORDS.registry.account.as_str()),
  }
}

/// A card service as a registration names it: none, or a name held to the registry's rule.
pub fn checked_card_service(service: Option<&str>) -> Result<Option<String>, Fault> {
  match service.map(str::trim).filter(|service| !service.is_empty() && *service != "none") {
    None => Ok(None),
    Some(service) if NAME.is_match(service) => Ok(Some(service.to_owned())),
    Some(_) => refused("cardService", CONTROL_WORDS.registry.card_service.as_str()),
  }
}

/// A registration's model changed to the name given, the name held to the registry's own rule.
pub async fn change_model<C: ConnectionTrait>(db: &C, name: &str, model: &str) -> Result<(), Fault> {
  if !MODEL.is_match(model) {
    return refused("model", CONTROL_WORDS.registry.model.as_str());
  }

  run(
    db,
    r#"UPDATE "registered_models" SET "model" = $1 WHERE "name" = $2"#,
    vec![model.into(), name.into()],
  )
  .await?;

  Ok(())
}

/// A registration as the service a chat with no tool asks directly: the gateway, under the
/// registration's name, sized to its window. None where the registry holds no such registration with
/// an address.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnLlmService {
  pub name: &'static str,
  pub label: String,
  pub short_label: String,
  pub base_url: String,
  pub key_file: String,
  pub registration: String,
}

pub async fn service_of<C: ConnectionTrait>(db: &C, model: &str) -> Result<Option<OwnLlmService>, Refused> {
  Ok(registration(db, model).await?.filter(Registration::offered).map(|held| OwnLlmService {
    name: PROVIDER,
    short_label: held.label.clone(),
    label: held.label,
    base_url: gateway::url(std::env::var("BMO_LLM_GATEWAY_URL").ok().as_deref()),
    key_file: REGISTERED.presence.key_file.clone(),
    registration: held.name,
  }))
}

/// The answer ceiling of a key that is no gate, where its tier names none.
pub const DEFAULT_ANSWER_TOKENS: i64 = 4_096;

/// What one work key's tier mapping on an own service asks it for, with the registration's window where
/// the settings were read for a run on it.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnLlmSettings {
  pub provider: &'static str,
  pub model: String,
  pub effort: Option<String>,
  pub thinking: bool,
  pub answer_tokens: i64,
  pub context_tokens: Option<i64>,
}

impl OwnLlmSettings {
  /// The settings as a screen reads them, the window only where it was read.
  pub fn view(&self) -> Value {
    let mut view = json!({
      "provider": self.provider,
      "model": self.model,
      "effort": self.effort,
      "thinking": self.thinking,
      "answerTokens": self.answer_tokens,
    });

    if let Some(window) = self.context_tokens {
      view["contextTokens"] = json!(window);
    }

    view
  }
}

/// An order's entry as an own service reads it: its model, effort and answer ceiling, each where named.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mapping {
  pub model: String,
  pub thinking: Option<bool>,
  pub effort: Option<String>,
  pub answer_tokens: Option<i64>,
}

impl From<&crate::order::Entry> for Mapping {
  fn from(held: &crate::order::Entry) -> Self {
    Self {
      model: held.model.clone(),
      thinking: held.thinks(),
      effort: held.effort.clone(),
      answer_tokens: held.answer_tokens,
    }
  }
}

/// The settings a work key asks an own service for: the mapping's model and thinking, and its answer
/// ceiling or else the work's own default. A mapping that cannot be read leaves the model empty.
pub fn own_llm_settings(service: &OwnLlmService, mapping: Option<&Mapping>) -> OwnLlmSettings {
  OwnLlmSettings {
    provider: service.name,
    model: mapping.map(|one| one.model.clone()).unwrap_or_default(),
    effort: mapping.and_then(|one| one.effort.clone()),
    thinking: mapping.and_then(|one| one.effort.as_deref()) == Some("thinking"),
    answer_tokens: mapping.and_then(|one| one.answer_tokens).unwrap_or(DEFAULT_ANSWER_TOKENS),
    context_tokens: None,
  }
}

/// Whether a run on a registration thinks: the mapping's word where it names one, the registration's
/// default otherwise.
fn thinks(registration: &Registration, effort: Option<&str>) -> bool {
  match effort {
    Some("thinking") => true,
    Some("no-thinking") => false,
    _ => registration.thinking,
  }
}

/// What a mapping of the registry asks a run for: its registration, thinking as the mapping names it
/// or else as the registration does, the mapping's answer ceiling or else the registration's, and the
/// registration's window. A mapping naming a registration the registry does not hold asks for
/// it all the same, and the run is refused where it is launched.
pub async fn run_settings<C: ConnectionTrait>(
  db: &C,
  mapping: Option<&Mapping>,
) -> Result<OwnLlmSettings, Refused> {
  let held = match mapping {
    Some(mapping) => registration(db, &mapping.model).await?,
    None => None,
  };
  let effort = mapping.and_then(|one| one.effort.clone());

  Ok(OwnLlmSettings {
    provider: PROVIDER,
    model: mapping.map(|one| one.model.clone()).unwrap_or_default(),
    thinking: mapping.and_then(|one| one.thinking).unwrap_or_else(|| match &held {
      Some(held) => thinks(held, effort.as_deref()),
      None => effort.as_deref() == Some("thinking"),
    }),
    effort,
    answer_tokens: mapping
      .and_then(|one| one.answer_tokens)
      .or(held.as_ref().map(|held| held.answer_tokens))
      .unwrap_or(DEFAULT_ANSWER_TOKENS),
    context_tokens: held.as_ref().map(|held| held.context_tokens),
  })
}

/// What names a registration, each thing that may name one answering for itself (docs/api.md, 「providers,
/// models and model files」): every supervisor and role whose order names it on the registry's provider, every
/// repository whose pin names it, and the model catalogue's entries served behind it.
pub async fn referrers<C: ConnectionTrait>(db: &C, name: &str) -> Result<Vec<String>, Refused> {
  let names = |rows: Vec<QueryResult>, column: &str| -> Result<Vec<String>, Refused> {
    rows
      .iter()
      .map(|row| row.try_get::<String>("", column).map_err(Refused::from))
      .collect()
  };
  let mut used = vec![];

  // A supervisor names it where its road stands on it.
  for supervisor in crate::supervisors::every(db).await? {
    if supervisor
      .road
      .as_ref()
      .is_some_and(|road| road.provider == PROVIDER && road.model == name)
    {
      used.push(format!("supervisor {}", supervisor.name));
    }
  }

  // A role names it where an agent of the role stands on it, switched on or off.
  for role in crate::agent_roles::every(db).await? {
    if role.agents.iter().any(|agent| agent.on(PROVIDER, name)) {
      used.push(format!("role {}", role.name));
    }
  }

  for repository in crate::repositories::every(db).await? {
    if repository.pin.as_ref().is_some_and(|pin| pin.provider == PROVIDER && pin.model == name) {
      used.push(format!("repository {}", repository.name));
    }
  }

  let entries = names(
    all(
      db,
      "SELECT name FROM model_catalogue WHERE registration = $1 ORDER BY added_at, name",
      vec![name.into()],
    )
    .await?,
    "name",
  )?;

  used.extend(
    entries
      .iter()
      .map(|entry| fill_marks(&CONTROL_WORDS.registry.model_file, &[("entry", entry)])),
  );

  Ok(used)
}

/// Every registration as the screen shows it, each address's key said in one word, and the gateway as
/// its launcher last wrote of itself.
pub async fn view<C: ConnectionTrait>(db: &C, data_root: &Path) -> Result<Value, Refused> {
  let mut shown = vec![];

  for one in registrations(db).await? {
    let addresses: Vec<Value> = one
      .addresses
      .iter()
      .map(|address| {
        json!({
          "node": address.node,
          "url": address.url,
          "key": keys::state(data_root, address.key_file.as_deref()),
          "roleFiles": address.role_files,
        })
      })
      .collect();

    shown.push(json!({
      "name": one.name,
      "label": one.label,
      "model": one.model,
      "contextTokens": one.context_tokens,
      "answerTokens": one.answer_tokens,
      "thinking": one.thinking,
      "walked": one.walked,
      "keepsResearch": one.keeps_research,
      "cardService": one.card_service,
      "roleFiles": one.role_files,
      "languages": one.languages,
      "account": one.account,
      "addresses": addresses,
      "referrers": referrers(db, &one.name).await?,
    }));
  }

  Ok(json!({ "registrations": shown, "gateway": gateway::status(data_root) }))
}

// A text the body gives, trimmed, or an empty one.
fn text(body: &Value, key: &str) -> String {
  body.get(key).and_then(Value::as_str).map(str::trim).unwrap_or_default().to_owned()
}

// A field the body names: left out and null both fall through to what stands.
fn given<'a>(body: &'a Value, key: &str) -> Option<&'a Value> {
  body.get(key).filter(|value| !value.is_null())
}

fn checked_name(name: &str) -> Result<(), Fault> {
  if NAME.is_match(name) {
    return Ok(());
  }

  refused("name", CONTROL_WORDS.registry.name.as_str())
}

fn checked_node(node: &str) -> Result<(), Fault> {
  if NAME.is_match(node) {
    return Ok(());
  }

  refused("node", CONTROL_WORDS.registry.node.as_str())
}

/// The address as the URL parser reads it: an http or https address whose path ends in the chat
/// completions path, with no query, fragment or credentials.
pub fn checked_url(address: &str) -> Result<String, Fault> {
  let Ok(parsed) = url::Url::parse(address) else {
    return refused("url", CONTROL_WORDS.registry.url_unread.as_str());
  };

  if !matches!(parsed.scheme(), "http" | "https")
    || !parsed.path().ends_with("/chat/completions")
    || parsed.query().is_some_and(|query| !query.is_empty())
    || parsed.fragment().is_some_and(|fragment| !fragment.is_empty())
    || !parsed.username().is_empty()
    || parsed.password().is_some()
  {
    return refused("url", CONTROL_WORDS.registry.url_shape.as_str());
  }

  Ok(parsed.to_string())
}

pub fn checked_key_file(key_file: Option<&str>) -> Result<(), Fault> {
  match key_file {
    Some(file) if !KEY_FILE.is_match(file) || file.split('/').any(|part| part == "." || part == "..") => {
      refused("key", CONTROL_WORDS.registry.key_file.as_str())
    }
    _ => Ok(()),
  }
}

/// A key file an address names in the platform's secret space: one plain file name the registry's rule takes.
fn checked_named_key_file(file: &str) -> Result<String, Fault> {
  match super::claude_account_changes::plain_secret(file) {
    Some(plain) if KEY_FILE.is_match(plain) => Ok(plain.to_owned()),
    _ => refused("keyFile", CONTROL_WORDS.registry.key_file_name.as_str()),
  }
}

// A whole number in its bounds, however it was written: a number with nothing after its point counts.
fn whole(field: &str, value: Option<&Value>, bounds: &WholeBounds, label: &str) -> Result<i64, Fault> {
  let held = value.and_then(Value::as_number).and_then(|number| {
    number
      .as_i64()
      .or_else(|| number.as_f64().filter(|float| float.fract() == 0.0).map(|float| float as i64))
  });

  match held {
    Some(held) if bounds.holds(held) => Ok(held),
    _ => refused(
      field,
      fill_marks(
        &CONTROL_WORDS.registry.whole,
        &[
          ("label", label),
          ("min", &grouped(bounds.min as usize)),
          ("max", &grouped(bounds.max as usize)),
        ],
      ),
    ),
  }
}

/// Every field a registration holds apart from its name and its addresses, checked together, since
/// the answer size is held to the window.
struct Fields {
  label: String,
  model: String,
  context_tokens: i64,
  answer_tokens: i64,
  thinking: bool,
  walked: bool,
  keeps_research: bool,
}

impl Fields {
  fn checked(fields: &Value) -> Result<Self, Fault> {
    let label = match fields.get("label").and_then(Value::as_str) {
      Some(label) if !label.trim().is_empty() && length(label) <= REGISTRY.label_most => {
        label.trim().to_owned()
      }
      _ => {
        return refused(
          "label",
          fill_marks(&CONTROL_WORDS.registry.label, &[("most", &REGISTRY.label_most.to_string())]),
        );
      }
    };
    let model = match fields.get("model").and_then(Value::as_str) {
      Some(model) if MODEL.is_match(model) => model.to_owned(),
      _ => return refused("model", CONTROL_WORDS.registry.model.as_str()),
    };
    let context_tokens = whole(
      "contextTokens",
      fields.get("contextTokens"),
      &REGISTRY.context_tokens,
      &CONTROL_WORDS.registry.context_label,
    )?;
    let answer_tokens = whole(
      "answerTokens",
      fields.get("answerTokens"),
      &REGISTRY.answer_tokens,
      &CONTROL_WORDS.registry.answer_label,
    )?;

    if answer_tokens >= context_tokens {
      return refused("answerTokens", CONTROL_WORDS.registry.answer_under_window.as_str());
    }

    let (Some(thinking), Some(walked)) = (
      fields.get("thinking").and_then(Value::as_bool),
      fields.get("walked").and_then(Value::as_bool),
    ) else {
      return refused("thinking", CONTROL_WORDS.registry.switches.as_str());
    };
    let Some(keeps_research) = fields.get("keepsResearch").and_then(Value::as_bool) else {
      return refused("keepsResearch", CONTROL_WORDS.registry.keeps_research.as_str());
    };

    Ok(Self {
      label,
      model,
      context_tokens,
      answer_tokens,
      thinking,
      walked,
      keeps_research,
    })
  }

  fn values(&self) -> Vec<sea_orm::Value> {
    vec![
      self.label.clone().into(),
      self.model.clone().into(),
      self.context_tokens.into(),
      self.answer_tokens.into(),
      i64::from(self.thinking).into(),
      i64::from(self.walked).into(),
      i64::from(self.keeps_research).into(),
    ]
  }
}

/// One change of the registry, made in one transaction; the key files it leaves behind are taken away
/// once it has committed.
struct Change<'a, C> {
  db: &'a C,
  data_root: &'a Path,
  // The key files agency wrote that the change leaves no address naming.
  gone: Vec<Address>,
}

impl<C: ConnectionTrait> Change<'_, C> {
  async fn required(&self, name: &str) -> Result<Registration, Fault> {
    match registration(self.db, name).await? {
      Some(held) => Ok(held),
      None => refused("name", fill_marks(&CONTROL_WORDS.registry.unknown, &[("name", name)])),
    }
  }

  async fn save(&mut self, body: &Value, name: &str) -> Result<(), Fault> {
    let held = registration(self.db, name).await?;
    let stands = |of: fn(&Registration) -> Value| held.as_ref().map(of).unwrap_or(Value::Null);
    let or_held =
      |key: &str, of: fn(&Registration) -> Value| given(body, key).cloned().unwrap_or_else(|| stands(of));
    let fields = json!({
      // A new registration shown under its name until it is given a label.
      "label": body.get("label").filter(|label| label.is_string()).cloned()
        .unwrap_or_else(|| held.as_ref().map_or_else(|| json!(name), |held| json!(held.label))),
      "model": body.get("model").and_then(Value::as_str).map(|model| json!(model.trim())).unwrap_or_else(|| stands(|held| json!(held.model))),
      "contextTokens": or_held("contextTokens", |held| json!(held.context_tokens)),
      "answerTokens": or_held("answerTokens", |held| json!(held.answer_tokens)),
      "thinking": given(body, "thinking").cloned().unwrap_or_else(|| json!(held.as_ref().is_some_and(|held| held.thinking))),
      "walked": given(body, "walked").cloned().unwrap_or_else(|| json!(held.as_ref().is_some_and(|held| held.walked))),
      "keepsResearch": given(body, "keepsResearch").cloned()
        .unwrap_or_else(|| json!(held.as_ref().is_some_and(|held| held.keeps_research))),
    });

    if held.is_none() {
      checked_name(name)?;

      let fields = Fields::checked(&fields)?;
      let mut values = vec![name.into()];

      values.extend(fields.values());
      values.push((now_ms() / 1000).into());
      run(
        self.db,
        "INSERT INTO registered_models (name, label, model, context_tokens, answer_tokens, thinking, walked,
         keeps_research, added_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        values,
      )
      .await?;
    } else {
      let mut values = Fields::checked(&fields)?.values();

      values.push(name.into());
      run(
        self.db,
        "UPDATE registered_models SET label = $1, model = $2, context_tokens = $3, answer_tokens = $4,
         thinking = $5, walked = $6, keeps_research = $7 WHERE name = $8",
        values,
      )
      .await?;
    }

    self.card_service(body, name, held.as_ref()).await?;
    self.languages(body, name).await?;
    self.account(body, name).await
  }

  // The account the registration belongs to, where the change names it: a user's id, or null for the
  // platform's. A change that names none leaves it as it stands, none on a new registration.
  async fn account(&self, body: &Value, name: &str) -> Result<(), Fault> {
    let account = match body.get("account") {
      None => return Ok(()),
      Some(named) => checked_account(named)?,
    };

    run(
      self.db,
      "UPDATE registered_models SET account = $1 WHERE name = $2",
      vec![account.into(), name.into()],
    )
    .await?;

    Ok(())
  }

  // The languages, where the change names them: a list, or an empty list or null for every language. A change
  // that names none leaves them as they stand, none on a new registration.
  async fn languages(&self, body: &Value, name: &str) -> Result<(), Fault> {
    let named = match body.get("languages") {
      None => return Ok(()),
      Some(Value::Null) => vec![],
      Some(Value::Array(named)) => checked_languages(named)?,
      Some(_) => return refused_languages(),
    };
    let kept = (!named.is_empty()).then(|| Value::from(named.clone()).to_string());

    run(
      self.db,
      "UPDATE registered_models SET languages = $1 WHERE name = $2",
      vec![kept.into(), name.into()],
    )
    .await?;

    Ok(())
  }

  // The card service and its instances' role files, each as the change names it and else as it stands,
  // none and off on a new registration. Naming a card service takes away the addresses it held, since
  // where its engine stands is the arbiter's to say (docs/own-llm.md, 「What a registration holds」).
  async fn card_service(
    &mut self,
    body: &Value,
    name: &str,
    held: Option<&Registration>,
  ) -> Result<(), Fault> {
    let service = match body.get("cardService") {
      None => held.and_then(|held| held.card_service.clone()),
      Some(Value::Null) => None,
      Some(Value::String(service)) => checked_card_service(Some(service))?,
      Some(_) => return refused("cardService", CONTROL_WORDS.registry.card_service.as_str()),
    };
    let role_files = match body.get("roleFiles") {
      None | Some(Value::Null) => held.is_some_and(|held| held.role_files),
      Some(Value::Bool(on)) => *on,
      Some(_) => return refused("roleFiles", CONTROL_WORDS.registry.card_role_files.as_str()),
    };

    run(
      self.db,
      "UPDATE registered_models SET card_service = $1, role_files = $2 WHERE name = $3",
      vec![service.clone().into(), i64::from(role_files).into(), name.into()],
    )
    .await?;

    if service.is_some()
      && let Some(held) = held.filter(|held| !held.addresses.is_empty())
    {
      run(
        self.db,
        "DELETE FROM registered_model_addresses WHERE registration = $1",
        vec![name.into()],
      )
      .await?;
      self.gone.extend(held.addresses.iter().cloned());
    }

    Ok(())
  }

  // An address added, or the one of the same node changed in place.
  async fn set_address(
    &self,
    name: &str,
    node: &str,
    url: &str,
    key_file: Option<&str>,
    key_owned: bool,
    role_files: bool,
  ) -> Result<(), Fault> {
    self.required(name).await?;
    checked_node(node)?;

    let url = checked_url(url)?;

    checked_key_file(key_file)?;

    let last = one(
      self.db,
      "SELECT MAX(position) AS position FROM registered_model_addresses WHERE registration = $1",
      vec![name.into()],
    )
    .await?
    .map(|row| row.try_get::<Option<i64>>("", "position"))
    .transpose()?
    .flatten();

    run(
      self.db,
      "INSERT INTO registered_model_addresses (registration, node, position, url, key_file, key_owned, role_files)
       VALUES ($1, $2, $3, $4, $5, $6, $7)
       ON CONFLICT (registration, node) DO UPDATE SET url = excluded.url, key_file = excluded.key_file,
       key_owned = excluded.key_owned, role_files = excluded.role_files",
      vec![
        name.into(),
        node.into(),
        (last.unwrap_or(0) + 1).into(),
        url.into(),
        key_file.map(str::to_owned).into(),
        i64::from(key_file.is_some() && key_owned).into(),
        i64::from(role_files).into(),
      ],
    )
    .await?;

    Ok(())
  }

  async fn address(&mut self, body: &Value, name: &str) -> Result<(), Fault> {
    let held = self.required(name).await?;

    if let Some(service) = &held.card_service {
      return refused(
        "node",
        fill_marks(&CONTROL_WORDS.registry.card_service_address, &[("name", name), ("service", service)]),
      );
    }
    let node = text(body, "node");
    let url = text(body, "url");
    let standing = held.addresses.iter().find(|one| one.node == node).cloned();
    let key = body.get("key");
    // A key file named rather than a key given: a secret the console's store wrote in the platform's space, which
    // agency neither writes nor removes (docs/own-llm.md, 「The keys」).
    let named = match body.get("keyFile") {
      None | Some(Value::Null) => None,
      Some(Value::String(file)) => Some(checked_named_key_file(file)?),
      Some(_) => return refused("keyFile", CONTROL_WORDS.registry.key_file_name.as_str()),
    };

    if key.is_some_and(|key| !key.is_null() && !key.is_string()) {
      return refused("key", CONTROL_WORDS.registry.key_text.as_str());
    }

    if named.is_some() && key.is_some_and(Value::is_string) {
      return refused("keyFile", CONTROL_WORDS.registry.key_both.as_str());
    }

    // Whether the instance keeps role files: as the change names it, else as the address stands, off on a new one.
    let role_files = match body.get("roleFiles") {
      None | Some(Value::Null) => standing.as_ref().is_some_and(|standing| standing.role_files),
      Some(Value::Bool(on)) => *on,
      Some(_) => return refused("roleFiles", CONTROL_WORDS.registry.role_files.as_str()),
    };

    // The address is checked before a key file is written, so a refused address leaves no file
    // behind, and the row is written once.
    checked_node(&node)?;
    checked_url(&url)?;

    if let Some(file) = named {
      self.set_address(name, &node, &url, Some(&file), false, role_files).await?;
      self.gone.extend(standing);

      return Ok(());
    }

    match key {
      Some(Value::String(key)) => {
        let key_file = keys::write(self.data_root, name, &node, key)?;

        self.set_address(name, &node, &url, Some(&key_file), true, role_files).await?;
        self
          .gone
          .extend(standing.filter(|standing| standing.key_file.as_deref() != Some(&key_file)));
      }
      Some(Value::Null) => {
        self.set_address(name, &node, &url, None, false, role_files).await?;
        self.gone.extend(standing);
      }
      _ => {
        let (key_file, owned) =
          standing.map_or((None, false), |standing| (standing.key_file, standing.key_owned));

        self.set_address(name, &node, &url, key_file.as_deref(), owned, role_files).await?;
      }
    }

    Ok(())
  }

  async fn refused_while_named(&self, name: &str, what: &str) -> Result<(), Fault> {
    let naming = referrers(self.db, name).await?;

    if naming.is_empty() {
      return Ok(());
    }

    refused(
      "name",
      fill_marks(&CONTROL_WORDS.registry.in_use, &[("what", what), ("names", &naming.join(", "))]),
    )
  }

  // One address taken out; the last address of a registration something names is refused, since the
  // registration would then be offered nowhere while it is named.
  async fn remove_address(&mut self, name: &str, node: &str) -> Result<(), Fault> {
    if registration(self.db, name).await?.is_some_and(|held| held.addresses.len() == 1) {
      self
        .refused_while_named(name, &fill_marks(&CONTROL_WORDS.registry.last_address, &[("name", name)]))
        .await?;
    }

    let held = self.required(name).await?;
    let Some(address) = held.addresses.into_iter().find(|one| one.node == node) else {
      return refused(
        "node",
        fill_marks(&CONTROL_WORDS.registry.no_address, &[("name", name), ("node", node)]),
      );
    };

    run(
      self.db,
      "DELETE FROM registered_model_addresses WHERE registration = $1 AND node = $2",
      vec![name.into(), node.into()],
    )
    .await?;
    self.gone.push(address);

    Ok(())
  }

  async fn remove(&mut self, name: &str) -> Result<(), Fault> {
    self
      .refused_while_named(name, &fill_marks(&CONTROL_WORDS.registry.the_registration, &[("name", name)]))
      .await?;

    let held = self.required(name).await?;

    run(
      self.db,
      "DELETE FROM registered_model_addresses WHERE registration = $1",
      vec![name.into()],
    )
    .await?;
    run(self.db, "DELETE FROM registered_models WHERE name = $1", vec![name.into()]).await?;
    self.gone.extend(held.addresses);

    Ok(())
  }
}

/// One change made in its transaction, and the key files it left nobody naming taken away once it has
/// committed: done, or the refusal naming its field.
pub async fn made(db: &sea_orm::DatabaseConnection, data_root: &Path, body: &Value) -> Result<(), Fault> {
  let written = db.begin().await?;
  let mut change = Change {
    db: &written,
    data_root,
    gone: vec![],
  };
  let name = text(body, "name");

  match body.get("action").and_then(Value::as_str) {
    Some("save") => change.save(body, &name).await,
    Some("set-address") => change.address(body, &name).await,
    Some("remove-address") => change.remove_address(&name, &text(body, "node")).await,
    Some("remove") => change.remove(&name).await,
    _ => refused("action", CONTROL_WORDS.request.action.as_str()),
  }?;

  let gone = std::mem::take(&mut change.gone);

  written.commit().await?;

  for address in gone {
    keys::remove(data_root, address.key_file.as_deref(), address.key_owned);
  }

  Ok(())
}

/// One change of the registry as a route makes it, from a body of the same shape: done, or the
/// sentence refusing it.
pub async fn change(
  db: &sea_orm::DatabaseConnection,
  data_root: &Path,
  body: &Value,
) -> Result<Result<(), String>, Refused> {
  match made(db, data_root, body).await {
    Ok(()) => Ok(Ok(())),
    Err(Fault::Field(_, error)) => Ok(Err(error)),
    Err(Fault::Failed(failed)) => Err(failed),
  }
}
