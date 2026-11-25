//! The test of one supply source of a road (docs/supply.md, 「Testing a source」; docs/api.md, 「accounts, rest and
//! usage」): for a person their own supply on the road, for the platform the platform's, answered as whether it
//! answers now and, where it does not, why. Which credential a road's source stands on is its provider's own
//! declaration: a provider that holds logins is tested on its Claude accounts, the registry's on a registration,
//! and a provider whose models are the gateway's members on one request through the gateway. The test reserves
//! nothing and records nothing of its own; the gateway records its one request as it records every request.

use std::time::Duration;

use axum::http::{Method, StatusCode};
use axum::response::Response;
use harness_providers::member::road_members;
use harness_providers::registered::Registered;
use harness_providers::{ARCHIVE_PROVIDERS, Machine, Provider};
use policy::fill_marks;
use policy::supply::{SUPPLY, TestWords};
use serde_json::{Value, json};
use store::one;

use super::claude_accounts::{
  Account, Carrying, Judging, LoginClosed, accounts, administrator, carried_for_user, closed_why,
};
use super::harness::{named, provider_enabled};
use super::{Call, Refused, Route, answer, registry};
use crate::access::Acting;
use crate::now_ms;
use crate::places::Ground;

fn words() -> &'static TestWords {
  &SUPPLY.test.words
}

/// What a test of a source came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
  /// The source answers now.
  Answers,
  /// The owner switched the road's provider off.
  Off,
  /// No credential of the road is on for whom the test acts.
  NoSource,
  /// The registration named holds no address and names no card service.
  NoAddress(String),
  /// Logins are on, and every one waits to be signed in.
  LoginNeeded,
  /// The gateway does not serve the key's member yet: asked again in a few seconds, it may.
  NotReady,
  /// The provider refused the key, with what was said.
  Refused(String),
  /// The test reached no answer, with why.
  Failed(String),
  /// The person's chain forbids a local AI of their own (docs/supply.md, 「A person's local AI」).
  Withheld,
  /// The person holds no PC of their own a local AI is reached from.
  NoPc,
  /// The person's PC, or the address on it, could not be reached, with why.
  Unreachable(String),
}

impl Outcome {
  /// The kebab-case code a caller reads, none where the source answers.
  pub fn code(&self) -> Option<&'static str> {
    match self {
      Self::Answers => None,
      Self::Off => Some("off"),
      Self::NoSource | Self::NoAddress(_) => Some("no-source"),
      Self::LoginNeeded => Some("login-needed"),
      Self::NotReady => Some("not-ready"),
      Self::Refused(_) => Some("refused"),
      Self::Failed(_) => Some("failed"),
      Self::Withheld => Some("withheld"),
      Self::NoPc => Some("no-pc"),
      Self::Unreachable(_) => Some("unreachable"),
    }
  }

  /// The English sentence a person reads, none where the source answers.
  fn reason(&self, road: &str, acting: Acting) -> Option<String> {
    let said = |text: &str, marks: &[(&str, &str)]| {
      let mut marks = marks.to_vec();

      marks.push(("road", road));
      Some(fill_marks(text, &marks))
    };

    match self {
      Self::Answers => None,
      Self::Off => said(&words().off, &[]),
      Self::NoSource => match acting {
        Acting::Person(_) => said(&words().no_source_own, &[]),
        Acting::Platform => said(&words().no_source_platform, &[]),
      },
      Self::NoAddress(model) => said(&words().no_address, &[("model", model)]),
      Self::LoginNeeded => said(&words().login_needed, &[]),
      Self::NotReady => said(&words().not_ready, &[]),
      Self::Refused(why) => said(&words().refused, &[("why", why)]),
      Self::Failed(why) => said(&words().failed, &[("why", why)]),
      Self::Withheld => said(&words().withheld, &[]),
      Self::NoPc => said(&words().no_pc, &[]),
      Self::Unreachable(why) => said(&words().unreachable, &[("why", why)]),
    }
  }
}

