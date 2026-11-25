//! Which port the console's remote desktop relay dials (docs/remote.md, 「Where the desktop is」), as
//! `bmo node remote-desktop` tells it, each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteDesktop {
  pub dials: String,
  pub unset: String,
  pub not_a_port: String,
  pub refused: String,
}

crate::part!(pub REMOTE_DESKTOP: RemoteDesktop = "remote-desktop");
