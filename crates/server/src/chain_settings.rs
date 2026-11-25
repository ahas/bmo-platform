//! Agency's own settings as they stand on the delegation chain (docs/api.md, 「settings」;
//! docs/prompt-layers.md, 「The settings agency declares」): the research supervisor, the minutes between two
//! flushes of the deploy queue, and the two DeepSeek secret names. The platform's word on each is the value of
//! the platform's link, which the administrator's account writes; the research supervisor stands under units
//! too, and the nearest link of the person a run serves names that run's supervisor. Values that stood in rows
//! of agency's settings table move to the platform's links once, as the server stands; where no account service
//! is named, or none answers, the rows and the policy's defaults stand, so a setting is never lost to a service
//! that is down.

use policy::organisation::ORGANISATION;
use serde_json::Value;

use crate::places::Ground;

/// One setting that stands on the chain: its kind, the settings row it moved off, and whether its value is an
/// amount rather than a text.
pub struct Standing {
  pub key: &'static str,
  pub row: &'static str,
  pub amount: bool,
}

/// The four settings the chain holds, in the order their rows are seeded.
pub fn standing() -> [Standing; 4] {
  let settings = &ORGANISATION.settings;

  [
    Standing {
      key: &settings.research_supervisor.key,
      row: crate::lines::token::RESEARCH_SUPERVISOR,
      amount: false,
    },
    Standing {
      key: &settings.deploy_flush_minutes.key,
      row: crate::watches::deploy_flush::MINUTES_KEY,
      amount: true,
    },
    Standing {
      key: &settings.deepseek_card_secret.key,
      row: "deepseek_card_secret",
      amount: false,
    },
    Standing {
      key: &settings.deepseek_login_secret.key,
      row: "deepseek_login_secret",
      amount: false,
    },
  ]
}

/// The administrator's account, which writes the platform's links; none where the deployment names none.
pub async fn administrator(ground: &Ground) -> Result<Option<i64>, String> {
  crate::account_isolation::administrator(&ground.db)
    .await
    .map(|held| held.and_then(|text| text.trim().parse::<i64>().ok()))
    .map_err(|why| why.to_string())
}

// A link's value that says something: a text with letters in it, a number or a flag.
fn said(value: &Value) -> Option<Value> {
  match value {
    Value::Null => None,
    Value::String(text) if text.trim().is_empty() => None,
    held => Some(held.clone()),
  }
}

/// The platform's word on a standing: the platform link's own value where it says one; none where no account
/// service is named, the deployment names no administrator, or the link says nothing.
pub async fn platform(ground: &Ground, key: &str) -> Result<Option<Value>, String> {
  let service = crate::apps::service(ground);

  if service.address.is_empty() {
    return Ok(None);
  }

  let Some(admin) = administrator(ground).await? else {
    return Ok(None);
  };

  let answered = crate::chain::entry(&service, key, "platform", admin).await?;

  Ok(if answered.ok() { said(&answered.body["value"]) } else { None })
}

/// The word the chain gives the person for a standing that reaches them: the nearest link of their path where
/// it says one, else the platform's word. None is no answer of the chain, which leaves the row and the default
/// to stand.
pub async fn for_person(ground: &Ground, key: &str, person: i64) -> Result<Option<String>, String> {
  let service = crate::apps::service(ground);

  if service.address.is_empty() {
    return Ok(None);
  }

  let answer = crate::chain::resolve(&service, key, crate::chain::Subject::Person(person)).await?;

  Ok(answer.as_ref().and_then(|answer| said(&answer["value"])).map(as_text))
}

// A chain value as the text its consumer keeps.
fn as_text(value: Value) -> String {
  match value {
    Value::String(text) => text,
    Value::Number(number) => number.to_string(),
    Value::Bool(on) => on.to_string(),
    held => held.to_string(),
  }
}

