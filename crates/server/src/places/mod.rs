//! The workers' places in herdr, stood by agency's server (docs/agent-control.md, 「One standing pane per
//! agent」): every agent's head and its room sessions' follow panes, the workbenches, a worker's folder
//! trusted and its dev server, the nodes' workspaces, the runs' leftovers swept and a renamed
//! worker's checkout, stood as the process starts, as it attaches to herdr and on their clocks.
//!
//! What an agent's program is stays its launcher's: a head stood here is answered whatever the place holds
//! once the launcher writes it (docs/agent-control.md, 「One standing pane per agent」).

pub mod base_mains;
pub mod branch;
pub mod checkouts;
pub mod claims;
pub mod dev;
pub mod follow;
pub mod gate_places;
mod ground;
pub mod heads;
pub mod home;
pub mod idle_supports;
pub mod item_sweep;
pub mod loan_branch;
pub mod locks;
pub mod nodes;
pub mod process_table;
pub mod standby;
pub mod trust;

use std::sync::Arc;
use std::time::Duration;

use self::claims::PaneClaims;
use self::follow::FollowTurns;
pub use self::ground::{
  AgentRow, DEPLOY_VARIABLE, Ground, HANDED_AS_FILES, VERBS_BINARY, file_variable, hand_as_files,
  machine_code_root, node_herdr_ceiling, secret_file, token_or_file,
};
use self::locks::{lock_path, try_hold};
use self::nodes::NodeList;
use crate::clock::every;
use crate::lines::place::Place;
use crate::now_ms;
use crate::watches::links::{self, Systemd};

// How often the heads and the room panes are stood again, a renamed checkout moved, and what an ended
// run left swept (docs/run-groups.md); and how often the node service's list is read for the nodes' workspaces and the links
// brought to the store.
const HEADS_PASS: Duration = Duration::from_secs(10 * 60);
// How often every idle base's main is brought up to its remote's, the remote fetched first
// (docs/worker-slots.md, 「The base checkout's main brought up to the remote's」).
const BASE_MAINS_PASS: Duration = Duration::from_secs(10 * 60);
const SERVERS_PASS: Duration = Duration::from_secs(10 * 60);
const NODES_PASS: Duration = Duration::from_secs(60);
const LINKS_PASS: Duration = Duration::from_secs(60);

/// What the places keep across their passes for as long as the process stands: the node list as the
/// last pass saw it, the turns that keep two passes over one thing from running at once, the hold a
/// supervisor's turns keep what they came to under and its reset moves the reset point under, and the
/// places a gate's turns stand in.
#[derive(Default)]
pub struct Kept {
  pub nodes: NodeList,
  heads_turn: tokio::sync::Mutex<()>,
  pub follow: FollowTurns,
  pub reset: crate::supervisor_turn::overtaken::ResetHold,
  pub gate_places: Arc<gate_places::GatePlaces>,
}

/// The places as one server stands them: the ground, and what is kept between passes.
#[derive(Clone)]
pub struct Places {
  pub ground: Arc<Ground>,
  pub kept: Arc<Kept>,
}

impl Places {
  pub fn new(ground: Ground, kept: Arc<Kept>) -> Self {
    Self {
      ground: Arc::new(ground),
      kept,
    }
  }

