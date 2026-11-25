//! The node build manifest (docs/work-nodes.md, 「The workplace on a node」) held to the repository
//! workspace: its version, members and dependencies are the root's own, and its members reach nothing
//! outside themselves.

use std::path::Path;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn read(path: &str) -> toml::Table {
  let text = std::fs::read_to_string(Path::new(ROOT).join(path)).expect(path);

  text.parse().expect(path)
}

fn table(manifest: &toml::Table, path: &[&str]) -> toml::Table {
  let mut at = manifest;

  for key in path {
    match at.get(*key).and_then(toml::Value::as_table) {
      Some(next) => at = next,
      None => return toml::Table::new(),
    }
  }

  at.clone()
}

#[test]
fn the_node_build_is_the_root_workspace_narrowed_to_the_node_crates() {
  let root = read("Cargo.toml");
  let node = read("crates/node-link/node-build.toml");
  let whole = table(&root, &["workspace", "dependencies"]);
  let declared = table(&node, &["workspace", "dependencies"]);

  assert_eq!(
    table(&node, &["workspace", "package"]).get("version"),
    table(&root, &["workspace", "package"]).get("version")
  );

  for (name, spec) in &declared {
    assert_eq!(Some(spec), whole.get(name), "{name}");
  }

  let members: Vec<&str> = node["workspace"]["members"]
    .as_array()
    .expect("members")
    .iter()
    .filter_map(toml::Value::as_str)
    .collect();
  let root_members = root["workspace"]["members"].as_array().expect("root members");

  for member in &members {
    assert!(root_members.iter().any(|one| one.as_str() == Some(member)), "{member}");

    let manifest = read(&format!("{member}/Cargo.toml"));

    for section in ["dependencies", "build-dependencies"] {
      for (name, spec) in table(&manifest, &[section]) {
        if let Some(path) = spec.get("path").and_then(toml::Value::as_str) {
          let reached = Path::new(member).join(path);
          let reached = reached.components().fold(Vec::new(), |mut parts, part| {
            match part {
              std::path::Component::ParentDir => {
                parts.pop();
              }
              other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
            }
            parts
          });

          assert!(members.contains(&reached.join("/").as_str()), "{member} reaches {name}");
        } else if spec.get("workspace").is_some() {
          assert!(declared.contains_key(&name), "{member} takes {name} from the workspace");
        }
      }
    }
  }
}
