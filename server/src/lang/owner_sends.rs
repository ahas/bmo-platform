//! What the console's sends in the owner's own name answer where a person reads them
//! (`lang/<language>/owner-sends.yaml`), each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSends {
  pub reply_missing: String,
  pub words_only: String,
  pub empty_file: String,
  pub too_large: String,
  pub megabytes: String,
}

crate::part!(pub OWNER_SENDS: OwnerSends = "owner-sends");
