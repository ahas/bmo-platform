//! The supervisors' homes carried into `supervisor-homes/` as agency's server starts (docs/supervisors.md,
//! 「Where a supervisor's turn runs」), a migration of the data root: a home standing in the folder the data
//! root held them in before is moved, and with it the folder each Claude Code configuration keeps that home's
//! sessions in, so a program stood on the moved home resumes the session it held. A home the new folder
//! already holds is left where it stands and named in the log.

use std::path::{Path, PathBuf};

use harness_machine::supervisor_prompt::homes;
use harness_providers::claude_code::ClaudeCode;

/// The folder the supervisors' homes stood in, under the data root.
pub const FORMER_HOMES: &str = "companion-homes";

// How deep under the data root a configuration's `projects` folder is looked for.
const DEPTH: usize = 4;

/// Every home under the data root's former folder moved into the supervisors' folder, its session records
/// under each configuration directory beside it: the machine's own and every one under the data root. What
/// was moved and what could not be, one line each.
pub fn carried(data_root: &Path, machine_config: &Path) -> Vec<String> {
  let former = data_root.join(FORMER_HOMES);
  let Ok(entries) = std::fs::read_dir(&former) else {
    return vec![];
  };
  let target = homes(data_root);
  let mut said = vec![];

  if let Err(why) = std::fs::create_dir_all(&target) {
    return vec![format!("{}: {why}", target.display())];
  }

  let mut configs = vec![machine_config.to_path_buf()];

  configurations(data_root, 0, &mut configs);

  for entry in entries.flatten() {
    let from = entry.path();
    let to = target.join(entry.file_name());

    if !from.is_dir() {
      continue;
    }

    if to.exists() {
      said.push(format!("{} stays: {} already stands", from.display(), to.display()));
      continue;
    }

    let records: Vec<PathBuf> =
      configs.iter().map(|config| ClaudeCode::project_folder(config, &from)).collect();

    if let Err(why) = std::fs::rename(&from, &to) {
      said.push(format!("{}: {why}", from.display()));
      continue;
    }

    said.push(format!("{} moved to {}", from.display(), to.display()));

    for (config, old) in configs.iter().zip(records) {
      let new = ClaudeCode::project_folder(config, &to);

      if !old.is_dir() || new.exists() {
        continue;
      }

      match std::fs::rename(&old, &new) {
        Ok(()) => said.push(format!("{} moved to {}", old.display(), new.display())),
        Err(why) => said.push(format!("{}: {why}", old.display())),
      }
    }
  }

  // The former folder goes once it holds nothing.
  let _ = std::fs::remove_dir(&former);

  said
}

// Every configuration directory under a folder: one holding a `projects` folder, looked for a few levels deep
// and never through a link.
fn configurations(folder: &Path, depth: usize, found: &mut Vec<PathBuf>) {
  if depth >= DEPTH {
    return;
  }

  let Ok(entries) = std::fs::read_dir(folder) else {
    return;
  };

  for entry in entries.flatten() {
    if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
      continue;
    }

    let path = entry.path();

    if entry.file_name() == "projects" {
      found.push(folder.to_path_buf());
    } else {
      configurations(&path, depth + 1, found);
    }
  }
}
