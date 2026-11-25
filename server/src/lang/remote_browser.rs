//! A remote browser run as a person watching herdr reads it (docs/remote-browser.md), each line with the
//! `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteBrowser {
  pub run_title: String,
}

crate::part!(pub REMOTE_BROWSER: RemoteBrowser = "remote-browser");
