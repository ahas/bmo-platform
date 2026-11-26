//! The places a thrown-away session on a road whose program carries its own turns goes out in
//! (docs/api.md, 「sessions」): one place a session, named after it and marked as its own by a record in it,
//! so it stands under no agent's row and no sweep of leftovers takes it down while the session stands; ended
//! with the session, and every one the process before this one left ended as a server starts, since a
//! thrown-away session does not outlive the process that started it, except one carrying a turn in flight
//! that the server picks up, a job's among them, which stands until that turn's end is handed back and is
//! ended then (docs/sessions.md, 「A restart hands the session over」).

use std::path::Path;

use agent_session::SessionId;
use policy::fill_marks;
use policy::place_words::PLACE_WORDS;
use serde_json::{Value, json};

use crate::Refusal;
use crate::lines::place::{Place, places_dir, token_of};
use crate::lines::token;
use crate::places::Ground;
use crate::places::locks::{agent_lock_path, hold};
use crate::watches::links;

/// The record a thrown-away session's place keeps while the session stands, naming the session.
pub const PLACE_RECORD: &str = "apart-place.json";

/// The place a thrown-away session's turns go into, by the session's id: its agent and its number.
pub fn place_name(session: &SessionId) -> String {
  let named: String = session
    .0
    .chars()
    .map(|one| if one.is_ascii_alphanumeric() || one == '-' { one } else { '-' })
    .collect();

  format!("apart-{named}")
}

/// Whether a name is a thrown-away session's place, as its record says.
pub fn is_apart_place(data: &Path, name: &str) -> bool {
  agent_host::disk::read_json::<Value>(&Place::of(data, name).path.join(PLACE_RECORD))
    .is_some_and(|record| record["place"] == name)
}

/// The session a thrown-away session's place stands for, as its record names it; none for a name that is no
/// such place or whose record names no session.
pub fn session_of(data: &Path, name: &str) -> Option<String> {
  let record = agent_host::disk::read_json::<Value>(&Place::of(data, name).path.join(PLACE_RECORD))?;

  if record["place"] != name {
    return None;
  }

  record["session"].as_str().filter(|session| !session.is_empty()).map(str::to_owned)
}

/// The agent whose thrown-away session a place is, as its record names the session: the name before the
/// session's `@`; none for a name that is no such place.
pub fn session_agent(data: &Path, name: &str) -> Option<String> {
  let session = session_of(data, name)?;

  session.split('@').next().filter(|agent| !agent.is_empty()).map(str::to_owned)
}

/// A job's run as the place it works in is shown to a person: the run's own name, the item it works where
/// the job holds one, and the seat agent that does its work where the job has one yet, joined as the place's
/// words join them.
pub fn job_label(run: &str, item: Option<i64>, agent: Option<&str>) -> String {
  let words = &PLACE_WORDS.job_place;
  let mut parts = vec![run.to_owned()];

  if let Some(item) = item {
    parts.push(fill_marks(&words.item, &[("item", &item.to_string())]));
  }

  if let Some(agent) = agent {
    parts.push(fill_marks(&words.agent, &[("agent", agent)]));
  }

  parts.join(&words.between)
}

/// What a thrown-away session's place is shown as by a person reading herdr: the job whose run works in it,
/// named by its run, its item and the agent that does its work; the place's own name for a place that stands
/// for no session, for a run that works for no job, and for a place whose job cannot be read. The name a
/// place is kept under — its folder, its record, its lock and the key the sweeps read it by — never moves
/// with this label.
pub async fn label(ground: &Ground, name: &str) -> String {
  let Some(session) = session_of(&ground.roots.data, name) else {
    return name.to_owned();
  };
  let job = match crate::job_words::of_session(&ground.db, &session).await {
    Ok(Some(job)) => job,
    Ok(None) => return name.to_owned(),
    Err(why) => {
      eprintln!("{name}: its job was not read, so its place keeps its own name: {}", why.detail);

      return name.to_owned();
    }
  };

  job_label(
    &job.own_run(),
    (job.item > 0).then_some(job.item),
    job.agent.as_deref().filter(|agent| !agent.is_empty()),
  )
}

/// Every thrown-away session's place standing now, by its name.
pub fn apart_places(data: &Path) -> Vec<String> {
  let Ok(entries) = std::fs::read_dir(places_dir(data)) else {
    return vec![];
  };

  entries
    .filter_map(|entry| entry.ok())
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .filter(|name| is_apart_place(data, name))
    .collect()
}

/// A session's place made ready for its turns: its folder and the record that names it, with the account its run
/// is of where it is one's, whose record of a node places it (docs/person-nodes.md, 「Which node is a person's」).
pub fn mark(ground: &Ground, name: &str, session: &SessionId, account: Option<&str>) -> Result<(), String> {
  let path = Place::of(&ground.roots.data, name).path;
  let record = json!({ "place": name, "session": session.0, "account": account });

  std::fs::create_dir_all(&path).map_err(|why| format!("{}: {why}", path.display()))?;
  std::fs::write(path.join(PLACE_RECORD), record.to_string())
    .map_err(|why| format!("{}: {why}", path.display()))
}

