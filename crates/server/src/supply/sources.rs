//! The sources a run walks (docs/supply.md, 「The sources a run walks」): the order the supply hook answers, which
//! names them — the person's own, their organisation's and the platform's — and which agency walks as it stands. What
//! agency reads here is the units of the person's path, the nearest first, read off the delegation chain's answer
//! for the person, read afresh for every run: organisation supply goes out on them, and so a run is read for the
//! seats of those units it may take (docs/role-agents.md, 「A unit's seats run their people's work」).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use policy::organisation::ORGANISATION;
use serde_json::Value;

use super::organisation_word;
use crate::chain::{self, Subject};
use crate::places::Ground;

// The last path this process read for each account, which a run stands on while the account service does not
// answer.
static LAST_READ: LazyLock<Mutex<HashMap<i64, Vec<i64>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Whether organisation supply stands open on a resolved answer of the account service's `supply.organisation`:
/// the platform's own word says it is open for the subject — its value, or a rule naming them, their group or a unit
/// on their path — and no link below closes it.
pub fn organisation_open(answer: Option<&Value>) -> bool {
  let Some(answer) = answer else { return false };
  let platform_says = answer["layers"]
    .as_array()
    .into_iter()
    .flatten()
    .any(|layer| layer["link"] == "platform" && layer["value"] == true);

  platform_says && chain::flag(Some(answer), false)
}

/// The units of a resolved subject's path, the nearest first.
pub fn path_units(answer: Option<&Value>) -> Vec<i64> {
  let mut units: Vec<i64> = answer
    .and_then(|answer| answer["path"].as_array())
    .into_iter()
    .flatten()
    .filter_map(|link| link.as_str()?.strip_prefix("unit:")?.parse().ok())
    .collect();

  units.reverse();
  units
}

/// The units whose own supply a person's run may take, where the order names organisation supply: the units of their
/// path, the nearest first; none where it does not.
pub async fn units_for(ground: &Ground, account: i64, order: &[String]) -> Vec<i64> {
  if !order.iter().any(|source| source == organisation_word()) {
    return Vec::new();
  }

  path_for(ground, account).await
}

/// The units of a person's path, the nearest first, as the delegation chain answers them for them now; where the
/// account service does not answer, the path this process last read for the person stands, else none. The supply
/// walk takes its units from this reading, and so does the line of the seats a run may take, which asks for the
/// path itself and not for the supply flag's value.
pub async fn path_for(ground: &Ground, account: i64) -> Vec<i64> {
  let service = crate::apps::service(ground);

  match chain::resolve(&service, &ORGANISATION.sources.organisation_kind, Subject::Person(account)).await {
    Ok(answer) => {
      let units = path_units(answer.as_ref());

      if let Ok(mut kept) = LAST_READ.lock() {
        kept.insert(account, units.clone());
      }

      units
    }
    Err(why) => {
      eprintln!("agency: the path of account {account} was not read: {why}");
      LAST_READ.lock().ok().and_then(|kept| kept.get(&account).cloned()).unwrap_or_default()
    }
  }
}
