//! The owner's own Firefox profile, found as his Firefox finds it and held for one call (docs/tools.md, 「How it
//! pays」): never while his Firefox has it open, never by two calls at once, and handed back with his saved session
//! and his settings as they stood, so a call leaves his tabs and his preferences as he left them.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use ini::Ini;

/// The lock file Firefox holds a record lock on while it has a profile open.
const PARENT_LOCK: &str = ".parentlock";
/// What Firefox keeps the session it restores in: the last session's file and the folder of its backups.
const SESSION: [&str; 2] = ["sessionstore.jsonlz4", "sessionstore-backups"];
/// What Firefox keeps the owner's settings in: the preferences it writes out as it ends, and the user file it lays
/// over them at each start.
const SETTINGS: [&str; 2] = ["prefs.js", USER_FILE];
const USER_FILE: &str = "user.js";
/// What Firefox writes into its profile while its WebDriver BiDi endpoint listens, the endpoint's address, and
/// leaves there where it is ended rather than closed.
const ENDPOINT_NOTE: &str = "WebDriverBiDiServer.json";

/// The profile Firefox opens by default, as its `profiles.ini` in `root` names it: the default profile its
/// installs agree on, else the profile marked default, else the only profile. A path the list marks relative, or
/// an install's default no profile lists, stands under `root`.
pub fn default_profile(root: &Path) -> Result<PathBuf, String> {
  let listed = root.join("profiles.ini");
  let read =
    Ini::load_from_file(&listed).map_err(|why| format!("{} could not be read: {why}", listed.display()))?;
  let profiles: Vec<(&str, bool, bool)> = sections(&read, "Profile")
    .filter_map(|fields| {
      Some((
        fields.get("Path")?,
        fields.get("IsRelative") != Some("0"),
        fields.get("Default") == Some("1"),
      ))
    })
    .collect();
  let mut installs: Vec<&str> =
    sections(&read, "Install").filter_map(|fields| fields.get("Default")).collect();

  installs.sort_unstable();
  installs.dedup();

  let chosen = match installs.as_slice() {
    [only] => Some(
      profiles
        .iter()
        .find(|(path, ..)| path == only)
        .map_or((*only, !Path::new(only).is_absolute()), |(path, relative, _)| (*path, *relative)),
    ),
    _ => profiles
      .iter()
      .find(|(.., marked)| *marked)
      .or(if profiles.len() == 1 { profiles.first() } else { None })
      .map(|(path, relative, _)| (*path, *relative)),
  };

  match chosen {
    Some((path, true)) => Ok(root.join(path)),
    Some((path, false)) => Ok(PathBuf::from(path)),
    None => Err(format!("{} names no default profile", listed.display())),
  }
}

/// The fields of each section of the list whose name begins with `prefix`.
fn sections<'a>(read: &'a Ini, prefix: &'a str) -> impl Iterator<Item = &'a ini::Properties> {
  read
    .iter()
    .filter(move |(section, _)| section.is_some_and(|name| name.starts_with(prefix)))
    .map(|(_, fields)| fields)
}

/// Why a profile could not be held.
#[derive(Debug)]
pub enum Unheld {
  /// Another call holds it.
  Busy,
  /// A Firefox has it open.
  Open,
  /// Its lock or its session could not be handled, with why.
  Fault(String),
}

/// Whether a process other than the open file description `file` stands on holds a record lock on the file, as a
/// Firefox that has the profile open holds one on its `.parentlock`. An open file description's own lock query
/// sees a record lock this process holds as well as another's.
fn record_locked(file: &File) -> std::io::Result<bool> {
  let mut asked = whole_file(libc::F_WRLCK);

  // SAFETY: `fcntl` reads the descriptor and writes the lock description it is handed, which outlives the call.
  if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_OFD_GETLK, &raw mut asked) } == -1 {
    return Err(std::io::Error::last_os_error());
  }

  Ok(i32::from(asked.l_type) != libc::F_UNLCK)
}

/// A record lock of the open file description on the whole file, taken without waiting: false where another
/// holds one. It is the lock Firefox asks for, so no Firefox opens the profile while it is held.
fn take_record_lock(file: &File) -> std::io::Result<bool> {
  let asked = whole_file(libc::F_WRLCK);

  // SAFETY: as in `record_locked`; the description is only read.
  if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_OFD_SETLK, &raw const asked) } == 0 {
    return Ok(true);
  }

  let why = std::io::Error::last_os_error();

  if matches!(why.raw_os_error(), Some(libc::EAGAIN | libc::EACCES)) {
    Ok(false)
  } else {
    Err(why)
  }
}

fn whole_file(kind: libc::c_int) -> libc::flock {
  // SAFETY: `flock` is plain integers, for which zero is a valid value of every field.
  let mut lock: libc::flock = unsafe { std::mem::zeroed() };

  lock.l_type = libc::c_short::try_from(kind).unwrap_or_default();
  lock.l_whence = libc::c_short::try_from(libc::SEEK_SET).unwrap_or_default();
  lock
}

