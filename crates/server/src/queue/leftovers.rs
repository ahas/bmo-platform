//! What runs and places leave standing in herdr, swept (docs/agent-control.md, 「The whole of herdr is swept of
//! what nothing stands for」), on agency's server: as a server starts, when a job ends and every ten minutes.
//! It reads the whole of herdr — this machine's, and each work node's whose link the node service reports up — because what
//! outlives a run is as often a pane of its own nothing closed, or a workspace no standing agent owns any more,
//! as a shell beside a head.
//!
//! A pane whose run is over — its execution ended, or open with nothing left to carry it — is closed, and a
//! workspace left with no run at work in it whose every pane stands in a removed folder is closed whole. A
//! standing agent's and a node's own workspace are left to the standing passes, a checkout's stands while its
//! dev server does, and only in a workspace this system marked is a pane carrying nothing closed. What the
//! sweep will not close is published as `herdr-leftovers` rather than closed, since an unmarked workspace may
//! be a person's own (docs/api.md, 「Events」). A workspace of another data root's is judged by this store's
//! runs alone.

use std::collections::{BTreeSet, HashSet};

use herdr::Herdr;
use policy::fill_marks;
use policy::queue_words::QUEUE_WORDS;
use serde::Serialize;
use serde_json::{Value, json};
use store::{all, one};

use super::settings;
use crate::Refusal;
use crate::events::Event;
use crate::places::Ground;
use crate::places::branch::place_agent;
use crate::places::claims::PaneClaims;
use crate::places::home::{AGENT_TOKEN, DATA_TOKEN, DEV_VERB, HEAD_MARK, STANDING_MARK};

// The token a node's workspace is reported under, and the mark its pane's loop runs under.
const NODE_TOKEN: &str = "node";
const NODE_MARK: &str = "bmo-node-";

// Where the survivors last told were written, so the same picture is told once.
const TOLD_KEY: &str = "herdr_leftovers_told";

/// One pane as the sweep reads it: its run, what herdr started it with, and whether the folder its process
/// stands in has been removed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaneView {
  pub pane: String,
  pub run: Option<String>,
  pub command: Vec<String>,
  pub cwd_gone: bool,
}

/// One workspace as the sweep reads it, with the data root its token names where it names one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkspaceView {
  pub workspace: String,
  pub label: String,
  pub agent: Option<String>,
  pub node: Option<String>,
  pub data: Option<String>,
  pub panes: Vec<PaneView>,
}

/// What the sweep judges herdr's picture against.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Standing {
  /// The names whose own workspace stands: the supervisors, the co-work places, the gate places, the
  /// thrown-away sessions' places and every agent a consumer claims a pane for.
  pub homes: HashSet<String>,
  /// The repositories, whose checkouts' workspaces stand for their dev servers alone: a checkout past its
  /// repository's present count among them, since its name still says whose it is.
  pub repositories: HashSet<String>,
  /// The runs whose panes stand for nothing any more.
  pub over: HashSet<String>,
  /// The data root this root's workspaces name, none for the machine's own.
  pub data: Option<String>,
}

/// A workspace the sweep would not close, told rather than closed.
#[derive(Clone, Debug, PartialEq)]
pub struct Survivor {
  pub workspace: String,
  pub label: String,
  pub panes: usize,
  pub marked: bool,
}

/// What to close, the runs over whose panes go with it, and what to tell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
  pub close_panes: Vec<String>,
  pub close_workspaces: Vec<String>,
  pub closed_runs: Vec<String>,
  pub survivors: Vec<Survivor>,
}

/// Whether a pane runs a standing process, told by its command alone.
pub fn carries_standing(command: &[String]) -> bool {
  command.iter().any(|part| {
    part.starts_with(HEAD_MARK)
      || part == DEV_VERB
      || part.starts_with(NODE_MARK)
      || part.starts_with(STANDING_MARK)
  })
}