/// What the gateway's answer to the test's one request comes to: any success answers; a refusal of the front's own
/// that judges the person's member as none of theirs, or the gateway knowing no such model, is a member the launcher
/// has not laid yet; an authentication or permission refusal, or a 401, 402 or 403, is the provider refusing the key;
/// anything else reached no answer.
pub fn gateway_outcome(status: u16, body: &Value) -> Outcome {
  if (200..300).contains(&status) {
    return Outcome::Answers;
  }

  let error = body.get("error").unwrap_or(body);
  let field = |name: &str| match error.get(name) {
    Some(Value::String(text)) => text.clone(),
    Some(Value::Number(number)) => number.to_string(),
    _ => String::new(),
  };
  let (code, kind) = (field("code"), field("type"));
  let message = match (field("message"), error) {
    (message, _) if !message.is_empty() => message,
    (_, Value::String(text)) => text.trim().to_owned(),
    _ => format!("the gateway answered {status}"),
  };

  // The front judges a name it does not hold as a person's yet by the platform's member it stands beside.
  if code == "not-your-member" || code == "no-supply" {
    return Outcome::NotReady;
  }

  if matches!(status, 400 | 404) && unknown_model(&message) {
    return Outcome::NotReady;
  }

  // The front's own refusal of the key the test sent is no provider's.
  if status == 401 && message.starts_with("The gateway's key") {
    return Outcome::Failed(message);
  }

  if (401..=403).contains(&status) || kind == "authentication_error" || kind == "permission_error" {
    return Outcome::Refused(message);
  }

  Outcome::Failed(format!("the gateway answered {status}: {message}"))
}

// Whether the gateway's words say it holds no model of the name asked.
fn unknown_model(message: &str) -> bool {
  let said = message.to_lowercase();

  said.contains("invalid model")
    || said.contains("model_name")
    || said.contains("model")
      && ["not found", "does not exist", "unknown"].iter().any(|word| said.contains(word))
}

// One request of a single answer token through the gateway, under `key`, naming `model`, waited for no longer than
// `wait`.
async fn asked(machine: &Machine, key: &str, model: &str, wait: Duration) -> Outcome {
  let url = harness_machine::gateway::url(machine.var("BMO_LLM_GATEWAY_URL"));
  let request = json!({
    "model": model,
    "messages": [{ "role": "user", "content": SUPPLY.test.asked }],
    "max_tokens": 1,
  });
  let sent = reqwest::Client::new()
    .post(format!("{url}/v1/chat/completions"))
    .bearer_auth(key)
    .timeout(wait)
    .json(&request)
    .send()
    .await;

  match sent {
    Err(why) => Outcome::Failed(format!("the gateway: {why}")),
    Ok(answered) => {
      let status = answered.status().as_u16();

      match answered.text().await {
        Ok(text) => {
          gateway_outcome(status, &serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text)))
        }
        Err(why) => Outcome::Failed(format!("the gateway's answer: {why}")),
      }
    }
  }
}

// A key a file holds, trimmed, none where it holds none.
fn key_in(path: &std::path::Path) -> Option<String> {
  std::fs::read_to_string(path)
    .ok()
    .map(|key| key.trim().to_owned())
    .filter(|key| !key.is_empty())
}

// The gateway's own key, as the deployment's own callers present it.
fn gateway_key(machine: &Machine) -> Result<String, Outcome> {
  key_in(&Registered::key_file(machine))
    .ok_or_else(|| Outcome::Failed("the gateway's own key could not be read".to_owned()))
}

/// The credential a road's source stands on, as its provider declares it.
enum Stands {
  /// Its own logins, the Claude accounts.
  Logins,
  /// The registry's registrations.
  Registrations,
  /// The gateway's members of the road.
  Members,
}

impl Stands {
  fn of(provider: &dyn Provider) -> Option<Self> {
    if provider.holds_logins() {
      Some(Self::Logins)
    } else if provider.unregistered().is_some() {
      Some(Self::Registrations)
    } else if provider.models_on_gateway() {
      Some(Self::Members)
    } else {
      None
    }
  }
}

