//! The parts of a request's session record agency's tools read, and the turn the request is made in, read
//! afresh from it (docs/tools.md, 「A tool's folder」). The record is the tool host's; the issuer's own
//! values stand under `labels`, and the data root under `variables`, which every call's environment reads.
//! A field it holds that no tool of agency's reads is passed over.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;

/// The variable naming the data root the record's tools read.
pub const DATA_ROOT_VARIABLE: &str = "BMO_DATA_DIR";

/// The turn variable naming the agent the turn is.
pub const AGENT_VARIABLE: &str = "BMO_TURN_AGENT";

/// The turn variable carrying the moment the turn took its words, in epoch milliseconds.
pub const BEGAN_VARIABLE: &str = "BMO_TURN_BEGAN";

/// The turn variable naming the room the turn answers.
pub const CHAT_VARIABLE: &str = "BMO_TURN_CHAT";

/// The turn variable naming the road the turn answers its room on.
pub const VIA_VARIABLE: &str = "BMO_TURN_VIA";

/// The issuer's own values, which the host hands on and never reads, as agency's tools read them.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Labels {
  /// The line the run goes out on.
  #[serde(default)]
  pub line: String,
  /// The store's address.
  #[serde(default)]
  pub archive: String,
  /// A resident program's place, where each request reads which run is going.
  #[serde(default)]
  pub place: Option<PathBuf>,
}

/// One session's record, its field names camel case as the tool host writes them.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
  /// Who holds the session.
  #[serde(default)]
  pub holder_name: String,
  /// The namespaces the session may use.
  #[serde(default)]
  pub namespaces: BTreeSet<String>,
  #[serde(default)]
  pub capabilities: BTreeSet<String>,
  /// The variables handed to every call's environment.
  #[serde(default)]
  pub variables: BTreeMap<String, String>,
  /// The issuer's own values.
  #[serde(default)]
  pub labels: Labels,
  /// The account the session acts for, whose personal apps it holds (docs/tools.md, 「The owner and the runner a
  /// record names」); none for the deployment's own callers.
  #[serde(default)]
  pub owner_account: Option<String>,
}

impl Session {
  /// The line the run goes out on.
  pub fn line(&self) -> &str {
    &self.labels.line
  }

  /// The data root the run reads, as its variables name it.
  pub fn data_root(&self) -> PathBuf {
    self.variables.get(DATA_ROOT_VARIABLE).map_or_else(PathBuf::new, PathBuf::from)
  }

  /// The store's address.
  pub fn archive(&self) -> &str {
    &self.labels.archive
  }

  /// One variable of the turn the request is made in: the run the resident program's place names — none for a
  /// person's turn — standing over the run's own variables.
  pub fn turn_variable(&self, name: &str) -> Option<String> {
    let from_place = self.labels.place.as_ref().and_then(|place| {
      let run: Value = serde_json::from_str(&std::fs::read_to_string(place.join("run.json")).ok()?).ok()?;

      if run.get("person").and_then(Value::as_bool) == Some(true) {
        return None;
      }

      run.get("env")?.get(name)?.as_str().map(str::to_owned)
    });

    from_place.or_else(|| self.variables.get(name).cloned())
  }
}

/// What a request reads of its turn: the agent it is, the moment it took its words, the room it answers and the
/// road it answers on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Turn {
  pub agent: Option<String>,
  /// In epoch milliseconds.
  pub began: Option<i64>,
  pub chat: Option<i64>,
  pub via: Option<String>,
}

impl Turn {
  /// The turn the record's request is made in.
  pub fn of(session: &Session) -> Self {
    let named = |name: &str| {
      session
        .turn_variable(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    };

    Self {
      agent: named(AGENT_VARIABLE),
      began: named(BEGAN_VARIABLE).and_then(|at| at.parse().ok()),
      chat: named(CHAT_VARIABLE).and_then(|chat| chat.parse().ok()),
      via: named(VIA_VARIABLE),
    }
  }
}