// The agent a head in the workspace names in its mark, which counts for a workspace whose token herdr lost.
fn head_agent(workspace: &WorkspaceView) -> Option<String> {
  workspace
    .panes
    .iter()
    .flat_map(|pane| &pane.command)
    .find_map(|part| part.strip_prefix(HEAD_MARK))
    .filter(|agent| !agent.is_empty())
    .map(str::to_owned)
}

/// What to close and what to tell, out of the workspaces as they stand and what stands beside them.
pub fn plan(workspaces: &[WorkspaceView], standing: &Standing) -> Plan {
  let mut plan = Plan::default();

  for workspace in workspaces {
    let is_over = |pane: &PaneView| pane.run.as_ref().is_some_and(|run| standing.over.contains(run));
    let at_work = workspace.panes.iter().any(|pane| pane.run.is_some() && !is_over(pane));
    let over: Vec<&PaneView> = workspace.panes.iter().filter(|pane| is_over(pane)).collect();
    let kept: Vec<&PaneView> = workspace
      .panes
      .iter()
      .filter(|pane| !is_over(pane) && (pane.run.is_some() || !pane.command.is_empty()))
      .collect();
    let empty: Vec<&PaneView> = workspace
      .panes
      .iter()
      .filter(|pane| pane.run.is_none() && pane.command.is_empty())
      .collect();
    let marked = workspace.agent.is_some() || workspace.node.is_some();
    let agent = workspace.agent.clone().or_else(|| head_agent(workspace));
    let runs_of =
      |panes: &[&PaneView]| -> Vec<String> { panes.iter().filter_map(|pane| pane.run.clone()).collect() };
    let close_whole = |plan: &mut Plan| {
      plan.close_workspaces.push(workspace.workspace.clone());
      plan.closed_runs.extend(runs_of(&over));
    };

    // Another root's workspace, or for a copy the machine's own, is judged by this store's runs alone, which
    // no other root's run is among.
    if workspace.data != standing.data {
      if !over.is_empty() && over.len() == workspace.panes.len() {
        close_whole(&mut plan);
      } else {
        plan.close_panes.extend(over.iter().map(|pane| pane.pane.clone()));
        plan.closed_runs.extend(runs_of(&over));
      }

      continue;
    }

    // A standing agent's own workspace, and a node's, are the standing passes' to tidy: one with no head yet
    // is about to be given one.
    if workspace.node.is_some() || agent.as_ref().is_some_and(|agent| standing.homes.contains(agent)) {
      if kept.iter().any(|pane| carries_standing(&pane.command)) {
        plan.close_panes.extend(empty.iter().map(|pane| pane.pane.clone()));
      }

      continue;
    }

    // Nothing at work stands in a removed folder: an earlier release's head, or a run whose folder went.
    if !at_work && !workspace.panes.is_empty() && workspace.panes.iter().all(|pane| pane.cwd_gone) {
      close_whole(&mut plan);
      continue;
    }

    // A checkout's workspace stands for its dev server, and goes whole once neither it nor a run is in it.
    let checkout = agent.as_deref().and_then(|agent| {
      crate::queue::workers::repository_named_by(agent, |name| standing.repositories.contains(name))
    });

    if checkout.is_some() {
      if !at_work && !kept.iter().any(|pane| pane.command.iter().any(|part| part == DEV_VERB)) {
        close_whole(&mut plan);
      } else {
        plan.close_panes.extend(empty.iter().chain(&over).map(|pane| pane.pane.clone()));
        plan.closed_runs.extend(runs_of(&over));
      }

      continue;
    }

    // A workspace no name of this system's marks may be a person's own: only the panes of runs over in it
    // close, and the whole only where nothing else was ever in it.
    if !marked && agent.is_none() {
      if kept.is_empty() && empty.is_empty() && !over.is_empty() {
        close_whole(&mut plan);
        continue;
      }

      plan.close_panes.extend(over.iter().map(|pane| pane.pane.clone()));
      plan.closed_runs.extend(runs_of(&over));

      if kept.is_empty() && !empty.is_empty() {
        plan.survivors.push(Survivor {
          workspace: workspace.workspace.clone(),
          label: workspace.label.clone(),
          panes: empty.len(),
          marked: false,
        });
      }

      continue;
    }

    // A gone agent's workspace goes whole once nothing is kept in it.
    if kept.is_empty() {
      close_whole(&mut plan);
      continue;
    }

    plan.close_panes.extend(empty.iter().chain(&over).map(|pane| pane.pane.clone()));
    plan.closed_runs.extend(runs_of(&over));

    // A run in it is still at work and says nothing about leftovers.
    if !at_work {
      plan.survivors.push(Survivor {
        workspace: workspace.workspace.clone(),
        label: workspace.label.clone(),
        panes: kept.len(),
        marked,
      });
    }
  }

  plan
}