// Whether a login stands: signed in and not ended, or an `api-key` account whose key file stands. A full window or a
// rest leaves the credential standing.
fn login_stands(closed: Option<&LoginClosed>) -> bool {
  !matches!(
    closed,
    Some(LoginClosed::Off | LoginClosed::Key | LoginClosed::Person | LoginClosed::NoLogin)
  )
}

// The Claude accounts that are on for whom the test acts: a person's own, or the platform's with those marked
// lendable (docs/supply.md, 「Owned supply」 and 「The lendable accounts」).
async fn logins_on(ground: &Ground, acting: Acting) -> Result<Vec<Account>, Refused> {
  let carrying = match acting {
    Acting::Person(person) => Carrying::Own(person),
    Acting::Platform => Carrying::Platform,
  };
  let mut on = carried_for_user(&ground.db, carrying).await?;

  if acting == Acting::Platform {
    for account in accounts(&ground.db).await? {
      if account.enabled && account.lendable && !on.iter().any(|one| one.id == account.id) {
        on.push(account);
      }
    }
  }

  Ok(on)
}

async fn logins(ground: &Ground, acting: Acting) -> Result<Outcome, Refused> {
  let on = logins_on(ground, acting).await?;

  if on.is_empty() {
    return Ok(Outcome::NoSource);
  }

  let (judging, now) = (Judging::of(ground), now_ms());

  for account in &on {
    if login_stands(closed_why(judging, account, None, now).await?.as_ref()) {
      return Ok(Outcome::Answers);
    }
  }

  Ok(Outcome::LoginNeeded)
}

async fn members(ground: &Ground, acting: Acting, road: &str) -> Result<Outcome, Refused> {
  let machine = ground.machine();
  let wait = Duration::from_secs(SUPPLY.test.gateway_seconds);
  // The members file is read once the source it names is known to be wanted.
  let held = || road_members(&machine, road).map_err(Outcome::Failed);

  match acting {
    Acting::Person(person) => {
      let keyed = one(
        &ground.db,
        r#"SELECT 1 AS "open" FROM "personal_supply"
           WHERE "user_id" = $1 AND "road" = $2 AND "enabled" AND "refused_at" IS NULL LIMIT 1"#,
        vec![person.into(), road.into()],
      )
      .await?
      .is_some();

      if !keyed {
        return Ok(Outcome::NoSource);
      }

      let held = match held() {
        Ok(held) => held,
        Err(failed) => return Ok(failed),
      };
      let Some(first) = held.first() else {
        return Ok(Outcome::Failed(format!("no member of the gateway serves {road}")));
      };
      let at = crate::call_keys::ensure(&ground.db, &ground.roots.data, person).await?;
      let Some(key) = key_in(&at) else {
        return Ok(Outcome::Failed("the person's call key could not be read".to_owned()));
      };

      // The launcher lays the person's key beside each member of the road (docs/own-llm.md, 「Personal members」).
      Ok(asked(&machine, &key, &format!("{}@u{person}", first.member.name), wait).await)
    }
    Acting::Platform => {
      let administrator = administrator(&ground.db).await?;
      let held = match held() {
        Ok(held) => held,
        Err(failed) => return Ok(failed),
      };
      let Some(member) = held
        .iter()
        .find(|one| one.member.account.is_none() || one.member.account == administrator)
      else {
        return Ok(Outcome::NoSource);
      };

      Ok(match gateway_key(&machine) {
        Ok(key) => asked(&machine, &key, &member.member.name, wait).await,
        Err(failed) => failed,
      })
    }
  }
}

// The registration a platform's test on the registry names, which the request must name and the registry hold.
async fn named_registration(
  ground: &Ground,
  model: Option<&str>,
) -> Result<registry::Registration, Refused> {
  let model = model.ok_or_else(|| Refused::failed(&words().model_needed).with_field("model"))?;
  let (_, name) = super::walk_reads::model_key(model, None);

  registry::registration(&ground.db, &name).await?.ok_or_else(|| {
    Refused::failed(fill_marks(&words().unknown_model, &[("model", model)])).with_field("model")
  })
}

