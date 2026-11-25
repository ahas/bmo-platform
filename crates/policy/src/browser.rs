//! How every Chrome agency starts stands apart from a person's session.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Browser {
  /// The arguments every Chrome agency starts carries.
  pub detached_arguments: Vec<String>,
  /// The variables of a person's graphical session, none of which reaches it.
  pub session_variables: Vec<String>,
}

crate::part!(pub BROWSER: Browser = "browser.toml");
