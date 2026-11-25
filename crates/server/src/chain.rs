//! The account service's delegation chain as agency asks it (the web's `docs/accounts.md`, 「The delegation
//! chain」): every setting agency declares, declared as the server stands and again where a resolution finds one
//! undeclared; a kind resolved for a person or a unit; and a link's entry read or a value written in the name of
//! the person a console call is made for, so the chain itself judges who may write it. Nothing is kept: every
//! consumer asks afresh as it acts.

use std::time::Duration;

use policy::organisation::ORGANISATION;
use policy::prompt_layers::{Kind, PROMPT_LAYERS};
use reqwest::StatusCode;
use serde_json::{Value, json};
use tool_call::account_service::AccountService;

/// Whom a kind is resolved for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
  Person(i64),
  Unit(i64),
}

impl Subject {
  fn query(self) -> (&'static str, String) {
    match self {
      Self::Person(person) => ("person", person.to_string()),
      Self::Unit(unit) => ("unit", unit.to_string()),
    }
  }
}

/// What the account service answers a kind's resolution: what its links said, or that it said nothing — the kind
/// undeclared or the subject not standing there.
enum Said {
  Resolved(Value),
  Undeclared,
  Nothing,
}

/// What the account service answered a call made in a person's name: its status and its body.
#[derive(Clone, Debug, PartialEq)]
pub struct Answered {
  pub status: u16,
  pub body: Value,
}

impl Answered {
  pub fn ok(&self) -> bool {
    (200..300).contains(&self.status)
  }
}

fn client() -> reqwest::Client {
  reqwest::Client::builder()
    .timeout(Duration::from_secs(PROMPT_LAYERS.ask_seconds))
    .build()
    .unwrap_or_default()
}

/// Every kind agency declares: the prompt layers' and the organisation's.
pub fn kinds() -> Vec<Kind> {
  let organisation = &ORGANISATION;
  let own = |kind: &Kind| Kind {
    key: kind.key.clone(),
    description: kind.description.clone(),
    shape: kind.shape.clone(),
    choices: kind.choices.clone(),
    mode: kind.mode.clone(),
    rules: kind.rules,
    levels: kind.levels.clone(),
    unit_permission: kind.unit_permission.clone(),
    person_writer: kind.person_writer.clone(),
    default: kind.default.clone(),
  };

  vec![
    own(&PROMPT_LAYERS.organisation.kind),
    own(&PROMPT_LAYERS.persona.kind),
    own(&PROMPT_LAYERS.personal.allowed),
    own(&PROMPT_LAYERS.personal.text),
    organisation.composition.kind(),
    own(&organisation.repositories),
    own(&organisation.personal_apps.make),
    own(&organisation.personal_apps.network),
    own(&organisation.personal_apps.secrets),
    own(&organisation.personal_apps.share_scope),
    own(&organisation.personal_apps.share_review),
    own(&organisation.personal_apps.install),
    own(&organisation.personal_local_ai),
    own(&organisation.own_agents.add),
    own(&organisation.own_agents.road),
  ]
}

/// The body the account service takes a kind's declaration in.
pub fn declaration(kind: &Kind) -> Value {
  let mut body = json!({
    "key": kind.key,
    "declaredBy": PROMPT_LAYERS.declared_by,
    "description": kind.description,
    "shape": kind.shape,
    "choices": kind.choices,
    "mode": kind.mode,
    "rules": kind.rules,
    "levels": kind.levels,
    "unitPermission": kind.unit_permission,
    "personWriter": kind.person_writer,
  });

  if let Some(default) = &kind.default {
    body["default"] = serde_json::to_value(default).unwrap_or(Value::Null);
  }

  body
}

/// Every kind agency declares, declared on the account service, `PUT /chain/kinds/{key}`; the first refusal or
/// failure is the error. Nothing is asked where no account service is named.
pub async fn declare(service: &AccountService) -> Result<(), String> {
  if service.address.is_empty() {
    return Ok(());
  }

  let client = client();

  for kind in kinds() {
    let answer = client
      .put(format!("{}/chain/kinds/{}", service.address, kind.key))
      .bearer_auth(&service.token)
      .json(&declaration(&kind))
      .send()
      .await
      .map_err(|why| format!("the account service was not reached: {why}"))?;
    let status = answer.status();

    if !status.is_success() {
      let said = answer.text().await.unwrap_or_default();

      return Err(format!(
        "the account service refused the kind {} with {}: {said}",
        kind.key,
        status.as_u16()
      ));
    }
  }

  Ok(())
}

// One kind resolved for a subject.
async fn said(
  client: &reqwest::Client,
  service: &AccountService,
  key: &str,
  subject: Subject,
) -> Result<Said, String> {
  let answer = client
    .get(format!("{}/chain/kinds/{key}/resolve", service.address))
    .bearer_auth(&service.token)
    .query(&[subject.query()])
    .send()
    .await
    .map_err(|why| format!("the account service was not reached: {why}"))?;
  let status = answer.status();

  if status == StatusCode::NOT_FOUND {
    let body: Value = answer.json().await.unwrap_or(Value::Null);

    return Ok(if body["code"] == "no-such-kind" { Said::Undeclared } else { Said::Nothing });
  }

  if !status.is_success() {
    return Err(format!("the account service answered {}", status.as_u16()));
  }

  answer
    .json()
    .await
    .map(Said::Resolved)
    .map_err(|why| format!("the account service's answer does not read: {why}"))
}

