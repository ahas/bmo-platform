use std::path::{Path, PathBuf};

use agency_server::lines::Trees;
use agency_server::lines::agent_plugin::{Whose, carried, placed};

// A folder of the test's own, gone with it.
struct Scratch(PathBuf);

impl Scratch {
  fn new(name: &str) -> Self {
    let root = std::env::temp_dir().join(format!("agency-agent-plugin-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    std::fs::create_dir_all(&root).unwrap();
    Self(root)
  }

  fn path(&self) -> &Path {
    &self.0
  }
}

impl Drop for Scratch {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.0);
  }
}

fn trees(root: &Path) -> Trees {
  Trees {
    code: root.join("code"),
    agents: root.join("agents"),
    source: root.join("source"),
    data: root.join("data"),
    home: root.join("home"),
  }
}

fn skill(root: &Path, plugin: &str, name: &str, text: &str) {
  let folder = root.join("code/prompts/agents").join(plugin).join("plugin/skills").join(name);

  std::fs::create_dir_all(&folder).unwrap();
  std::fs::write(folder.join("SKILL.md"), text).unwrap();
}

fn manifest(root: &Path, plugin: &str) {
  let folder = root.join("code/prompts/agents").join(plugin).join("plugin/.claude-plugin");

  std::fs::create_dir_all(&folder).unwrap();
  std::fs::write(folder.join("plugin.json"), format!(r#"{{"name":"{plugin}"}}"#)).unwrap();
}

#[test]
fn an_agents_plugin_holds_the_skills_it_names_alone_under_its_roles_manifest() {
  let root = Scratch::new("holds");
  let trees = trees(root.path());

  manifest(root.path(), "coder");
  skill(root.path(), "coder", "clean", "coder's clean");
  skill(root.path(), "coder", "design", "design");
  skill(root.path(), "worker", "clean", "worker's clean");
  skill(root.path(), "worker", "closing", "closing");

  assert_eq!(
    carried(&trees).into_iter().collect::<Vec<_>>(),
    ["clean", "closing", "design"].map(str::to_owned)
  );

  let base = trees.agent_dir("coder").join("plugin");
  let names = ["clean", "closing", "gone", "clean"].map(str::to_owned);
  let placed_at = placed(&trees, &base, Whose::Agent("coder-03"), &names, &[]).unwrap();

  assert_eq!(placed_at, root.path().join("agents/role-agents/coder-03/plugin"));
  // Its role's own text of a skill stands before another plugin's, and a skill nothing carries is left out.
  assert_eq!(
    std::fs::read_to_string(placed_at.join("skills/clean/SKILL.md")).unwrap(),
    "coder's clean"
  );
  assert_eq!(std::fs::read_to_string(placed_at.join("skills/closing/SKILL.md")).unwrap(), "closing");
  assert!(!placed_at.join("skills/design").exists());
  assert!(!placed_at.join("skills/gone").exists());
  assert_eq!(
    std::fs::read_to_string(placed_at.join(".claude-plugin/plugin.json")).unwrap(),
    r#"{"name":"coder"}"#
  );
}

#[test]
fn a_roles_list_stands_in_a_folder_of_its_own_beside_an_agents() {
  let root = Scratch::new("role");
  let trees = trees(root.path());

  manifest(root.path(), "coder");
  skill(root.path(), "coder", "clean", "clean");
  skill(root.path(), "coder", "design", "design");

  let base = trees.agent_dir("coder").join("plugin");
  let of_role = placed(&trees, &base, Whose::Role("coder"), &["design".to_owned()], &[]).unwrap();
  let of_agent = placed(&trees, &base, Whose::Agent("coder"), &["clean".to_owned()], &[]).unwrap();

  assert_eq!(of_role, root.path().join("agents/role-skills/coder/plugin"));
  assert_eq!(of_agent, root.path().join("agents/role-agents/coder/plugin"));
  // A role and an agent of the same name keep each its own list.
  assert!(of_role.join("skills/design").is_dir() && !of_role.join("skills/clean").exists());
  assert!(of_agent.join("skills/clean").is_dir() && !of_agent.join("skills/design").exists());
}

#[test]
fn an_agents_plugin_is_written_again_only_when_what_it_should_hold_moved() {
  let root = Scratch::new("moved");
  let trees = trees(root.path());

  manifest(root.path(), "coder");
  skill(root.path(), "coder", "clean", "first");

  let base = trees.agent_dir("coder").join("plugin");
  let placed_at = placed(&trees, &base, Whose::Agent("coder-03"), &["clean".to_owned()], &[]).unwrap();

  // A file of the run's own left in the folder stands while nothing moved.
  std::fs::write(placed_at.join("kept"), "x").unwrap();
  placed(&trees, &base, Whose::Agent("coder-03"), &["clean".to_owned()], &[]).unwrap();
  assert!(placed_at.join("kept").exists());

  // The release's text of the skill moved, so the folder is written whole again.
  skill(root.path(), "coder", "clean", "second");
  placed(&trees, &base, Whose::Agent("coder-03"), &["clean".to_owned()], &[]).unwrap();
  assert!(!placed_at.join("kept").exists());
  assert_eq!(std::fs::read_to_string(placed_at.join("skills/clean/SKILL.md")).unwrap(), "second");

  // A list that names none leaves a plugin that carries no skill.
  placed(&trees, &base, Whose::Agent("coder-03"), &[], &[]).unwrap();
  assert!(placed_at.join("skills").is_dir());
  assert!(!placed_at.join("skills/clean").exists());
}
