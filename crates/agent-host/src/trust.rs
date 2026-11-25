//! A folder trusted in a login's configuration, the way the CLI itself keeps it (docs/agent-control.md,
//! 「A first interactive run stops at the trust-this-folder check」): the folder's entry in the login's
//! global configuration file, so an interactive program never stops on the question, and the CLI's plugin
//! recommendation declined in the same file (「A turn's end stops at a plugin recommendation」).
//!
//! The file is the CLI's own, so it is changed by that one entry and written back whole in its own key
//! order, under the CLI's own lock — a directory beside it.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use indexmap::IndexMap;
use serde_json::Value;
use serde_json::value::RawValue;

const CONFIG_NAME: &str = ".claude.json";
const TRUST_KEY: &str = "hasTrustDialogAccepted";
const ONBOARDED_KEY: &str = "hasCompletedOnboarding";
// The CLI's own top-level mark declining its plugin recommendation for every session of the login
// (docs/agent-control.md, 「A turn's end stops at a plugin recommendation」).
const DECLINED_KEY: &str = "lspRecommendationDisabled";

// How old a lock may be before it is taken as one its holder left behind: the CLI's holder touches its
// lock every few seconds while it holds it, so an untouched one is a holder gone, not a slow one.
const LOCK_STALE: Duration = Duration::from_secs(10);
// How often a held lock is looked at again.
const LOCK_LOOK: Duration = Duration::from_millis(25);

/// How a folder stands in one configuration file once asked to be trusted there.
#[derive(Debug, PartialEq, Eq)]
pub enum Trust {
  Trusted,
  Written,
  NoConfigDir,
  Unreadable,
}

impl Trust {
  /// The word a shell reading the outcome is told it by.
  pub fn word(&self) -> &'static str {
    match self {
      Self::Trusted => "trusted",
      Self::Written => "written",
      Self::NoConfigDir => "no-config-dir",
      Self::Unreadable => "unreadable",
    }
  }

  /// Whether the folder stands trusted in the file now.
  pub fn holds(&self) -> bool {
    matches!(self, Self::Trusted | Self::Written)
  }
}

// The CLI's lock taken: made once nobody holds it, waited for until its holder lets it go, and a holder's
// left behind — untouched past its age — taken away.
fn take_lock(lock: &Path) {
  loop {
    if std::fs::create_dir(lock).is_ok() {
      return;
    }

    let stale = std::fs::metadata(lock)
      .and_then(|meta| meta.modified())
      .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age > LOCK_STALE));

    if stale {
      let _ = std::fs::remove_dir(lock);
      continue;
    }

    std::thread::sleep(LOCK_LOOK);
  }
}

fn entry_trusts(entry: Option<&Value>) -> bool {
  entry.and_then(|entry| entry.get(TRUST_KEY)) == Some(&Value::Bool(true))
}

fn write_trusted(file: &Path, folder: &str) -> Trust {
  let present = file.exists();
  let text = if present {
    match std::fs::read_to_string(file) {
      Ok(text) => text,
      Err(_) => return Trust::Unreadable,
    }
  } else {
    "{}".into()
  };
  let Ok(mut config) = serde_json::from_str::<IndexMap<String, Box<RawValue>>>(&text) else {
    return Trust::Unreadable;
  };
  let mut projects: IndexMap<String, Value> = config
    .get("projects")
    .and_then(|projects| serde_json::from_str(projects.get()).ok())
    .unwrap_or_default();

  let declined = config.get(DECLINED_KEY).is_some_and(|declined| declined.get() == "true");
  let trusted = entry_trusts(projects.get(folder));

  if trusted && declined {
    return Trust::Trusted;
  }

  if !trusted {
    let mut entry = match projects.get(folder) {
      Some(Value::Object(kept)) => kept.clone(),
      _ => serde_json::Map::new(),
    };

    entry.insert(TRUST_KEY.into(), Value::Bool(true));
    projects.insert(folder.to_owned(), Value::Object(entry));

    let Ok(projects) = serde_json::value::to_raw_value(&projects) else {
      return Trust::Unreadable;
    };

    config.insert("projects".into(), projects);
  }

  if !declined {
    let Ok(yes) = RawValue::from_string("true".to_owned()) else {
      return Trust::Unreadable;
    };

    config.insert(DECLINED_KEY.into(), yes);
  }

  let Ok(body) = serde_json::to_string_pretty(&config) else { return Trust::Unreadable };
  let mode = if present {
    std::fs::metadata(file).map_or(0o600, |meta| meta.permissions().mode() & 0o777)
  } else {
    0o600
  };
  let fresh = PathBuf::from(format!("{}.trust-{}", file.display(), std::process::id()));
  let written = std::fs::write(&fresh, format!("{body}\n"))
    .and_then(|()| std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(mode)))
    .and_then(|()| std::fs::rename(&fresh, file));

  if written.is_err() {
    let _ = std::fs::remove_file(&fresh);

    return Trust::Unreadable;
  }

  Trust::Written
}

