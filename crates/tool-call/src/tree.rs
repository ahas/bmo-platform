//! The tree agency's tools read `prompts/` from (docs/tools.md, 「A tool's folder」).

use std::path::{Path, PathBuf};

/// The tree holding agency's prompts: the nearest folder above `here` whose `prompts/` holds `agents/`, which is the
/// deploy tree a tool is placed in; else the tree `BMO_DEPLOY_DIR` names; else agency's own deploy link under the
/// home folder. A `prompts/` of another shape, the data root's personas above the tool host's copy of a tool, is
/// passed by.
pub fn prompts_tree(here: &Path) -> PathBuf {
  if let Some(tree) = here.ancestors().find(|tree| tree.join("prompts").join("agents").is_dir()) {
    return tree.to_path_buf();
  }

  match std::env::var_os("BMO_DEPLOY_DIR") {
    Some(named) if !named.is_empty() => PathBuf::from(named),
    _ => std::env::var_os("HOME")
      .map_or_else(|| PathBuf::from("/"), PathBuf::from)
      .join(".local/share/bmo-agency/deploy"),
  }
}
