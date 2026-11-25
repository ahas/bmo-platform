//! A worker's workbench and its dev server (docs/worker-bench.md, 「Workbench folder」, 「Dev server」):
//! the folder its jobs keep things in across sessions, and a pane in its standing workspace running
//! the dev-server pane verb for it while a job of it has asked for one, or none where its row names no
//! command or no request stands. Every call is the same idempotent one, and the pane itself ends the
//! server once no job holds the checkout or the server stands unused.

use std::path::PathBuf;

use policy::job_prompt::JOB_PROMPT;
use sha2::{Digest, Sha256};

use super::home::{DEV_VERB, Launch};
use super::{AgentRow, Ground, VERBS_BINARY, locks, trust};

/// Where a worker's workbench stands.
pub fn workbench(ground: &Ground, worker: &str) -> PathBuf {
  ground.roots.data.join("workbench").join(worker)
}

/// The folder of a worker's workbench that holds each item's scratch (docs/worker-bench.md, 「An item's
/// scratch」).
pub fn item_scratches(ground: &Ground, worker: &str) -> PathBuf {
  workbench(ground, worker).join(&JOB_PROMPT.workbench.scratch)
}

/// The file a job's start verb leaves in the workbench while it wants the worker's dev server; the
/// pane and the runner take it away (docs/worker-bench.md, 「It stands only while a job of the worker
/// runs」).
const DEV_SERVER_WANTED: &str = "dev-server.wanted";

/// Whether a job's request for the worker's dev server stands.
pub fn asked(ground: &Ground, worker: &str) -> bool {
  workbench(ground, worker).join(DEV_SERVER_WANTED).exists()
}

fn make_workbench(ground: &Ground, worker: &str) {
  if let Err(why) = std::fs::create_dir_all(workbench(ground, worker)) {
    eprintln!("workbench of {worker}: {why}");
  }
}

/// A job's start: the worker's workbench made, and a request for its dev server a job before it left taken
/// back, since a job asks for none at its start and starts the server itself once it is about to look at
/// it (docs/worker-bench.md, 「It stands only while a job of the worker runs」).
pub fn job_workbench(ground: &Ground, worker: &str) {
  make_workbench(ground, worker);

  if let Err(why) = std::fs::remove_file(workbench(ground, worker).join(DEV_SERVER_WANTED))
    && why.kind() != std::io::ErrorKind::NotFound
  {
    eprintln!("workbench of {worker}: {why}");
  }
}

/// Every worker's workbench, made where it is not there yet. It asks nothing of herdr.
pub async fn workbenches(ground: &Ground) {
  match AgentRow::workers(&ground.db).await {
    Ok(workers) => workers.iter().for_each(|worker| make_workbench(ground, &worker.name)),
    Err(refusal) => eprintln!("workbenches: {}", refusal.detail),
  }
}

/// The dev command a worker's pane runs: its own row's, or its base's for a support worker.
pub async fn dev_command(ground: &Ground, row: &AgentRow) -> Result<Option<String>, String> {
  if row.kind != "worker" {
    return Ok(None);
  }

  let Some(base) = &row.support_of else { return Ok(row.dev_command.clone()) };

  match AgentRow::named(&ground.db, base).await.map_err(|refusal| refusal.detail)? {
    Some(base) if base.kind == "worker" => Ok(base.dev_command),
    _ => Ok(row.dev_command.clone()),
  }
}

/// What the verbs' binary is, as a word on the pane's own command line. A pane started from an older binary
/// keeps running the code it was started with, so a pane whose word differs is replaced.
pub fn binary_version(ground: &Ground) -> String {
  match std::fs::read(ground.code_root().join("bin").join(VERBS_BINARY)) {
    Ok(text) => format!("{:x}", Sha256::digest(text))[..12].to_owned(),
    Err(_) => "unknown".into(),
  }
}