/// The workspaces as a herdr holds them now, each pane with its command and whether its folder is gone; none
/// where herdr did not answer.
pub async fn read_workspaces(herdr: &Herdr) -> Option<Vec<WorkspaceView>> {
  let snapshot = herdr.snapshot().await.ok()?;
  let mut views = vec![];

  for workspace in &snapshot.workspaces {
    let Some(id) = &workspace.workspace_id else { continue };
    let mut panes = vec![];

    for pane in snapshot.panes.iter().filter(|pane| pane.workspace_id.as_ref() == Some(id)) {
      let Some(pane_id) = &pane.pane_id else { continue };
      let run = pane.run().map(str::to_owned);
      // A pane carrying a run is judged by its run alone, so its command is not asked for.
      let command = if run.is_some() { vec![] } else { herdr.command_of(pane_id).await };

      panes.push(PaneView {
        pane: pane_id.clone(),
        run,
        command,
        cwd_gone: pane.cwd_gone(),
      });
    }

    views.push(WorkspaceView {
      workspace: id.clone(),
      label: workspace.label.clone().unwrap_or_default(),
      agent: workspace.token(AGENT_TOKEN).map(str::to_owned),
      node: workspace.token(NODE_TOKEN).map(str::to_owned),
      data: workspace.token(DATA_TOKEN).map(str::to_owned),
      panes,
    });
  }

  Some(views)
}

/// The names whose own workspace stands, of the store's own: every supervisor and each place of the co-work
/// pools.
pub async fn standing_homes(db: &sea_orm::DatabaseConnection) -> Result<HashSet<String>, Refusal> {
  let mut names = HashSet::new();

  for row in all(db, r#"SELECT "name" FROM "supervisors""#, vec![]).await? {
    names.insert(row.try_get("", "name")?);
  }

  for row in all(db, r#"SELECT DISTINCT "pool", "slot" FROM "branch_agents""#, vec![]).await? {
    let pool: String = row.try_get("", "pool")?;
    let slot: i64 = row.try_get("", "slot")?;

    names.insert(place_agent(&pool, slot));
  }

  Ok(names)
}

/// Of the runs named, those over: an execution of this store's ended, or open while what carried it has gone,
/// as its execution directory says (docs/agent-control.md, 「Completion, cancellation and loss」); the second
/// apart, whose rows close once their panes do. A run this store holds no execution of — another root's, or
/// a consumer's — is never over here.
pub async fn runs_over(
  ground: &Ground,
  runs: &BTreeSet<String>,
) -> Result<(HashSet<String>, HashSet<String>), Refusal> {
  let executions = ground.roots.data.join("executions");
  let mut over = HashSet::new();
  let mut orphaned = HashSet::new();

  for run in runs {
    let Some(row) = one(
      &ground.db,
      r#"SELECT ("body"::jsonb)->>'endedAt' AS "ended" FROM "agent_executions" WHERE "id" = $1"#,
      vec![run.as_str().into()],
    )
    .await?
    else {
      continue;
    };

    if row.try_get::<Option<String>>("", "ended")?.is_some() {
      over.insert(run.clone());
    } else if super::reconcile::collector_ended(&executions.join(run)) {
      over.insert(run.clone());
      orphaned.insert(run.clone());
    }
  }

  Ok((over, orphaned))
}

