//! What a borrowed checkout writes down as a job puts it on its branch (docs/worker-slots.md), with the
//! `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Borrow {
  pub parked: String,
}

crate::part!(pub BORROW: Borrow = "borrow");