  /// Every head, room pane, dev pane, workbench, server and checkout on its moment, for as long as the
  /// process runs: the workbenches made, the renamed checkouts moved and the grown support checkouts settled
  /// as it starts and on the heads' clock; the dev panes of
  /// the workers a job holds, the heads and the room panes as it attaches to herdr and after every drop;
  /// the heads, the room panes and the checkouts again every ten minutes, with each pane a consumer
  /// claims and marks idle taken down; every idle base's main brought up to its remote's every ten minutes;
  /// every run whose program is gone
  /// taken down every ten minutes; and the nodes' workspaces and the links to the work nodes as it starts
  /// and every minute; and the prompt layers' settings declared on the account service's delegation chain as
  /// it starts. A process on a copy of the data root moves no checkout, sweeps no run, keeps no link and
  /// declares nothing: the machine is not the copy's.
  /// The places have stood once, for the readiness the health route answers, when the first pass over
  /// the heads is done.
  pub fn stand(self, watch: Option<herdr::picture::Watch>) {
    let own = self.ground.roots.own_data_root();
    let starting = self.clone();

    tokio::spawn(async move {
      dev::workbenches(&starting.ground).await;

      if own {
        checkouts::move_renamed(&starting.ground).await;
        settle_supports(&starting.ground).await;
      }
    });

    if let Some(watch) = watch {
      let attaching = self.clone();
      let mut attachments = watch.attachments();

      // An attachment made before this follower began is acted on at once, and every one after as it
      // comes.
      tokio::spawn(async move {
        loop {
          let attached = *attachments.borrow_and_update() > 0;

          if attached {
            attaching.on_attached().await;
          }

          if attachments.changed().await.is_err() {
            break;
          }
        }
      });
    } else {
      // With no herdr to attach to there is no head to stand, so the places stand as they are.
      self.ground.readiness.stood();
    }

    let clocked = self.clone();

    tokio::spawn(every("heads' clock", HEADS_PASS, HEADS_PASS, move || {
      let places = clocked.clone();

      async move {
        if own {
          checkouts::move_renamed(&places.ground).await;
          settle_supports(&places.ground).await;
        }

        places.stand_heads().await;
        retire_idle(&places.ground).await;

        Ok(())
      }
    }));

    // The bases' mains brought up on a clock of their own, since each fetches its remote.
    if own {
      let fetching = self.clone();

      tokio::spawn(every("bases' mains", BASE_MAINS_PASS, BASE_MAINS_PASS, move || {
        let places = fetching.clone();

        async move { base_mains::pass(&places.ground, base_mains::Remote::Fetched).await.map(drop) }
      }));
    }

    let noding = self.clone();

    tokio::spawn(every("nodes' clock", Duration::ZERO, NODES_PASS, move || {
      let places = noding.clone();

      async move {
        places.settle_nodes().await;

        Ok(())
      }
    }));

    if own {
      let declaring = self.clone();

      // The settings the prompt layers read off the delegation chain, declared as the server stands
      // (docs/prompt-layers.md, 「The settings agency declares」).
      tokio::spawn(async move {
        if let Err(why) = crate::prompt_layers::declare(&crate::apps::service(&declaring.ground)).await {
          eprintln!("agency: the prompt layers' kinds were not declared: {why}");
        }
      });

      let linking = self.clone();

      tokio::spawn(every("node links' clock", Duration::ZERO, LINKS_PASS, move || {
        let places = linking.clone();

        async move { places.settle_links().await }
      }));

      let sweeping = self.clone();

      tokio::spawn(every("run group sweep", SERVERS_PASS, SERVERS_PASS, move || {
        let places = sweeping.clone();

        async move {
          for line in crate::run_groups::sweep(&places.ground.roots.data, None).await {
            println!("{line}");
          }

          Ok(())
        }
      }));
    }
  }

  /// What an attachment to herdr puts back: each dev server a job is at work for, then every head and
  /// room pane. The dev panes go first, since both make a workspace that is not there, and the two
  /// racing over one agent would leave it with two heads.
  pub async fn on_attached(&self) {
    dev::dev_panes(&self.ground).await;
    self.stand_heads().await;
  }

  /// Every head and every agent's room panes, one pass at a time; the places have stood once the first
  /// pass is done.
  pub async fn stand_heads(&self) {
    let _turn = self.kept.heads_turn.lock().await;

    heads::stand_all(self).await;
    follow::stand_all(&self.ground, &self.kept.follow).await;
    self.ground.readiness.stood();
  }

  /// The node workspaces stood after the node service's list as it now reads.
  pub async fn settle_nodes(&self) {
    nodes::settle(&self.ground, &self.kept.nodes).await;
  }

  /// The links to the work nodes brought to what the store says, each one started, stopped, being
  /// brought up or skipped said on the process's output; as it starts, every minute, and whenever the
  /// workers move. A process on a copy of the data root keeps none.
  pub async fn settle_links(&self) -> Result<(), String> {
    let ground = &self.ground;

    if !ground.roots.own_data_root() {
      return Ok(());
    }

    let settled =
      links::settle(&ground.db, ground.peers.node.as_ref(), &Systemd(&ground.roots), &ground.roots).await?;

    for node in &settled.started {
      println!("node link {node}: started");
    }

    for node in &settled.stopped {
      println!("node link {node}: stopped");
    }

    for node in &settled.syncing {
      println!("node link {node}: its copy is being brought up");
    }

    for node in &settled.checking {
      println!("node link {node}: its workplace is not ready, so it is checked again");
    }

    for skipped in &settled.skipped {
      println!("node link {}: skipped, {}", skipped.node, skipped.why);
    }

    Ok(())
  }
}

/// The support checkouts of the repositories set to `auto` settled (docs/worker-slots.md, 「An idle support
/// checkout」), what could not be said on the error stream.
async fn settle_supports(ground: &Ground) {
  if let Err(why) = idle_supports::pass(ground, crate::now_ms()).await {
    eprintln!("agency: the support checkouts were not settled: {why}");
  }
}

/// Every pane a consumer claims and marks idle taken down where nothing is running in its agent: its
/// program released from the place and its workspace closed. A claim whose agent's lock is held is at
/// work, and a place that holds no program was taken down already.
pub async fn retire_idle(ground: &Ground) {
  let data = &ground.roots.data;

  for claim in ground.hooks.ask::<PaneClaims>(()).await.into_iter().filter(|claim| claim.idle) {
    let Some(_held) = try_hold(&lock_path(data, &claim.agent)) else { continue };

    if Place::of(data, &claim.agent).release().await.is_none() {
      continue;
    }

    if let Some(home) = ground.home_of(&claim.agent, &claim.label) {
      let _ = home.close().await;
    }

    println!("{}: claimed idle, taken down at {}", claim.agent, now_ms());
  }
}
