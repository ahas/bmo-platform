//! Every place's program rewritten to hand agency's tokens as their files (docs/agent-control.md, 「A token is
//! handed as the file it stands in」): a place no launch composes again keeps what it was written with, so the
//! operator runs this once. A program standing on a place goes on as it is until it next starts.

use std::collections::BTreeMap;
use std::path::Path;

use super::{Answer, VerbError};
use crate::lines::place::{Program, carry, command_digest, places_dir};
use crate::places::{HANDED_AS_FILES, file_variable, hand_as_files, secret_file};

/// A command's environment with each token it holds by value handed as its file: the file the launcher hands,
/// which the launcher's own value fills and a value written here before it never overwrites, written from the
/// program's value only where the launcher has not written it yet. Answers whether anything changed.
pub fn hand_command(env: &mut BTreeMap<String, String>, data_root: &Path) -> bool {
  let before = env.clone();
  let mut fresh = BTreeMap::new();

  for name in HANDED_AS_FILES {
    let Some(value) = env.get(name).filter(|value| !value.trim().is_empty()).cloned() else {
      continue;
    };
    let file = secret_file(data_root, name);

    if file.exists() {
      env.remove(name);
      env.insert(file_variable(name), file.to_string_lossy().into_owned());
    } else {
      fresh.insert(name.to_owned(), value);
    }
  }

  if !fresh.is_empty() {
    let names: Vec<String> = fresh.keys().cloned().collect();

    hand_as_files(&mut fresh, data_root);

    for name in names {
      if let Some(file) = fresh.remove(&file_variable(&name)) {
        env.remove(&name);
        env.insert(file_variable(&name), file);
      }
    }
  }

  carry(env);

  *env != before
}

/// One place's program rewritten where it holds a token by value; answers whether it was.
pub fn hand_place(place: &Path, data_root: &Path) -> Result<bool, String> {
  let path = place.join("program.json");
  let Some(mut program) = agent_host::disk::read_json::<Program>(&path) else {
    return Ok(false);
  };
  let mut changed = hand_command(&mut program.command.env, data_root);

  if let Some(restart) = program.restart.as_mut() {
    changed |= hand_command(&mut restart.env, data_root);
  }

  if !changed {
    return Ok(false);
  }

  program.identity = command_digest(program.restart.as_ref().unwrap_or(&program.command));

  let mut bytes = serde_json::to_vec(&program).map_err(|why| why.to_string())?;

  bytes.push(b'\n');
  agent_host::disk::publish_bytes(&path, &bytes).map_err(|why| format!("{}: {why}", path.display()))?;

  Ok(true)
}

/// `bmo-agency hand-tokens`: every place under the data root rewritten, each one rewritten named.
pub fn hand_tokens(data_root: &Path) -> Result<Answer, VerbError> {
  let dir = places_dir(data_root);
  let mut lines = Vec::new();
  let mut faults = Vec::new();
  let mut places: Vec<_> = std::fs::read_dir(&dir)
    .map_err(|why| VerbError::Refused(format!("{}: {why}", dir.display())))?
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .filter(|path| path.is_dir())
    .collect();

  places.sort();

  for place in places {
    let name = place.file_name().unwrap_or_default().to_string_lossy().into_owned();

    match hand_place(&place, data_root) {
      Ok(true) => lines.push(format!("{name}: its tokens are handed as files")),
      Ok(false) => {}
      Err(why) => faults.push(format!("{name}: {why}")),
    }
  }

  lines.push(format!(
    "{} place(s) rewritten; the tokens handed as files are {}",
    lines.len(),
    HANDED_AS_FILES.map(file_variable).join(", ")
  ));

  if faults.is_empty() {
    Ok(Answer::from(lines))
  } else {
    Err(VerbError::Partly {
      lines,
      refusal: faults.join("\n"),
    })
  }
}
