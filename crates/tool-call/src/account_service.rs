//! The account service as agency's code reaches it (docs/api.md, 「Calls agency makes」): its base address and its
//! token, read from the process's environment or from the web unit's env file under the data root.

use std::collections::BTreeMap;
use std::path::Path;

/// The variables naming the account service's base address and its bearer token, and the env file under the data
/// root the web unit writes them into.
pub const ADDRESS_VARIABLE: &str = "WEB_ADDRESS";
pub const TOKEN_VARIABLE: &str = "WEB_TOKEN";
pub const ENV_FILE: &str = "env/web.env";

/// The account service as a caller reaches it: its base address and its token.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountService {
  pub address: String,
  pub token: String,
}

impl AccountService {
  /// The account service a data root names: each variable `variable` answers standing first, else the web unit's
  /// env file; the token by value or in the file `WEB_TOKEN_FILE` names.
  pub fn read(data_root: &Path, variable: impl Fn(&str) -> Option<String>) -> Self {
    let file = env_file(&data_root.join(ENV_FILE));
    let named = |name: &str| {
      variable(name)
        .or_else(|| file.get(name).cloned())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    };
    let token = named(TOKEN_VARIABLE).or_else(|| {
      named(&format!("{TOKEN_VARIABLE}_FILE"))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
    });

    Self {
      address: named(ADDRESS_VARIABLE).unwrap_or_default().trim_end_matches('/').to_owned(),
      token: token.unwrap_or_default(),
    }
  }
}

// An env file's `NAME=value` lines.
fn env_file(path: &Path) -> BTreeMap<String, String> {
  std::fs::read_to_string(path)
    .unwrap_or_default()
    .lines()
    .filter(|line| !line.trim_start().starts_with('#'))
    .filter_map(|line| {
      let (name, value) = line.split_once('=')?;

      Some((name.trim().to_owned(), value.trim().trim_matches('"').to_owned()))
    })
    .collect()
}
