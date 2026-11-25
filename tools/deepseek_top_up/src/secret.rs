//! A secret of the console's store, read for the fields a kind holds (docs/tools.md, 「Its secrets」). Its values
//! stay in this process's memory: nothing here formats, logs or answers one.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

/// Why a secret does not serve.
#[derive(Debug, PartialEq, Eq)]
pub enum SecretFault {
  Missing,
  /// The field keys it holds no value for.
  Lacks(Vec<String>),
}

/// A secret's values by field key, every field the kind asks for filled. Its `Debug` names the keys alone.
pub struct Saved {
  values: BTreeMap<String, String>,
}

impl Saved {
  /// The fields of `wanted` from the secret at `path`, or the fields it holds no value for, or that it stands
  /// nowhere.
  ///
  /// The console's secret store writes a secret of several fields as one `KEY=VALUE` line to each field it holds a
  /// value for, the value as typed to the line's end, unquoted and unescaped (bmo-web's `docs/accounts.md`,
  /// 「Secret kinds」). That is the store's own shape, not a dotenv file: a dotenv reader refuses a value holding a
  /// space, as a holder's name does, and reads `$` and `#` as marks. So a line is split at its first `=`, and a
  /// value is kept as typed but for the line's end.
  pub fn read(path: &Path, wanted: &[String]) -> Result<Self, SecretFault> {
    let text = std::fs::read_to_string(path).map_err(|_| SecretFault::Missing)?;
    let filled: BTreeMap<&str, &str> = text
      .lines()
      .filter_map(|line| line.split_once('='))
      .filter(|(_, value)| !value.trim().is_empty())
      .map(|(key, value)| (key.trim(), value.trim_end_matches('\r')))
      .collect();
    let lacking: Vec<String> =
      wanted.iter().filter(|key| !filled.contains_key(key.as_str())).cloned().collect();

    if !lacking.is_empty() {
      return Err(SecretFault::Lacks(lacking));
    }

    let values = wanted.iter().map(|key| (key.clone(), filled[key.as_str()].to_owned())).collect();

    Ok(Self { values })
  }

  /// The value of a field the kind asks for; empty for a key it does not.
  pub fn value(&self, key: &str) -> &str {
    self.values.get(key).map_or("", String::as_str)
  }
}

impl fmt::Debug for Saved {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("Saved")
      .field("keys", &self.values.keys().collect::<Vec<_>>())
      .finish()
  }
}
