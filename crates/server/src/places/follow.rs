//! The standing panes of every agent's room sessions (docs/supervisors.md, 「What a turn shows」): in the
//! agent's own herdr workspace, one tab per room session, named by the room's title, running the follow
//! verb. Every name comes from the rows and from the consumer that owns the room; none is written here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use store::all;

use super::dev::binary_version;
use super::home::{Launch, STANDING_MARK, standing_command};
use super::{AgentRow, Ground, VERBS_BINARY};
use crate::Refusal;
use crate::hooks::HookPoint;

// The verb the follow pane runs, the verbs' binary's supervisor verb and its form.
const FOLLOW_VERB: [&str; 2] = ["supervisor", "follow"];

/// The name the standing mark carries for a follow pane.
pub const FOLLOW_NAME: &str = "follow";

/// The point a room's title is asked at (docs/api.md, 「Hooks」): the consumer that owns the room answers
/// its title for its chat id. The default answers none, and the tab is named by the chat id.
pub struct RoomTitle;

impl HookPoint for RoomTitle {
  type Ask = i64;
  type Answer = Option<String>;
  const NAME: &'static str = "room-title";

  fn default_answer(_chat_id: i64) -> Option<String> {
    None
  }
}

crate::wired!(RoomTitle);

/// The turns that keep two passes over one agent's follow panes from running at once, for as long as the
/// process stands.
#[derive(Default)]
pub struct FollowTurns {
  turns: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl FollowTurns {
  fn of(&self, agent: &str) -> Arc<tokio::sync::Mutex<()>> {
    self
      .turns
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .entry(agent.to_owned())
      .or_default()
      .clone()
  }
}

/// Whether a command line is a follow pane of this agent: the mark it carries and the agent it names.
pub fn follows_for(agent: &str, command: &[String]) -> bool {
  let mark = format!("{STANDING_MARK}{FOLLOW_NAME}");
  let named = command.iter().position(|part| part == "--agent").and_then(|at| command.get(at + 1));

  command.contains(&mark) && named.is_some_and(|named| named == agent)
}

/// The follow verb's command line for one room and one agent, at the verbs' binary's version.
pub fn follow_verb(binary: &str, chat_id: i64, agent: &str, version: &str) -> Vec<String> {
  vec![
    binary.to_owned(),
    FOLLOW_VERB[0].to_owned(),
    FOLLOW_VERB[1].to_owned(),
    chat_id.to_string(),
    "--agent".to_owned(),
    agent.to_owned(),
    format!("--version={version}"),
  ]
}

/// The tabs an agent's room sessions want, one per session in the order of their rooms, each named by
/// its room's title.
async fn tabs(ground: &Ground, agent: &str) -> Result<Vec<(String, Launch)>, Refusal> {
  // A pane started from an older binary keeps running the code it started with, so a pane whose version
  // word differs is replaced.
  let version = binary_version(ground);
  let root = ground.code_root();
  let binary = root.join("bin").join(VERBS_BINARY).to_string_lossy().into_owned();
  let sessions = all(
    &ground.db,
    r#"SELECT DISTINCT "chat_id" FROM "room_sessions" WHERE "agent" = $1 ORDER BY "chat_id""#,
    vec![agent.into()],
  )
  .await?;
  let mut wanted = vec![];

  for session in sessions {
    let chat_id: i64 = session.try_get("", "chat_id")?;
    let title = ground.hooks.ask::<RoomTitle>(chat_id).await;

    wanted.push((
      title.unwrap_or_else(|| chat_id.to_string()),
      Launch {
        command: standing_command(FOLLOW_NAME, &follow_verb(&binary, chat_id, agent, &version)),
        cwd: ground.pane_dir(),
        env: ground.run_env(&format!("bmo-follow-{agent}")),
      },
    ));
  }

  Ok(wanted)
}

/// One agent's follow panes brought to its sessions, one pass at a time for the agent.
pub async fn stand(ground: &Ground, turns: &FollowTurns, row: &AgentRow) -> Result<(), String> {
  let turn = turns.of(&row.name);
  let _held = turn.lock().await;
  let wanted = tabs(ground, &row.name).await.map_err(|refusal| refusal.detail)?;

  if wanted.is_empty() {
    return Ok(());
  }

  if !ground.node_heard(row.node.as_deref()) {
    return Ok(());
  }

  let Some(home) = ground.home_of_row(row).await.map_err(|refusal| refusal.detail)? else {
    return Ok(());
  };

  home
    .stand_tabs(&wanted, |command| follows_for(&row.name, command))
    .await
    .map(drop)
    .map_err(|fault| format!("herdr refused ({fault})"))
}

/// One agent's follow panes stood as a session of its opens; nothing here throws, since a herdr that cannot
/// be reached costs the panes and never the turn that asked for them.
pub async fn stand_agent(ground: &Ground, turns: &FollowTurns, agent: &str) {
  match AgentRow::named(&ground.db, agent).await {
    Ok(Some(row)) => {
      if let Err(why) = stand(ground, turns, &row).await {
        eprintln!("session panes of {agent}: {why}");
      }
    }
    Ok(None) => {}
    Err(refusal) => eprintln!("session panes of {agent}: {}", refusal.detail),
  }
}

/// Every agent holding room sessions, one after another.
pub async fn stand_all(ground: &Ground, turns: &FollowTurns) {
  let rows = match AgentRow::standing(&ground.db).await {
    Ok(rows) => rows,
    Err(refusal) => return eprintln!("session panes: {}", refusal.detail),
  };

  for row in &rows {
    if let Err(why) = stand(ground, turns, row).await {
      eprintln!("session panes of {}: {why}", row.name);
    }
  }
}
