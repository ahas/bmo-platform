//! A standing branch checkout as the agent a co-work branch's turn goes into (docs/coworker-pool.md,
//! 「Standing branch checkouts and their list」): one per place, named after its pool and its number, in a
//! workspace of its own, shown under the co-work task running there and resting on the place's own label.

use std::path::{Component, Path, PathBuf};

use policy::fill_marks;
use policy::place_words::PLACE_WORDS;

use super::Ground;
use super::home::Home;

/// The co-work task a turn of the place runs: its item and its role.
pub struct BranchRun {
  pub todo: i64,
  pub role: String,
}

/// Where the standing places live, under the data root.
pub fn slot_root(data: &Path) -> PathBuf {
  data.join("branch-agents")
}

/// One place, by its pool and the number that names it.
pub fn slot_dir(data: &Path, pool: &str, slot: i64) -> PathBuf {
  slot_root(data).join(pool).join(slot.to_string())
}

/// The places one pool has, by the numbers that name them.
pub fn pool_slots(data: &Path, pool: &str) -> Vec<i64> {
  let Ok(entries) = std::fs::read_dir(slot_root(data).join(pool)) else { return vec![] };
  let mut slots: Vec<i64> = entries
    .flatten()
    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
    .filter_map(|entry| {
      let name = entry.file_name().into_string().ok()?;

      if name.is_empty() || !name.chars().all(|char| char.is_ascii_digit()) {
        return None;
      }

      name.parse().ok()
    })
    .collect();

  slots.sort_unstable();
  slots
}

/// Which pool a checkout's side work borrows places from: its repository, named for a reader by the
/// repository's own name and made unique by a hash of where its commits live; a directory that is no
/// repository is named by its own path the same way.
pub async fn pool_of(cwd: &str) -> String {
  let common = common_dir(cwd).await;
  let place = common.clone().unwrap_or_else(|| {
    let path = Path::new(cwd);

    lexical(&if path.is_absolute() {
      path.to_path_buf()
    } else {
      std::env::current_dir().unwrap_or_default().join(path)
    })
    .to_string_lossy()
    .into_owned()
  });
  let base = |path: &str| {
    Path::new(path)
      .file_name()
      .map(|name| name.to_string_lossy().into_owned())
      .unwrap_or_default()
  };
  let named = match &common {
    None => base(&place),
    Some(_) if base(&place) == ".git" => base(
      &Path::new(&place)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default(),
    ),
    Some(_) => {
      let name = base(&place);

      match name.strip_suffix(".git") {
        Some(stem) if !stem.is_empty() => stem.to_owned(),
        _ => name,
      }
    }
  };
  let named: String = named
    .chars()
    .map(|char| if char.is_ascii_alphanumeric() || "._-".contains(char) { char } else { '_' })
    .collect();
  let hash = format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(place.as_bytes()));

  format!("{}-{}", if named.is_empty() { "repo" } else { &named }, &hash[..10])
}

// The repository a checkout's commits live in, as an absolute path; none where it is no repository.
async fn common_dir(cwd: &str) -> Option<String> {
  if !Path::new(cwd).exists() {
    return None;
  }

  let output = tokio::process::Command::new("git")
    .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
    .current_dir(cwd)
    .stdin(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .output()
    .await
    .ok()?;

  if !output.status.success() {
    return None;
  }

  let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();

  Some(std::fs::canonicalize(&path).map_or(path, |real| real.to_string_lossy().into_owned()))
}

fn number(slot: i64) -> String {
  format!("{slot:02}")
}

/// The place as a person reads it with nothing of an item running in it.
pub fn place_label(pool: &str, slot: i64) -> String {
  fill_marks(&PLACE_WORDS.coworker_place, &[("pool", pool), ("number", &number(slot))])
}

/// The agent a place stands as, named by its pool and its number, with every character a name does not
/// take a dash.
pub fn place_agent(pool: &str, slot: i64) -> String {
  format!("branch-{pool}-{slot}")
    .chars()
    .map(|char| if char.is_ascii_alphanumeric() || char == '_' || char == '-' { char } else { '-' })
    .collect()
}

// A path with its `.` and `..` read away, as a path is resolved before it is measured against another.
fn lexical(path: &Path) -> PathBuf {
  let mut kept = PathBuf::new();

  for part in path.components() {
    match part {
      Component::CurDir => {}
      Component::ParentDir => {
        kept.pop();
      }
      other => kept.push(other),
    }
  }

  kept
}

/// The pool and the number of the place a directory belongs to, where it is one of the standing checkouts
/// or a directory inside one; none for any other.
pub fn place_of(data: &Path, cwd: &str) -> Option<(String, i64)> {
  let resolved = lexical(Path::new(cwd));
  let within = resolved.strip_prefix(lexical(&slot_root(data))).ok()?;
  let mut parts = within.components();
  let pool = parts.next()?.as_os_str().to_str()?.to_owned();
  let slot = parts.next()?.as_os_str().to_str()?;

  if slot.is_empty() || !slot.chars().all(|char| char.is_ascii_digit()) {
    return None;
  }

  Some((pool, slot.parse().ok()?))
}

/// The home of the place a directory belongs to: under the co-work task's label while one runs there,
/// resting on the place's own; none for a directory outside every standing checkout, or where this machine
/// names no herdr.
pub fn home(ground: &Ground, cwd: &str, run: Option<&BranchRun>) -> Option<Home> {
  let (pool, slot) = place_of(&ground.roots.data, cwd)?;
  let agent = place_agent(&pool, slot);
  let resting = ground.home_of(&agent, &place_label(&pool, slot))?;
  let Some(run) = run else { return Some(resting) };
  let mut home = ground.home_of(&agent, &format!("#{} {} {}", run.todo, run.role, number(slot)))?;

  home.resting = Some(resting.label);

  Some(home)
}
