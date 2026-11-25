//! What the places write where a person reads it (docs/agent-control.md, 「One standing pane per agent」,
//! 「A worker standing with nothing to do」): the labels herdr shows a place under, and the notes a worker's
//! log is given when its pane's program is replaced for its pin.

use serde::Deserialize;

/// The notes a worker's log is given when its pane's program is replaced for its pin.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Standby {
  pub unknown_provider: String,
  pub replaced: String,
  pub replaced_on: String,
  pub released: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceWords {
  pub copy_label: String,
  pub dev_tab: String,
  pub coworker_place: String,
  pub standby: Standby,
}

crate::part!(pub PLACE_WORDS: PlaceWords = "place-words.toml");