/// The account a thrown-away session's place runs for, as its record names it; none for a run of no account.
pub fn account_of(data: &Path, name: &str) -> Option<String> {
  let record = agent_host::disk::read_json::<Value>(&Place::of(data, name).path.join(PLACE_RECORD))?;

  record["account"].as_str().map(str::to_owned)
}

/// The node a thrown-away session's place stands on now: for a run of an account, the node the account's present
/// record names, the place marked again and that node's link spec written where it names another than the
/// place's mark; the mark as it stands otherwise (docs/person-nodes.md, 「Which node is a person's」).
pub async fn placed_now(ground: &Ground, name: &str) -> Result<Option<String>, Refusal> {
  let place = ground.roots.place(name);
  let marked = links::place_node(&place);
  let Some(account) = account_of(&ground.roots.data, name) else {
    return Ok(marked);
  };
  let now = crate::person_nodes::node_of(&ground.db, &account).await?;

  if now == marked {
    return Ok(marked);
  }

  let Some(cwd) = links::place_cwd(&place) else {
    return Err(Refusal::failed(format!("{name}: its mark names no folder, so it is not placed again")));
  };

  links::mark_place_on(&place, now.as_deref(), &cwd).map_err(Refusal::failed)?;

  if let Some(node) = &now
    && let Err(why) =
      links::write_node_spec(&ground.db, ground.peers.node.as_ref(), &ground.roots, node).await
  {
    eprintln!("{name}: {node}'s link spec was not written again: {why}");
  }

  Ok(now)
}

/// A session's place ended: its program released, the session token it carried taken back, its workspace
/// closed in the herdr of the node it stands on, or of this machine, and its folder taken away, under its
/// lock, taken once a turn still holding it ends however long that takes. The workspace lives as long as the
/// session: no turn's end closes it.
pub async fn end(ground: &Ground, name: &str) {
  let data = &ground.roots.data;
  let _held = match hold(agent_lock_path(data, name)).await {
    Ok(held) => held,
    Err(fault) => return eprintln!("thrown-away place {name}: its lock could not be taken ({fault})"),
  };
  let place = Place::of(data, name);
  let node = links::place_node(&ground.roots.place(name));

  if let Some(token) = place.release().await.as_ref().and_then(token_of) {
    token::revoke(ground, token).await;
  }

  // Stood under the label the place shows, since a herdr that lost its tokens finds a home by that label.
  match ground.home_on(name, &label(ground, name).await, node.as_deref()).await {
    Ok(Some(home)) => {
      let _ = home.close().await;
    }
    Ok(None) => {}
    Err(why) => eprintln!("thrown-away place {name}: its workspace was not closed ({})", why.detail),
  }

  let _ = std::fs::remove_dir_all(&place.path);
}

/// The run a thrown-away place's hooks write for, as the place's run names it: a run in a pane of its own whose
/// line holds its stops (docs/agent-control.md, 「What a resident turn reads」); none for a place naming none.
pub fn run_of(data: &Path, name: &str) -> Option<String> {
  crate::door::place_run::read_run(&Place::of(data, name).path).and_then(|run| run.name)
}

/// Every thrown-away place whose hooks write for the run named, but the one named `beside`, ended: the run's
/// turn has ended, and its end has been handed back.
pub async fn end_run_places(ground: &Ground, run: &str, beside: &str) {
  for name in apart_places(&ground.roots.data) {
    if name != beside && run_of(&ground.roots.data, &name).as_deref() == Some(run) {
      end(ground, &name).await;
    }
  }
}

/// Every thrown-away session's place the process before this one left, ended as a server starts, after the
/// turns it left in flight are picked up: how many. A place carrying a turn the pickup adopted stays, its
/// session ending with that turn, and so does a place whose hooks write for such a turn's run. What each place
/// ended held is written to the log first — the program its record names, whether that process runs, and what
/// its pane's head was last answered — since taking the place down takes that evidence with it.
pub async fn sweep(ground: &Ground) -> usize {
  let turns = crate::turn_pickup::in_flight(&ground.db).await.unwrap_or_default();
  let carrying: Vec<String> = turns.iter().map(crate::turn_pickup::place_name_of).collect();
  let mut ended = 0;

  for name in apart_places(&ground.roots.data) {
    let found = crate::door::liveness::place_found(&Place::of(&ground.roots.data, &name));
    let run = run_of(&ground.roots.data, &name);

    if carrying.contains(&name) || turns.iter().any(|turn| run.as_deref() == Some(turn.run.as_str())) {
      println!("agency: thrown-away place {name} carries a turn picked up, kept until it ends: {found}");
      continue;
    }

    println!("agency: thrown-away place {name} as the last process left it: {found}");
    end(ground, &name).await;
    ended += 1;
  }

  ended
}
