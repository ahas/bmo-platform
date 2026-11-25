//! The files a supervisor's reset takes (docs/supervisors.md, 「Starting it afresh」): the programs' own
//! records of every harness session run in its home or its brain folder, as each provider names them, and
//! the prompts their histories kept of either. The brain folder itself is the brain's to take.

use std::path::{Path, PathBuf};

use harness_providers::{Machine, providers};

/// Where a file may be removed from: anywhere where the data root is the machine's own, and inside
/// the data root alone where it is a bench's or a test's, whose rows name the machine's own files.
pub fn removable(machine: &Machine, path: &Path) -> bool {
  let own = Path::new(machine.var("HOME").unwrap_or_default()).join(".local/share/bmo/data");
  let real = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

  real(machine.data_root()) == real(&own) || path.starts_with(machine.data_root())
}

/// Every record the harnesses keep of the sessions run in `cwd`, each as its provider names them, in every
/// configuration folder they may stand in, `logins` the homes of the logins agency holds, where they stand:
/// a session no room recorded, one a turn opened and never finished, among them.
pub fn harness_records(machine: &Machine, logins: &[PathBuf], cwd: &Path) -> Vec<PathBuf> {
  let mut records: Vec<PathBuf> = providers()
    .into_iter()
    .flat_map(|provider| provider.home_records(machine, logins, cwd))
    .collect();

  records.retain(|path| path.exists() && removable(machine, path));
  records.sort();
  records.dedup();

  records
}

/// Every prompt the harnesses kept in their histories of what was put in while they worked in `cwd` taken
/// out of them, each as its provider keeps them and under `logins` as `harness_records` reads them: the words
/// a turn or a writer was given stand there as they stand in its session's record. What could not be taken,
/// said.
pub fn forget_prompts(machine: &Machine, logins: &[PathBuf], cwd: &Path) -> Vec<String> {
  providers()
    .into_iter()
    .filter_map(|provider| provider.forget_prompts(machine, logins, cwd, None).err())
    .collect()
}

/// Removes a file or a folder.
pub fn remove(path: &Path) -> std::io::Result<()> {
  if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }
}