/// The owner's profile held for one call, its saved session moved aside and his settings copied aside into a private
/// folder of the call's in the profiles' folder, which also keeps what the call's browser needs readable beside it,
/// and the call's own preferences laid over his for its one start. Dropped, it gives the session and the settings
/// back as they stood, under Firefox's own lock so no Firefox opens the profile meanwhile; where a Firefox has the
/// profile open by then, they stay aside in its folder, which is kept, rather than being written under a running
/// Firefox.
pub struct Held {
  profile: PathBuf,
  folder: Option<tempfile::TempDir>,
  /// Whether the whole session and the settings were set aside, so whatever stands in their place by the end is the
  /// call's own.
  whole: bool,
  /// The `.parentlock`, locked against another call for as long as this is held. The lock between calls is a whole
  /// file lock, which never meets Firefox's record lock on the same file.
  lock: File,
}

impl Held {
  /// The profile at `profile` held, its session moved aside and its settings copied aside into a folder made in
  /// `beside`, and `preferences`, lines of a Firefox user file, laid over the owner's own for the call's start.
  pub fn take(profile: &Path, beside: &Path, preferences: &str) -> Result<Self, Unheld> {
    let lock = OpenOptions::new()
      .read(true)
      .write(true)
      .create(true)
      .truncate(false)
      .open(profile.join(PARENT_LOCK))
      .map_err(|why| Unheld::Fault(format!("the profile's lock could not be opened: {why}")))?;

    match lock.try_lock() {
      Ok(()) => {}
      Err(TryLockError::WouldBlock) => return Err(Unheld::Busy),
      Err(TryLockError::Error(why)) => return Err(Unheld::Fault(format!("the profile's lock: {why}"))),
    }

    match record_locked(&lock) {
      Ok(false) => {}
      Ok(true) => return Err(Unheld::Open),
      Err(why) => return Err(Unheld::Fault(format!("the profile's lock could not be asked: {why}"))),
    }

    let folder = tempfile::Builder::new()
      .prefix(".deepseek-top-up-")
      .tempdir_in(beside)
      .map_err(|why| Unheld::Fault(format!("the call's folder could not be made: {why}")))?;
    let mut held = Self {
      profile: profile.to_owned(),
      folder: Some(folder),
      whole: false,
      lock,
    };

    std::fs::create_dir(held.aside()).map_err(|why| Unheld::Fault(format!("the call's folder: {why}")))?;

    for name in SESSION {
      let kept = profile.join(name);

      if kept.exists() {
        std::fs::rename(&kept, held.aside().join(name))
          .map_err(|why| Unheld::Fault(format!("the profile's {name} could not be set aside: {why}")))?;
      }
    }

    for name in SETTINGS {
      let kept = profile.join(name);

      if kept.exists() {
        std::fs::copy(&kept, held.aside().join(name))
          .map_err(|why| Unheld::Fault(format!("the profile's {name} could not be set aside: {why}")))?;
      }
    }

    held.whole = true;

    OpenOptions::new()
      .create(true)
      .append(true)
      .open(profile.join(USER_FILE))
      .and_then(|mut user| write!(user, "\n{preferences}\n"))
      .map_err(|why| Unheld::Fault(format!("the call's preferences could not be laid over: {why}")))?;

    Ok(held)
  }

  /// The call's private folder.
  pub fn folder(&self) -> &Path {
    self.folder.as_ref().map_or(self.profile.as_path(), |folder| folder.path())
  }

  pub fn profile(&self) -> &Path {
    &self.profile
  }

  fn aside(&self) -> PathBuf {
    self.folder().join("kept")
  }

  /// The owner's session and settings put back as they stood, and, once the whole of them was set aside, whatever
  /// the call's browser wrote in their place and the note of its endpoint removed first; an error where any of it
  /// could not be.
  fn give_back(&self) -> std::io::Result<()> {
    let note = self.profile.join(ENDPOINT_NOTE);

    if self.whole && note.exists() {
      std::fs::remove_file(note)?;
    }

    for name in SESSION.into_iter().chain(SETTINGS) {
      let left = self.profile.join(name);

      if self.whole && left.is_dir() {
        std::fs::remove_dir_all(&left)?;
      } else if self.whole && left.exists() {
        std::fs::remove_file(&left)?;
      }

      let kept = self.aside().join(name);

      if kept.exists() {
        std::fs::rename(&kept, &left)?;
      }
    }

    Ok(())
  }
}

impl Drop for Held {
  fn drop(&mut self) {
    let given = match take_record_lock(&self.lock) {
      Ok(true) => self.give_back().map_err(|why| why.to_string()),
      Ok(false) => Err("a Firefox opened the profile meanwhile".to_owned()),
      Err(why) => Err(why.to_string()),
    };

    if let Err(why) = given
      && let Some(folder) = self.folder.take()
    {
      let kept = folder.keep();

      eprintln!(
        "the owner's Firefox session and settings could not be given back ({why}); they stand in {}",
        kept.join("kept").display()
      );
    }
  }
}
