//! What a verb of the `bmo` binary says where it puts something into a room (`lang/<language>/sends.yaml`),
//! each line with the `{name}` marks its file names.

use indexmap::IndexMap;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sends {
  /// What each chat verb is, by the verb's name.
  pub capabilities: IndexMap<String, String>,
  pub verb_refused: String,
  pub another_agents_room: String,
  pub role_without_sends: String,
  pub sticker_list: String,
  pub no_picture: String,
  pub say_title: String,
  pub say_log: SayLog,
}

/// The lines a line's run writes into its log.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SayLog {
  pub started: String,
  pub sent: String,
  pub relay_dropped: String,
}

crate::part!(pub SENDS: Sends = "sends");