/// The platform's text word on a standing: the chain's platform link where it says one, else the row the value
/// stood under until it moved.
pub async fn text(ground: &Ground, standing: &Standing) -> Result<Option<String>, String> {
  if let Some(value) = platform(ground, standing.key).await? {
    return Ok(Some(as_text(value)));
  }

  crate::queue::settings::get(&ground.db, standing.row)
    .await
    .map(|held| held.filter(|text| !text.trim().is_empty()))
    .map_err(|why| why.to_string())
}

/// The platform's word on a standing, written where it belongs: on the platform's link, in the
/// administrator's name, where an account service is named; into the settings row where none is. Null gives
/// the value back to the kind's default.
pub async fn write(ground: &Ground, standing: &Standing, value: Value) -> Result<(), String> {
  let service = crate::apps::service(ground);

  if service.address.is_empty() {
    return write_row(ground, standing, value).await;
  }

  let Some(admin) = administrator(ground).await? else {
    return Err("the deployment names no administrator, so no one writes the platform's link".to_owned());
  };

  let answered = crate::chain::write_value(&service, standing.key, "platform", admin, value).await?;

  match answered.status {
    200..=299 => {
      // The row of the standing goes with the write, so the older word never shadows the chain's.
      let _ = crate::queue::settings::forget(&ground.db, standing.row).await;

      Ok(())
    }
    404 => Err(format!("the account service does not hold the kind {}", standing.key)),
    409 => Err(format!("a lock withholds the value of {}", standing.key)),
    status => Err(format!("the account service answered the write of {} with {status}", standing.key)),
  }
}

// The settings row of a standing, written or given back.
async fn write_row(ground: &Ground, standing: &Standing, value: Value) -> Result<(), String> {
  let result = match value {
    Value::Null => crate::queue::settings::forget(&ground.db, standing.row).await,
    Value::String(text) => crate::queue::settings::set(&ground.db, standing.row, &text).await,
    Value::Number(number) => {
      crate::queue::settings::set(&ground.db, standing.row, &number.to_string()).await
    }
    _ => return Err("a standing is written as a text, an amount, or null".to_owned()),
  };

  result.map_err(|why| why.to_string())
}

/// The values that stood in rows of agency's settings table moved onto the platform's links once: where the
/// chain is reached and a link holds no word yet, the row's word stands there and the row goes; where the
/// chain already says one, the older row goes; where the chain does not answer, every row stands until a
/// later stand moves it.
pub async fn seed_once(ground: &Ground) -> Result<(), String> {
  let service = crate::apps::service(ground);

  if service.address.is_empty() {
    return Ok(());
  }

  let Some(admin) = administrator(ground).await? else {
    return Ok(());
  };

  for standing in standing() {
    let held = crate::queue::settings::get(&ground.db, standing.row)
      .await
      .map_err(|why| why.to_string())?
      .filter(|text| !text.trim().is_empty());
    let Some(text) = held else { continue };

    let current = platform(ground, standing.key).await;

    if matches!(current, Ok(Some(_))) {
      let _ = crate::queue::settings::forget(&ground.db, standing.row).await;
      continue;
    }

    if current.is_err() {
      continue;
    }

    let value = if standing.amount {
      match text.trim().parse::<i64>() {
        Ok(minutes) => Value::from(minutes),
        Err(_) => continue,
      }
    } else {
      Value::String(text)
    };

    match crate::chain::write_value(&service, standing.key, "platform", admin, value).await {
      Ok(answered) if answered.ok() => {
        let _ = crate::queue::settings::forget(&ground.db, standing.row).await;
      }
      Ok(answered) => eprintln!(
        "agency: the setting {} was not moved onto the chain: it answered {}",
        standing.key, answered.status
      ),
      Err(why) => eprintln!("agency: the setting {} was not moved onto the chain: {why}", standing.key),
    }
  }

  Ok(())
}

/// The standing of one kind, none where the kind is not one the chain holds.
pub fn of(kind: &str) -> Option<Standing> {
  standing().into_iter().find(|one| one.key == kind)
}
