//! The base persona a chat launch's system prompt ends with (docs/sessions.md, 「Document reload」): the base
//! layer the agent's brain renders, its placeholders filled from the deployment's values file (docs/api.md,
//! 「Calls agency makes」), else the base persona the turn handed in. Nothing here fails a launch: where the
//! brain's persona is not used, one sentence says why for the run's log.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::LazyLock;

use policy::chat_line::CHAT_LINE;
use regex::{Captures, Regex};

use crate::config::PERSONA_VALUES;
use crate::peers::brain::{Brain, Rendered};

// A placeholder as a persona piece writes it: two opening braces, a key of lowercase ASCII letters, digits and
// hyphens joined by dots, and two closing braces.
static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"\{\{([a-z0-9-]+(?:\.[a-z0-9-]+)*)\}\}").expect("the placeholder pattern compiles")
});

/// The base persona a launch ends its system prompt with, and why the brain's was not used where it was not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BasePersona {
  /// The text under the persona's header, none where neither the brain nor the turn gave one.
  pub text: Option<String>,
  /// Why the brain's persona was not used, one sentence; none where it was.
  pub unused: Option<String>,
}

/// The base persona of the agent's launch: the base layer its brain renders with every placeholder filled
/// from the values file named, else `handed`, with why the brain's was not used.
pub async fn base_persona(
  brain: &dyn Brain,
  agent: &str,
  values: Option<&Path>,
  handed: Option<&str>,
) -> BasePersona {
  let why = match brain.persona(agent, &CHAT_LINE.persona.base_layer).await {
    Ok(Rendered::Text(text)) if text.trim().is_empty() => {
      format!("the brain renders its {} layer empty", CHAT_LINE.persona.base_layer)
    }
    Ok(Rendered::Text(text)) => match filled(&text, &values_of(values)) {
      Ok(text) => {
        return BasePersona {
          text: Some(text),
          unused: None,
        };
      }
      Err(why) => why,
    },
    Ok(Rendered::NoBrain) => "the agent has no brain".to_owned(),
    Ok(Rendered::Undeployed) => "no brain is deployed".to_owned(),
    Err(why) => why.to_string(),
  };
  let handed = handed.map(str::trim).filter(|text| !text.is_empty());
  let unused = match handed {
    Some(_) => format!("The base persona is the one the turn handed in, since {why}."),
    None => format!("No base persona follows the system prompt, since {why} and the turn handed in none."),
  };

  BasePersona {
    text: handed.map(str::to_owned),
    unused: Some(unused),
  }
}

/// The values the deployment keeps for the persona's placeholders: the file named, read as TOML, or why
/// there are none.
pub fn values_of(path: Option<&Path>) -> Result<toml::Table, String> {
  let path = path.ok_or_else(|| format!("{PERSONA_VALUES} names no values file"))?;
  let text = std::fs::read_to_string(path)
    .map_err(|why| format!("the values file {} could not be read ({why})", path.display()))?;

  text
    .parse::<toml::Table>()
    .map_err(|why| format!("the values file {} is not TOML ({})", path.display(), why.message()))
}

/// `text` with every placeholder filled from `values`, or why one stays unfilled.
pub fn filled(text: &str, values: &Result<toml::Table, String>) -> Result<String, String> {
  let keys: BTreeSet<&str> = PLACEHOLDER
    .captures_iter(text)
    .filter_map(|found| found.get(1))
    .map(|key| key.as_str())
    .collect();

  if keys.is_empty() {
    return Ok(text.to_owned());
  }

  let table = values.as_ref().map_err(|why| format!("{why}, so its placeholders stay unfilled"))?;
  let missing: Vec<&str> = keys.iter().copied().filter(|key| value_of(table, key).is_none()).collect();

  if !missing.is_empty() {
    return Err(format!("the values file holds no value for {}", missing.join(", ")));
  }

  Ok(
    PLACEHOLDER
      .replace_all(text, |found: &Captures| value_of(table, &found[1]).unwrap_or_default().to_owned())
      .into_owned(),
  )
}

// The string a key names, its dots walking the file's tables.
fn value_of<'a>(table: &'a toml::Table, key: &str) -> Option<&'a str> {
  let mut parts = key.split('.');
  let mut value = table.get(parts.next()?)?;

  for part in parts {
    value = value.as_table()?.get(part)?;
  }

  value.as_str()
}
