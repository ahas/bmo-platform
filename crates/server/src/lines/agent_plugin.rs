//! The plugin a run is handed where its agent or its role names skills (docs/role-agents.md, 「What a run
//! reads off its agent」): a folder of the list's own under the agents root, holding the manifest of the role's
//! plugin and the skills named alone, each copied from the first of the release's plugins that carries it.
//! It is written whole whenever what it should hold differs from what it holds, so a release that moved a
//! skill's text and a list that changed both reach the next run. The skills of a person's attached to the run's
//! holder join it (docs/person-nodes.md, 「Attaching skills and MCP servers」), and a supervisor holding any is handed
//! a plugin of its own holding them alone.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::{Trees, fault};
use crate::Refusal;

// Every plugin the release carries: the `plugin` folder of each agent folder under the prompts, by name.
fn plugins(trees: &Trees) -> Vec<PathBuf> {
  let Ok(read) = std::fs::read_dir(trees.code.join("prompts").join("agents")) else {
    return vec![];
  };
  let mut held: Vec<PathBuf> = read
    .filter_map(Result::ok)
    .map(|entry| entry.path().join("plugin"))
    .filter(|plugin| plugin.join("skills").is_dir())
    .collect();

  held.sort();
  held
}

/// The names of every skill a plugin of the release carries, which an agent may name as its own.
pub fn carried(trees: &Trees) -> BTreeSet<String> {
  plugins(trees)
    .iter()
    .filter_map(|plugin| std::fs::read_dir(plugin.join("skills")).ok())
    .flatten()
    .filter_map(Result::ok)
    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .collect()
}

// Every file under a folder, as its path from the folder, in order.
fn files_under(root: &Path) -> Vec<PathBuf> {
  let mut found = vec![];
  let mut open = vec![root.to_owned()];

  while let Some(dir) = open.pop() {
    let Ok(read) = std::fs::read_dir(&dir) else { continue };

    for entry in read.filter_map(Result::ok) {
      let path = entry.path();

      if path.is_dir() {
        open.push(path);
      } else if let Ok(inside) = path.strip_prefix(root) {
        found.push(inside.to_owned());
      }
    }
  }

  found.sort();
  found
}

// One skill's folder copied whole.
fn copy_skill(from: &Path, to: &Path) -> Result<(), Refusal> {
  for inside in files_under(from) {
    let target = to.join(&inside);

    if let Some(dir) = target.parent() {
      std::fs::create_dir_all(dir).map_err(|why| fault(format!("{}: {why}", dir.display())))?;
    }

    std::fs::copy(from.join(&inside), &target)
      .map_err(|why| fault(format!("{}: {why}", target.display())))?;
  }

  Ok(())
}

