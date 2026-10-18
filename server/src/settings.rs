//! The settings: `settings.toml`, overridden by `settings.local.toml` and then by the environment
//! (docs/boundary.md, 「Settings」). Both files are read from the directory the process starts in.

use std::net::SocketAddr;

use config::{Config, Environment, File, FileFormat};
use serde::Deserialize;

/// The settings file every deployment reads.
pub const SETTINGS_FILE: &str = "settings.toml";
/// The file a deployment overrides it with, which git ignores.
pub const LOCAL_SETTINGS_FILE: &str = "settings.local.toml";
/// What every environment variable that sets a key begins with.
pub const ENVIRONMENT_PREFIX: &str = "BMO_WEB_SERVER";

/// Everything the server and its verbs are started with.
#[derive(Clone, Debug, Deserialize)]
pub struct Settings {
  pub server: Server,
  pub database: Database,
  pub sessions: Sessions,
  /// Nothing set here leaves the receiver unknown.
  #[serde(default)]
  pub receiver: Unit,
}

/// The HTTP server.
#[derive(Clone, Debug, Deserialize)]
pub struct Server {
  /// The address the server listens on.
  pub listen: SocketAddr,
}

/// The service's own database.
#[derive(Clone, Debug, Deserialize)]
pub struct Database {
  /// The PostgreSQL address.
  pub url: String,
}

/// Where sumida keeps its sessions.
#[derive(Clone, Debug, Deserialize)]
pub struct Sessions {
  /// The Valkey address.
  pub valkey: String,
}

/// Another unit the server calls: its base address and the token it is called with.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Unit {
  pub base: Option<String>,
  pub token: Option<String>,
}

impl Settings {
  /// The settings as the files and the environment set them, or why they cannot be read.
  pub fn load() -> Result<Self, String> {
    Config::builder()
      .add_source(File::new(SETTINGS_FILE, FileFormat::Toml))
      .add_source(File::new(LOCAL_SETTINGS_FILE, FileFormat::Toml).required(false))
      .add_source(Environment::with_prefix(ENVIRONMENT_PREFIX).prefix_separator("_").separator("__"))
      .build()
      .and_then(Config::try_deserialize)
      .map_err(|why| format!("the settings cannot be read: {why}"))
  }
}
