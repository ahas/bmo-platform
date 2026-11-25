//! Every agent's head, standing (docs/agent-control.md, 「One standing pane per agent」). A head is the
//! agent's one pane — its program where its launcher has written one into its place, the view of it
//! otherwise — and it is the only place the agent's state is shown while nothing of it runs, so it is
//! stood up rather than left to the first turn that needs it.
//!
//! A renamed agent's old workspace is closed first, head and all: nothing answers to its old name any
//! more, and the new name stands its own. Then each agent's home is stood, a head of the other kind in
//! it replaced once no open execution names its pane, and the leftovers of its workspace closed. The
//! names that launch with no row stand nothing of their own, but a head of the other kind left in their
//! workspace is replaced the same way.
//!
//! Nothing here fails the pass: a herdr that cannot be reached costs a pane, never the rest.

use std::collections::HashSet;

use policy::agent_renames::AGENT_RENAMES;
use serde_json::{Value, json};
use store::all;

use super::home::{Home, workspace_agents};
use super::{AgentRow, Places};
use crate::Refusal;

/// The panes an open execution names: what runs on them is still at work.
async fn busy_panes(places: &Places) -> Result<HashSet<String>, Refusal> {
  let rows = all(
    &places.ground.db,
    r#"SELECT "body" FROM "agent_executions" WHERE ("body"::json ->> 'endedAt') IS NULL"#,
    vec![],
  )
  .await?;

  Ok(
    rows
      .iter()
      .filter_map(|row| row.try_get::<String>("", "body").ok())
      .filter_map(|body| serde_json::from_str::<Value>(&body).ok())
      .filter_map(|body| body.get("paneId").and_then(Value::as_str).map(str::to_owned))
      .collect(),
  )
}

/// A head of the other kind in the home closed where nothing runs on it, and the home stood again so
/// its own kind takes the place.
async fn replace_other_heads(home: &Home, busy: &HashSet<String>) {
  let Ok(other) = home.heads_of_other_kind().await else { return };
  let mut closed = 0;

  for pane in other.iter().filter(|pane| !busy.contains(*pane)) {
    if home.herdr.call("pane.close", json!({ "pane_id": pane }), None).await.is_ok() {
      closed += 1;
    }
  }

  if closed > 0 {
    let _ = home.stand().await;
    println!("head of {}: {closed} head(s) of the other kind replaced", home.agent);
  }
}

async fn stand_one(places: &Places, row: &AgentRow, busy: &HashSet<String>) -> Result<(), String> {
  let ground = &places.ground;

  // A head on a node whose link is not heard is stood on a later pass, once it is.
  if !ground.node_heard(row.node.as_deref()) {
    return Ok(());
  }

  let Some(home) = ground.home_of_row(row).await.map_err(|refusal| refusal.detail)? else {
    return Ok(());
  };

  home.stand().await.map_err(|fault| format!("herdr refused ({fault})"))?;
  replace_other_heads(&home, busy).await;

  match home.close_leftovers().await {
    Err(fault) => Err(format!("leftovers unswept ({fault})")),
    Ok(0) => Ok(()),
    Ok(closed) => {
      println!("head of {}: {closed} leftover pane(s) closed", row.name);

      Ok(())
    }
  }
}

/// Every agent's head stood, as the pass above says.
pub async fn stand_all(places: &Places) {
  let ground = &places.ground;
  let Some(herdr) = ground.herdr.clone() else { return };

  for rename in &AGENT_RENAMES.renames {
    match AgentRow::named(&ground.db, &rename.from).await {
      Ok(None) => {}
      Ok(Some(_)) => continue,
      Err(refusal) => {
        eprintln!("head of {}: {}", rename.from, refusal.detail);
        continue;
      }
    }

    let Some(home) = ground.home_of(&rename.from, &rename.label.seeded) else {
      continue;
    };

    match home.close().await {
      Ok(0) => {}
      Ok(_) => println!("head of {}: renamed, its old workspace closed", rename.from),
      Err(fault) => eprintln!("head of {}: {fault}", rename.from),
    }
  }

  let rows = match AgentRow::standing(&ground.db).await {
    Ok(rows) => rows,
    Err(refusal) => return eprintln!("heads: {}", refusal.detail),
  };
  let busy = busy_panes(places).await.unwrap_or_default();

  for row in &rows {
    // What a worker stands with nothing put in is written before its head, which is answered it.
    if let Err(refusal) = super::standby::stand_idle(ground, row).await {
      eprintln!("standby {}: {}", row.name, refusal.detail);
    }

    if let Err(why) = stand_one(places, row, &busy).await {
      eprintln!("head of {}: {why}", row.name);
    }
  }

  // A renamed agent's old name was closed above and is not stood again.
  let known: HashSet<&str> = rows
    .iter()
    .map(|row| row.name.as_str())
    .chain(AGENT_RENAMES.renames.iter().map(|rename| rename.from.as_str()))
    .collect();

  for name in workspace_agents(&herdr).await.unwrap_or_default() {
    if known.contains(name.as_str()) {
      continue;
    }

    if let Some(home) = ground.home_of(&name, &name) {
      replace_other_heads(&home, &busy).await;
    }
  }
}