/// One workspace the sweep would not close, as the leftovers event names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeftWorkspace {
  pub workspace_id: String,
  pub label: String,
  pub panes: usize,
  pub marked: bool,
}

/// What a job's end left open in herdr that the sweep would not close (docs/api.md, 「Events」): how many
/// panes, in which workspaces, and one English sentence of it. Telling the owner is the consumer's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HerdrLeftovers {
  pub panes: usize,
  pub workspaces: Vec<LeftWorkspace>,
  pub account: String,
}

impl Event for HerdrLeftovers {
  const NAME: &'static str = "herdr-leftovers";
}

impl HerdrLeftovers {
  /// The event of the survivors a plan left.
  pub fn of(survivors: &[Survivor]) -> Self {
    let words = &QUEUE_WORDS.leftovers;
    let panes: usize = survivors.iter().map(|one| one.panes).sum();
    let named: Vec<String> = survivors
      .iter()
      .map(|one| {
        let label = if one.label.is_empty() { &one.workspace } else { &one.label };
        let workspace =
          fill_marks(&words.workspace, &[("label", label), ("panes", &one.panes.to_string())]);

        if one.marked {
          workspace
        } else {
          fill_marks(&words.unmarked, &[("workspace", &workspace)])
        }
      })
      .collect();

    Self {
      panes,
      workspaces: survivors
        .iter()
        .map(|one| LeftWorkspace {
          workspace_id: one.workspace.clone(),
          label: one.label.clone(),
          panes: one.panes,
          marked: one.marked,
        })
        .collect(),
      account: fill_marks(&words.account, &[("panes", &panes.to_string()), ("named", &named.join(", "))]),
    }
  }
}

/// One sweep of one herdr: what it may close, closed, the open rows of the runs over whose panes it closed
/// closed with them, and what it would not close published once for each picture; none where herdr could not
/// be asked, which closes and publishes nothing.
pub async fn sweep(ground: &Ground, herdr: &Herdr, standing: &Standing) -> Result<Option<Plan>, Refusal> {
  let db = &ground.db;
  let Some(workspaces) = read_workspaces(herdr).await else {
    return Ok(None);
  };
  let runs: BTreeSet<String> = workspaces
    .iter()
    .flat_map(|workspace| &workspace.panes)
    .filter_map(|pane| pane.run.clone())
    .collect();
  let (over, orphaned) = runs_over(ground, &runs).await?;
  let plan = plan(
    &workspaces,
    &Standing {
      over,
      ..standing.clone()
    },
  );

  for pane in &plan.close_panes {
    let _ = herdr.call("pane.close", json!({ "pane_id": pane }), None).await;
  }

  for workspace in &plan.close_workspaces {
    let _ = herdr.call("workspace.close", json!({ "workspace_id": workspace }), None).await;
  }

  for run in plan.closed_runs.iter().filter(|run| orphaned.contains(*run)) {
    super::reconcile::close_swept(db, run, super::reconcile::PANE_SWEPT).await?;

    // Whatever the run started ends with it (docs/run-groups.md).
    if let Some(line) = crate::run_groups::end_of_run(&ground.roots.data, run).await {
      println!("{line}");
    }
  }

  let told =
    Value::Array(plan.survivors.iter().map(|one| json!([one.workspace, one.panes])).collect()).to_string();

  if plan.survivors.is_empty() {
    settings::forget(db, TOLD_KEY).await?;
  } else if settings::get(db, TOLD_KEY).await?.as_deref() != Some(told.as_str()) {
    settings::set(db, TOLD_KEY, &told).await?;
    ground.events.publish(HerdrLeftovers::of(&plan.survivors));
  }

  Ok(Some(plan))
}