/// Whose list of skills a plugin is placed for: an agent's own, its role's, or a supervisor's attached skills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Whose<'a> {
  Agent(&'a str),
  Role(&'a str),
  Supervisor(&'a str),
}

/// The manifest a supervisor's own plugin carries, which no plugin of the release gives it.
const SUPERVISOR_MANIFEST: &str =
  r#"{"name":"attached-skills","description":"The skills attached to this supervisor."}"#;

/// The names of the skills a plugin folder holds.
pub fn held_in(plugin: &Path) -> Vec<String> {
  let Ok(read) = std::fs::read_dir(plugin.join("skills")) else {
    return vec![];
  };
  let mut names: Vec<String> = read
    .filter_map(Result::ok)
    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .collect();

  names.sort();
  names
}

impl Whose<'_> {
  // The folder the list's plugin stands in, under the agents root.
  fn home(self, trees: &Trees) -> PathBuf {
    match self {
      Self::Agent(agent) => trees.agents.join("role-agents").join(agent),
      Self::Role(role) => trees.agents.join("role-skills").join(role),
      Self::Supervisor(supervisor) => trees.agents.join("supervisor-skills").join(supervisor),
    }
  }

  fn said(self) -> String {
    match self {
      Self::Agent(agent) => format!("the agent {agent}"),
      Self::Role(role) => format!("the role {role}"),
      Self::Supervisor(supervisor) => format!("the supervisor {supervisor}"),
    }
  }
}

/// The plugin folder of a list of skills, holding `skills` alone under the manifest of `base`, the role's
/// plugin: each skill from `base` where it carries it, else from the first other plugin of the release that
/// does. A skill no plugin of the release carries is left out, and said. Each of `attached`, a skill's name and
/// its folder at home, joins them under its name, one whose name the list holds already left out and said. The
/// folder stands under the agents root at the same path run after run, so the command a run goes out on names it
/// the same way every time.
pub fn placed(
  trees: &Trees,
  base: &Path,
  whose: Whose<'_>,
  skills: &[String],
  attached: &[(String, PathBuf)],
) -> Result<PathBuf, Refusal> {
  let home = whose.home(trees);
  let target = home.join("plugin");
  let manifest = match whose {
    Whose::Supervisor(_) => SUPERVISOR_MANIFEST.as_bytes().to_vec(),
    _ => std::fs::read(base.join(".claude-plugin").join("plugin.json")).unwrap_or_default(),
  };
  let mut sources: Vec<(String, PathBuf)> = vec![];

  for skill in skills {
    if sources.iter().any(|(held, _)| held == skill) {
      continue;
    }

    let found = std::iter::once(base.to_owned())
      .chain(plugins(trees))
      .map(|plugin| plugin.join("skills").join(skill))
      .find(|folder| folder.is_dir());

    match found {
      Some(folder) => sources.push((skill.clone(), folder)),
      None => eprintln!(
        "agency: {} names the skill {skill}, which no plugin of this release carries",
        whose.said()
      ),
    }
  }

  for (skill, folder) in attached {
    if sources.iter().any(|(held, _)| held == skill) {
      eprintln!(
        "agency: {} holds the skill {skill} already, so the one attached is left out",
        whose.said()
      );
    } else {
      sources.push((skill.clone(), folder.clone()));
    }
  }

  // What the folder should hold, as one digest: the manifest and every file of every skill it names.
  let mut digest = Sha256::new();

  digest.update(&manifest);

  for (skill, folder) in &sources {
    digest.update(skill.as_bytes());

    for inside in files_under(folder) {
      digest.update(inside.to_string_lossy().as_bytes());
      digest.update(std::fs::read(folder.join(&inside)).unwrap_or_default());
    }
  }

  let stamp = format!("{:x}", digest.finalize());
  let stamp_path = home.join("plugin.stamp");

  if target.is_dir() && std::fs::read_to_string(&stamp_path).is_ok_and(|held| held.trim() == stamp) {
    return Ok(target);
  }

  // Written beside its place and moved into it whole.
  let fresh = home.join("plugin.new");
  let _ = std::fs::remove_dir_all(&fresh);

  std::fs::create_dir_all(fresh.join("skills"))
    .map_err(|why| fault(format!("{}: {why}", fresh.display())))?;

  if !manifest.is_empty() {
    let folder = fresh.join(".claude-plugin");

    std::fs::create_dir_all(&folder).map_err(|why| fault(format!("{}: {why}", folder.display())))?;
    std::fs::write(folder.join("plugin.json"), &manifest)
      .map_err(|why| fault(format!("{}: {why}", folder.display())))?;
  }

  for (skill, folder) in &sources {
    copy_skill(folder, &fresh.join("skills").join(skill))?;
  }

  let _ = std::fs::remove_dir_all(&target);

  std::fs::rename(&fresh, &target).map_err(|why| fault(format!("{}: {why}", target.display())))?;
  std::fs::write(&stamp_path, &stamp).map_err(|why| fault(format!("{}: {why}", stamp_path.display())))?;

  Ok(target)
}
