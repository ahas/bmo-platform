//! Agency's server as a placed tool reaches it (docs/tools.md, 「The procedures」): the port and the token the
//! tool's own environment names, else those the data root's `env/agency.env` names, the file the server's unit
//! takes its environment from. The tool host hands a placed tool a clean environment, so the file is what a
//! deployment's tool reads.

use std::path::Path;

const DEFAULT_PORT: u16 = 3220;
const PORT: &str = "AGENCY_CONTROL_PORT";
const TOKEN: &str = "AGENCY_CONTROL_TOKEN";

/// Where agency's control routes answer, and the token they are asked with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
  pub port: u16,
  pub token: Option<String>,
}

impl Control {
  /// The server the data root's `env/agency.env` describes, each variable `variable` answers standing first.
  pub fn read(data_root: &Path, variable: impl Fn(&str) -> Option<String>) -> Self {
    let file: Vec<(String, String)> = dotenvy::from_path_iter(data_root.join("env").join("agency.env"))
      .map(|pairs| pairs.filter_map(Result::ok).collect())
      .unwrap_or_default();
    let named = |name: &str| {
      variable(name)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| file.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone()))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    };

    Self {
      port: named(PORT).and_then(|port| port.parse().ok()).unwrap_or(DEFAULT_PORT),
      // The token by value, else the file its `_FILE` variable names.
      token: named(TOKEN).or_else(|| {
        named(&format!("{TOKEN}_FILE"))
          .and_then(|file| std::fs::read_to_string(file).ok())
          .map(|token| token.trim().to_owned())
          .filter(|token| !token.is_empty())
      }),
    }
  }

  /// The base address the control routes stand at.
  pub fn base(&self) -> String {
    format!("http://127.0.0.1:{}", self.port)
  }
}