/// What a job's end leaves in herdr swept, once the end has been answered: this machine's herdr, and each
/// ready work node's whose link the node service reports up, on its own socket; then the pinned workers'
/// programs stood again, waiting for the checkout's lock the ended runner still holds to be let go.
pub async fn after_job_end(ground: &Ground) {
  if let Err(refusal) = sweep_everywhere(ground).await {
    eprintln!("agency: leftover sweep: {}", refusal.detail);
  }

  crate::places::standby::restand_all(ground, true).await;
}

/// What stands beside herdr's picture as this root sees it: every name whose own workspace stands, a pane a
/// consumer claims, a gate place and a thrown-away session's place among them (docs/api.md, 「Hooks」, the pane
/// claims, and 「sessions」; docs/agent-control.md, 「A gate's places」) and a checkout a job asked a dev server
/// of, every repository, and the data root this
/// root's workspaces name. Which runs are over is read per herdr, off the runs its panes carry.
pub async fn standing(ground: &Ground) -> Result<Standing, Refusal> {
  let db = &ground.db;
  let mut homes = standing_homes(db).await?;
  let mut repositories = HashSet::new();

  for row in all(db, r#"SELECT "name" FROM "repositories""#, vec![]).await? {
    repositories.insert(row.try_get("", "name")?);
  }

  // A checkout a job asked a dev server of is the dev server's setup to tidy, which may be making its
  // workspace now.
  for checkout in crate::queue::workers::workers_in(db).await? {
    if crate::places::dev::asked(ground, &checkout) {
      homes.insert(checkout);
    }
  }

  homes.extend(ground.hooks.ask::<PaneClaims>(()).await.into_iter().map(|claim| claim.agent));
  homes.extend(crate::places::gate_places::recorded(&ground.roots.data).into_keys());
  homes.extend(crate::apart_places::apart_places(&ground.roots.data));

  Ok(Standing {
    homes,
    repositories,
    over: HashSet::new(),
    data: ground.copy().map(|_| ground.roots.data.to_string_lossy().into_owned()),
  })
}

/// The open rows of runs in a pane of their own that this machine's herdr no longer holds, their collector gone,
/// closed with what their runs started: a run opens its pane of its own on this machine's herdr alone. Nothing
/// where herdr did not answer.
pub async fn close_paneless(ground: &Ground, herdr: &Herdr) -> Result<usize, Refusal> {
  let Ok(snapshot) = herdr.snapshot().await else { return Ok(0) };
  let panes: HashSet<String> = snapshot.panes.into_iter().filter_map(|pane| pane.pane_id).collect();
  let mut closed = 0;

  for run in super::reconcile::paneless(&ground.db, &ground.roots.data, &panes).await? {
    if super::reconcile::close_swept(&ground.db, &run, super::reconcile::PANE_GONE).await? {
      closed += 1;
    }

    if let Some(line) = crate::run_groups::end_of_run(&ground.roots.data, &run).await {
      println!("{line}");
    }
  }

  Ok(closed)
}

/// What runs and places leave in herdr swept on this machine's herdr, where one stands, and on each ready work
/// node's whose link the node service reports up.
pub async fn sweep_everywhere(ground: &Ground) -> Result<(), Refusal> {
  let db = &ground.db;
  let standing = standing(ground).await?;

  if let Some(herdr) = &ground.herdr {
    sweep(ground, herdr, &standing).await?;
    close_paneless(ground, herdr).await?;
  }

  for row in all(
    db,
    r#"SELECT "node" FROM "node_workplaces" WHERE "state" = 'ready' ORDER BY "node""#,
    vec![],
  )
  .await?
  {
    let node: String = row.try_get("", "node")?;

    // A node service that cannot be asked reads the link as down, as a link not heard from lately is.
    if ground.peers.node.link_up(&node).await.unwrap_or(false) {
      let herdr = Herdr::there(ground.roots.data.join("nodes").join(&node).join("herdr.sock"))
        .within(crate::places::node_herdr_ceiling());

      sweep(ground, &herdr, &standing).await?;
    }
  }

  Ok(())
}
