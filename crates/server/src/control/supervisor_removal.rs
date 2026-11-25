//! What a supervisor's removal takes of its own once its reset has run (docs/supervisors.md, 「Adding and
//! removing a supervisor」): in one write its settings, its base prompt choice, its session reset asks, its
//! gate switches, its sampling, its reset point and its row; then what stands for it and what is left under
//! its name.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use harness_machine::supervisor_prompt::{self, Roots};
use sea_orm::{DatabaseConnection, DatabaseTransaction};
use store::run;

use super::Call;
use super::fault::Fault;
use super::supervisor_settings::roots;
use crate::lines::place::Place;
use crate::peers::brain::Brain;
use crate::places::home::Home;

/// The tables keyed by the agent alone whose rows a removal takes after the reset: its settings, its base
/// prompt choice and the asks of its session resets.
const AGENT_TABLES: [&str; 4] =
  ["supervisor_settings", "agent_base_prompts", "session_resets", "supervisor_sleep"];

/// What the reset keeps of an agent taken out, in one write, its row last.
pub(super) async fn remove(written: &DatabaseTransaction, agent: &str) -> Result<(), Fault> {
  for table in AGENT_TABLES {
    run(written, &format!("DELETE FROM {table} WHERE agent = $1"), vec![agent.into()]).await?;
  }

  // Its own switches of its gates, its sampling and its reset point.
  run(
    written,
    "DELETE FROM settings WHERE key LIKE $1 OR key = $2 OR key = $3",
    vec![
      format!("agent_gate:{agent}:%").into(),
      format!("agent_sampling:{agent}").into(),
      store::reset_point::key(agent).into(),
    ],
  )
  .await?;
  run(written, r#"DELETE FROM "supervisors" WHERE "name" = $1"#, vec![agent.into()]).await?;

  Ok(())
}

/// What a removal reads of the call to take down what is left of the agent, read before it is awaited.
pub(super) struct Left {
  pub(super) db: DatabaseConnection,
  data_root: PathBuf,
  home: Option<Home>,
  roots: Roots,
  brain: Arc<dyn Brain>,
}

impl Left {
  pub(super) fn of(call: &Call, agent: &str) -> Self {
    Self {
      db: call.store().clone(),
      data_root: call.data_root(),
      home: call.places().and_then(|places| places.ground.home_of(agent, agent)),
      roots: roots(call),
      brain: call.ground().peers.brain.clone(),
    }
  }

  /// What stands for a removed agent and what is left under its name, taken once its row is gone: its
  /// program killed, so it writes no record of what the removal took as it goes, its workspace closed where
  /// herdr stands here, its place and home folder, its execution rows with their envelopes, the agent
  /// host's view and head records of it, its agent lock, and, where its brain is to go, everything the
  /// brain holds under its scope. Nothing here refuses the removal, which has already been written; what
  /// could not go is logged.
  pub(super) async fn taken(&self, agent: &str, forget_brain: bool) {
    let place = Place::of(&self.data_root, agent);
    let mut left = vec![];

    place.release_now().await;

    if let Some(home) = &self.home
      && let Err(fault) = home.close().await
    {
      left.push(format!("its workspace: {}", fault.message));
    }

    for folder in [place.path.clone(), supervisor_prompt::folder(&self.roots, agent)] {
      removed(&folder, &mut left);
    }

    match crate::door::execution::forget_agent(&self.db, &self.data_root, agent).await {
      Ok(unremoved) => left.extend(unremoved),
      Err(refusal) => left.push(format!("its execution rows: {}", refusal.detail)),
    }

    let host = agent_host::layout::Layout::new(&self.data_root).agent_files(agent);

    for path in host.into_iter().chain([crate::places::locks::agent_lock_path(&self.data_root, agent)]) {
      removed(&path, &mut left);
    }

    if forget_brain && let Err(fault) = self.brain.forget(agent).await {
      left.push(format!("its brain scope: {fault}"));
    }

    for what in left {
      eprintln!("agency: what was left of the removed agent {agent} was not removed: {what}");
    }
  }
}

// A file or a folder removed, what could not go named among what was left.
fn removed(path: &Path, left: &mut Vec<String>) {
  let gone = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };

  if let Err(why) = gone
    && why.kind() != std::io::ErrorKind::NotFound
  {
    left.push(format!("{}: {why}", path.display()));
  }
}