/// One kind resolved for a subject, every kind declared again and the kind asked once more where it stood
/// undeclared; a kind that stays undeclared, or a subject the service does not hold, says nothing. Where no
/// account service is named, nothing is said either.
pub async fn resolve(
  service: &AccountService,
  key: &str,
  subject: Subject,
) -> Result<Option<Value>, String> {
  if service.address.is_empty() {
    return Ok(None);
  }

  let client = client();

  match said(&client, service, key, subject).await? {
    Said::Resolved(answer) => Ok(Some(answer)),
    Said::Nothing => Ok(None),
    Said::Undeclared => {
      if let Err(why) = declare(service).await {
        eprintln!("agency: its delegation chain kinds were not declared: {why}");
        return Ok(None);
      }

      match said(&client, service, key, subject).await? {
        Said::Resolved(answer) => Ok(Some(answer)),
        Said::Nothing | Said::Undeclared => Ok(None),
      }
    }
  }
}

// A call made in a person's name, its answer whole.
async fn in_name_of(request: reqwest::RequestBuilder, caller: i64) -> Result<Answered, String> {
  let answer = request
    .header("Owner-Key", caller.to_string())
    .send()
    .await
    .map_err(|why| format!("the account service was not reached: {why}"))?;
  let status = answer.status().as_u16();
  let body = answer.json().await.unwrap_or(Value::Null);

  Ok(Answered { status, body })
}

/// A link's entry of a kind, read in the caller's name: the chain answers it to whoever writes the link and to a
/// person at their own, and refuses anyone else 403.
pub async fn entry(
  service: &AccountService,
  key: &str,
  link: &str,
  caller: i64,
) -> Result<Answered, String> {
  let request = client()
    .get(format!("{}/chain/kinds/{key}/links/{link}", service.address))
    .bearer_auth(&service.token);

  in_name_of(request, caller).await
}

/// A link's value written in the caller's name, null to follow the link above; the chain judges whether the caller
/// writes it, and whether a lock above withholds it. The call names agency as the unit that declared the kind, so
/// the chain takes a `json` value agency judged on its own route.
pub async fn write_value(
  service: &AccountService,
  key: &str,
  link: &str,
  caller: i64,
  value: Value,
) -> Result<Answered, String> {
  let request = client()
    .put(format!("{}/chain/kinds/{key}/links/{link}/value", service.address))
    .bearer_auth(&service.token)
    .header("Declared-By", &PROMPT_LAYERS.declared_by)
    .json(&json!({ "value": value }));

  in_name_of(request, caller).await
}

/// Whether the caller may write the unit's link of a kind now: the chain answers the entry to its writers alone and
/// says whether a lock above withholds it.
pub async fn writes_unit(
  service: &AccountService,
  key: &str,
  unit: i64,
  caller: i64,
) -> Result<bool, String> {
  let read = entry(service, key, &format!("unit:{unit}"), caller).await?;

  Ok(read.ok() && read.body["writable"].as_bool().unwrap_or(false))
}

/// The units on a resolved subject's path, the farthest first, as its answer's layers name them.
pub fn units_of(answer: &Value) -> Vec<i64> {
  answer["layers"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|layer| layer["link"].as_str()?.strip_prefix("unit:")?.parse().ok())
    .collect()
}

/// A resolved flag's value, `fallback` where the answer says none.
pub fn flag(answer: Option<&Value>, fallback: bool) -> bool {
  answer.and_then(|answer| answer["value"].as_bool()).unwrap_or(fallback)
}

/// A link's delegation written in the caller's name, `allow`, `lock` or `inherit`; the chain judges whether the caller
/// writes it.
pub async fn write_delegation(
  service: &AccountService,
  key: &str,
  link: &str,
  caller: i64,
  delegation: &str,
) -> Result<Answered, String> {
  let request = client()
    .put(format!("{}/chain/kinds/{key}/links/{link}/delegation", service.address))
    .bearer_auth(&service.token)
    .json(&json!({ "delegation": delegation }));

  in_name_of(request, caller).await
}

/// A read of the account service at `path`, in the caller's name where one is named: a unit, its subtree or a group
/// of the chain.
pub async fn read(service: &AccountService, path: &str, caller: Option<i64>) -> Result<Answered, String> {
  let request = client().get(format!("{}{path}", service.address)).bearer_auth(&service.token);

  match caller {
    Some(caller) => in_name_of(request, caller).await,
    None => {
      let answer = request
        .send()
        .await
        .map_err(|why| format!("the account service was not reached: {why}"))?;
      let status = answer.status().as_u16();
      let body = answer.json().await.unwrap_or(Value::Null);

      Ok(Answered { status, body })
    }
  }
}

/// The links of a resolved subject's chain, top first, as its answer's path names them.
pub fn path_of(answer: &Value) -> Vec<String> {
  answer["path"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|link| link.as_str().or_else(|| link["link"].as_str()).map(str::to_owned))
    .collect()
}

/// A link's rules written whole in the caller's name, each `who` and `value`.
pub async fn write_rules(
  service: &AccountService,
  key: &str,
  link: &str,
  caller: i64,
  rules: Value,
) -> Result<Answered, String> {
  let request = client()
    .put(format!("{}/chain/kinds/{key}/links/{link}/rules", service.address))
    .bearer_auth(&service.token)
    .json(&json!({ "rules": rules }));

  in_name_of(request, caller).await
}
