//! A worker's folder trusted under every login (docs/agent-control.md, 「A first interactive run stops at
//! the trust-this-folder check」): the logins this machine's runs may go out on, read from the store, each
//! file written by the agent host's own folder trust.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::Ground;
use crate::control::claude_accounts::accounts;
pub use agent_host::trust::{Trust, config_file, mark_onboarded, trust_folder};

/// Every configuration file a run of this machine may read its folder trust from: each login's own
/// configuration home's, a directory account's or an endpoint account's, and the machine's own, which a
/// login handed as a token goes out on.
async fn config_files(ground: &Ground) -> Vec<PathBuf> {
  let mut files = BTreeSet::new();

  match accounts(&ground.db).await {
    Ok(held) => {
      files.extend(held.iter().filter_map(|account| account.credential.own_home()).map(config_file))
    }
    Err(refused) => eprintln!("trust: the logins could not be read: {}", refused.error),
  }

  files.insert(match ground.vars().get("CLAUDE_CONFIG_DIR").filter(|dir| !dir.is_empty()) {
    Some(dir) => config_file(Path::new(dir)),
    None => config_file(&ground.roots.home),
  });

  files.into_iter().collect()
}

/// A worker's folder trusted under every login, what was written and what could not be said on the
/// process's output under the worker's name. A folder that is not there is nobody's yet and is left.
pub async fn trust_worker_folder(ground: &Ground, folder: &str, worker: &str) {
  if !Path::new(folder).exists() {
    return;
  }

  for file in config_files(ground).await {
    let (asked, trusted) = (file.clone(), folder.to_owned());
    let trust = tokio::task::spawn_blocking(move || trust_folder(&asked, &trusted)).await;

    match trust {
      Ok(Trust::Written) => println!("trust {worker}: {folder} is now trusted in {}", file.display()),
      Ok(trust @ Trust::Unreadable) => {
        eprintln!("trust {worker}: {folder} could not be trusted in {} ({trust:?})", file.display());
      }
      Ok(_) => {}
      Err(why) => eprintln!("trust {worker}: {why}"),
    }
  }
}