/// The folder written as trusted in the file and the plugin recommendation declined there, every other
/// entry kept as it stood. Nothing is written where the folder is already trusted and the recommendation
/// already declined, where the configuration directory is not there — no login
/// lives in it — or where the file is there and is not the CLI's configuration.
pub fn trust_folder(file: &Path, folder: &str) -> Trust {
  if !file.parent().is_some_and(Path::exists) {
    return Trust::NoConfigDir;
  }

  let lock = PathBuf::from(format!("{}.lock", file.display()));

  take_lock(&lock);

  let trust = write_trusted(file, folder);

  let _ = std::fs::remove_dir(&lock);

  trust
}

fn write_onboarded(file: &Path) -> Trust {
  let present = file.exists();
  let text = if present {
    match std::fs::read_to_string(file) {
      Ok(text) => text,
      Err(_) => return Trust::Unreadable,
    }
  } else {
    "{}".into()
  };
  let Ok(config) = serde_json::from_str::<IndexMap<String, Box<RawValue>>>(&text) else {
    return Trust::Unreadable;
  };

  if config.get(ONBOARDED_KEY).is_some_and(|done| done.get() == "true") {
    return Trust::Trusted;
  }

  let Ok(dark) = RawValue::from_string("\"dark\"".to_owned()) else {
    return Trust::Unreadable;
  };
  let mut written: IndexMap<String, Box<RawValue>> = IndexMap::from([("theme".to_owned(), dark)]);
  let Ok(done) = RawValue::from_string("true".to_owned()) else {
    return Trust::Unreadable;
  };

  written.extend(config);
  written.insert(ONBOARDED_KEY.to_owned(), done);

  let Ok(body) = serde_json::to_string_pretty(&written) else { return Trust::Unreadable };
  let mode = if present {
    std::fs::metadata(file).map_or(0o600, |meta| meta.permissions().mode() & 0o777)
  } else {
    0o600
  };
  let fresh = PathBuf::from(format!("{}.trust-{}", file.display(), std::process::id()));
  let wrote = std::fs::write(&fresh, format!("{body}\n"))
    .and_then(|()| std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(mode)))
    .and_then(|()| std::fs::rename(&fresh, file));

  if wrote.is_err() {
    let _ = std::fs::remove_file(&fresh);

    return Trust::Unreadable;
  }

  Trust::Written
}

/// The configuration a home holding no login is started on, its first-run question — the colour theme
/// — answered, every other entry kept as it stood (docs/own-llm.md, 「Claude Code in the pane」).
pub fn mark_onboarded(file: &Path) -> Trust {
  if !file.parent().is_some_and(Path::exists) {
    return Trust::NoConfigDir;
  }

  let lock = PathBuf::from(format!("{}.lock", file.display()));

  take_lock(&lock);

  let marked = write_onboarded(file);

  let _ = std::fs::remove_dir(&lock);

  marked
}

/// A program's login readied just before the program starts (docs/work-nodes.md, 「The logins on a node」): the
/// file's directory made where it is not, the login's first-run screens marked as passed, and the program's folder
/// trusted in it.
pub fn ready_login(file: &Path, folder: &Path) -> Trust {
  if let Some(dir) = file.parent() {
    let _ = std::fs::create_dir_all(dir);
  }

  mark_onboarded(file);

  match folder.to_str() {
    Some(folder) => trust_folder(file, folder),
    None => Trust::Unreadable,
  }
}

/// The configuration file of a configuration directory.
pub fn config_file(dir: &Path) -> PathBuf {
  dir.join(CONFIG_NAME)
}