// The platform's registration, by one request under the gateway's own key; a person brings none on the registry.
async fn registration(ground: &Ground, held: Option<registry::Registration>) -> Outcome {
  let Some(held) = held else {
    return Outcome::NoSource;
  };

  if !held.offered() {
    return Outcome::NoAddress(held.name);
  }

  let machine = ground.machine();

  match gateway_key(&machine) {
    Ok(key) => asked(&machine, &key, &held.name, Duration::from_secs(SUPPLY.test.registered_seconds)).await,
    Err(failed) => failed,
  }
}

// What a person's test on the registry tries (docs/supply.md, 「Testing a source」): the local AI of theirs `model`
// names, or the one `local` describes before it is saved — its address, its model and its key file — and none where
// they name neither, or name a registration that is no local AI of theirs.
async fn local_tried(
  ground: &Ground,
  person: i64,
  model: Option<&str>,
  local: Option<&Value>,
) -> Result<Option<(String, String, Option<String>)>, Refused> {
  if let Some(local) = local.filter(|local| !local.is_null()) {
    let text =
      |name: &str| local.get(name).and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty());
    let (Some(url), Some(name)) = (text("url"), text("modelName")) else {
      return Err(Refused::failed(&SUPPLY.local_ai.words.candidate).with_field("local"));
    };
    let url = super::personal_models::checked_url(url)?;
    let key = match text("keyName") {
      None => None,
      Some(secret) => {
        Some(crate::local_ai::key_file(person, &super::personal_models::checked_secret(secret)?))
      }
    };

    return Ok(Some((url, name.to_owned(), key)));
  }

  let Some(model) = model else { return Ok(None) };
  let (_, name) = super::walk_reads::model_key(model, None);
  let held = crate::local_ai::of_person(&ground.db, person)
    .await?
    .into_iter()
    .find(|one| one.registration == name);
  // A registration that is no local AI of theirs is none they bring.
  Ok(held.map(|held| (held.url, held.model, held.key_file)))
}

/// `POST /supply-test`: whether a source of a road answers now, the person's own for a person's call and the
/// platform's for the administrator's and the deployment's own.
pub struct SupplyTest;

impl Route for SupplyTest {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/supply-test";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let acting = Acting::of(&call)?;
    let ground = call.shared_ground();
    let body = call.body().await?;
    let text = |name: &str| {
      body
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
    };
    let road = text("road").unwrap_or_default();
    let model = text("model");
    let unknown_road = || {
      Refused::failed(fill_marks(&words().road, &[("roads", &ARCHIVE_PROVIDERS.join(", "))]))
        .with_field("road")
    };
    let stands = named(&road).ok().and_then(Stands::of).ok_or_else(unknown_road)?;

    // The request is read whole before the switch is: a test naming what the registry does not hold is refused
    // as it is.
    let held = match (&stands, acting) {
      (Stands::Registrations, Acting::Platform) => {
        Some(named_registration(&ground, model.as_deref()).await?)
      }
      _ => None,
    };
    let local = match (&stands, acting) {
      (Stands::Registrations, Acting::Person(person)) => {
        Some(local_tried(&ground, person, model.as_deref(), body.get("local")).await?)
      }
      _ => None,
    };
    let outcome = if !provider_enabled(&ground.db, &road).await? {
      Outcome::Off
    } else {
      match (stands, acting, local) {
        (Stands::Logins, ..) => logins(&ground, acting).await?,
        (Stands::Members, ..) => members(&ground, acting, &road).await?,
        (Stands::Registrations, Acting::Person(person), Some(Some(tried))) => {
          let tried = crate::local_ai::Tried {
            url: &tried.0,
            model: &tried.1,
            key_file: tried.2.as_deref(),
          };

          crate::local_ai::test(&ground, person, &tried).await?
        }
        (Stands::Registrations, Acting::Person(_), _) => Outcome::NoSource,
        (Stands::Registrations, ..) => registration(&ground, held).await,
      }
    };

    Ok(answer(
      StatusCode::OK,
      json!({
        "road": road,
        "ok": outcome == Outcome::Answers,
        "reasonCode": outcome.code(),
        "reason": outcome.reason(&road, acting),
      }),
    ))
  }
}

crate::moved!(SupplyTest);