fn dev_launch(ground: &Ground, worker: &str) -> Launch {
  Launch {
    command: vec![
      ground.code_root().join("bin").join(VERBS_BINARY).to_string_lossy().into_owned(),
      DEV_VERB.to_owned(),
      worker.to_owned(),
      format!("--version={}", binary_version(ground)),
    ],
    cwd: ground.pane_dir(),
    env: ground.run_env(&format!("bmo-dev-{worker}")),
  }
}

async fn bring(ground: &Ground, worker: &str) -> Result<Option<bool>, String> {
  let row = AgentRow::named(&ground.db, worker).await.map_err(|refusal| refusal.detail)?;
  // A resting worker's dev server is closed like a removed one's, and a worker on a work node runs
  // none: its dev server would be this machine's, over a checkout that is the node's. Nor does one
  // stand that no job has asked for.
  let wanted = match &row {
    Some(row)
      if row.node.is_none()
        && !row.resting
        && asked(ground, worker)
        && dev_command(ground, row).await?.is_some() =>
    {
      Some(dev_launch(ground, worker))
    }
    _ => None,
  };
  // A worker whose row is gone has no home to make any more; its workspace is still found by the
  // token it was reported under, which is all closing its dev server needs.
  let home = match &row {
    Some(row) => ground.home_of_row(row).await.map_err(|refusal| refusal.detail)?,
    None => ground.home_of(worker, worker),
  };
  let Some(home) = home else { return Ok(None) };

  if wanted.is_some() {
    make_workbench(ground, worker);
  }

  let _held = locks::hold(locks::dev_pane_lock_path(&ground.roots.data, worker))
    .await
    .map_err(|why| format!("its lock: {why}"))?;
  let (_, started) = home
    .stand_dev_pane(wanted.as_ref())
    .await
    .map_err(|fault| format!("herdr refused ({fault})"))?;

  Ok(Some(started))
}

/// One worker's dev server brought to its row, under the worker's own dev-pane lock, since a job's
/// run and an attachment to herdr may ask at once. Nothing fails here: a herdr that cannot be
/// reached costs the dev server, never what asked for it.
pub async fn dev_pane(ground: &Ground, worker: &str) {
  match bring(ground, worker).await {
    Ok(Some(true)) => println!("dev server of {worker}: started in its workspace"),
    Ok(_) => {}
    Err(why) => eprintln!("dev server of {worker}: {why}"),
  }
}

/// Every worker's dev server a job is at work for, one after another, with the workbenches made first:
/// what a herdr that came back no longer runs put back.
pub async fn dev_panes(ground: &Ground) {
  workbenches(ground).await;

  let workers = match AgentRow::workers(&ground.db).await {
    Ok(workers) => workers,
    Err(refusal) => return eprintln!("dev servers: {}", refusal.detail),
  };

  let at_work = match crate::queue::holds::checkouts_at_work(&ground.db).await {
    Ok(at_work) => at_work,
    Err(refusal) => return eprintln!("dev servers: {}", refusal.detail),
  };

  for worker in workers.iter().filter(|worker| at_work.contains(&worker.name)) {
    if matches!(dev_command(ground, worker).await, Ok(Some(_))) {
      dev_pane(ground, &worker.name).await;
    }
  }
}

/// What follows a change to a worker's row: the workbench of a worker that still has a row, its folder
/// trusted under every login, and its dev server brought to the row either way — started, replaced, or
/// closed.
pub async fn settle(ground: &Ground, worker: &str) {
  let row = match AgentRow::named(&ground.db, worker).await {
    Ok(row) => row,
    Err(refusal) => return eprintln!("worker {worker}: settling its workbench failed: {}", refusal.detail),
  };

  if row.is_some() {
    make_workbench(ground, worker);
  }

  if let Some(row) = row.as_ref().filter(|row| row.kind == "worker") {
    trust::trust_worker_folder(ground, &ground.checkout_of(row), worker).await;
  }

  dev_pane(ground, worker).await;
}
