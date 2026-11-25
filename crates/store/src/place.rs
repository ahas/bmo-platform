//! Where agency's store is (docs/store.md, 「Connecting」): the address one variable names, read the same
//! way by the server, the command and the model gateway, so every one of them reads and writes one store.

use std::path::Path;

/// The variable the store's address stands in.
pub const VARIABLE: &str = "BMO_ARCHIVE_URL";

/// The server's environment file under the data root, which names the store the server runs on.
pub const SERVICE_ENV: &str = "env/agency.env";

/// The file an installation without the server's environment file names the store in.
pub const ADDRESS_FILE: &str = "env/archive-url";

/// The store's address: the one the process's own `BMO_ARCHIVE_URL` names, else the one the server's
/// environment file names, else the one `env/archive-url` holds; none where nothing names one.
pub fn place(data_root: &Path) -> Option<String> {
  std::env::var(VARIABLE)
    .ok()
    .and_then(held)
    .or_else(|| service_named(data_root))
    .or_else(|| std::fs::read_to_string(data_root.join(ADDRESS_FILE)).ok().and_then(held))
}

// The address the server's environment file names, read as the service manager reads it.
fn service_named(data_root: &Path) -> Option<String> {
  dotenvy::from_path_iter(data_root.join(SERVICE_ENV))
    .ok()?
    .filter_map(Result::ok)
    .find(|(name, _)| name == VARIABLE)
    .and_then(|(_, value)| held(value))
}

fn held(text: String) -> Option<String> {
  Some(text.trim().to_owned()).filter(|text| !text.is_empty())
}
