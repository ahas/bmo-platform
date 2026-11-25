//! The standing branch checkouts a work order's branches run in (docs/coworker-pool.md, 「Standing branch
//! checkouts and their list」): the list's judgement of which place still holds a provider's prefix, a place
//! taken for a branch — warm first, then one with nothing to lose, then a new one — laid out of the
//! checkout's open change, its row kept once its result lands, the places an ended order worked in retired
//! from herdr, and the places and pools nothing will use again taken away. What is reused is a place, not a
//! conversation: its path is part of every prefix opened there. Which places are taken is the consumer's to
//! say: it hands in the places its tasks sit in now, and nothing here reads its orders or its jobs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use policy::queue_words::QUEUE_WORDS;
use sea_orm::DatabaseConnection;
use store::{all, run};

use super::dispatch::Fault;
use crate::lines::Trees;
use crate::lines::place::Place;
use crate::lines::standing::worker_prefix_digest;
use crate::places::branch::{self, place_agent, pool_of, pool_slots, slot_dir, slot_root};
use crate::places::{AgentRow, Ground};
use crate::queue::routes;

/// A place a consumer's task sits in now: the checkout its order works in and the place's number. A place
/// is taken exactly as long as the consumer names it so; a restart frees nothing by hand.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Held {
  pub cwd: String,
  pub slot: i64,
}

/// What a branch asks a place for: the checkout its order works in, the selection its task names, its role,
/// and the effort its slot carries where it names one.
#[derive(Clone, Copy, Debug)]
pub struct Asked<'a> {
  pub cwd: &'a str,
  pub selection: &'a str,
  pub role: &'a str,
  pub effort: Option<&'a str>,
}

/// The usable part of a published lifetime, in milliseconds: the list's margin taken off
/// (`policy/pool-warm.toml`), and none where nothing is left of it.
pub fn usable(published_ms: i64) -> Option<i64> {
  bmo_cache_warmer_schedule::usable(published_ms, policy::pool_warm::POOL_WARM.margin_minutes * 60_000)
}

/// How many places a pool keeps standing once free, for the prompt caches they hold. A stage as wide as its
/// plan still gets a place for every branch, and the ones past this are taken away once free.
pub const CAP: i64 = 8;

/// How long a place with nothing alive in it is left standing.
pub const KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;

// The provider whose rows name the prefix they opened on: the one prefix this bot assembles itself.
const DIGESTED: &str = "claude-code";

// The provider whose rows keep the upstream that answered.
const ROUTED: &str = "openrouter";

/// One row of the standing checkouts list: what a place last opened on, for which provider and model.
#[derive(Clone, Debug, PartialEq)]
pub struct Agent {
  pub slot: i64,
  pub mode: String,
  pub model: String,
  pub role: String,
  pub upstream: String,
  pub prefix_digest: String,
  pub last_reply_at: i64,
}

/// How long a place's prefix is worth counting on for a provider and model: the lifetime the provider
/// publishes less the pool's margin, as the keepalive schedule takes it off (docs/keepalive-schedule.md);
/// none where it publishes none or nothing is left of it after the margin. A routed model's cache belongs to
/// the upstream that took the turn.
pub fn ttl_ms(mode: &str, model: &str, upstream: Option<&str>) -> Option<i64> {
  let upstream = upstream.filter(|_| mode == ROUTED);

  harness_providers::provider(mode)
    .and_then(|provider| provider.place_cache(model, upstream).ok())
    .and_then(|cache| usable(cache.ttl_minutes * 60_000))
}

/// Whether the clock still allows a row; `now` in milliseconds, the row's time in seconds.
pub fn alive(agent: &Agent, now: i64) -> bool {
  let upstream = (!agent.upstream.is_empty()).then_some(agent.upstream.as_str());

  ttl_ms(&agent.mode, &agent.model, upstream).is_some_and(|ttl| now - agent.last_reply_at * 1000 < ttl)
}

/// Whether a run opening in the place would hit what the row names: the clock, and for Claude Code the
/// prefix the place would open on now.
pub fn warm(agent: &Agent, now: i64, digest_now: &str) -> bool {
  alive(agent, now) && (agent.prefix_digest.is_empty() || agent.prefix_digest == digest_now)
}

/// What a run leaves in the list, or nothing worth writing: no row for a provider with no published
/// lifetime, nor for a Claude Code run whose prefix could not be read.
pub fn record(
  slot: i64,
  mode: &str,
  model: &str,
  role: &str,
  upstream: Option<&str>,
  digest: Option<&str>,
  at: i64,
) -> Option<Agent> {
  let upstream = upstream.filter(|_| mode == ROUTED);

  ttl_ms(mode, model, upstream)?;

  let claude = mode == DIGESTED;
  let digest = digest.unwrap_or_default();

  if claude && digest.is_empty() {
    return None;
  }

  Some(Agent {
    slot,
    mode: mode.to_owned(),
    model: model.to_owned(),
    role: role.to_owned(),
    upstream: upstream.unwrap_or_default().to_owned(),
    prefix_digest: if claude { digest.to_owned() } else { String::new() },
    last_reply_at: at / 1000,
  })
}

/// The places a caller would use for so many tasks on one provider and model, and the rows to delete.
#[derive(Debug, Default, PartialEq)]
pub struct Choice {
  /// Places whose row for this provider and model still names a cache, so the prefix is a read.
  pub reuse: Vec<i64>,
  /// Places standing free whose prefix this provider no longer has, or never had for this pair.
  pub cold: Vec<i64>,
  /// Places that do not exist yet, by the numbers the pool would give them.
  pub opened: Vec<i64>,
  /// Rows the clock has passed, or whose prefix has moved.
  pub expired: Vec<Agent>,
}

/// The pool's answer for `want` tasks on one provider and model: the warm places, oldest answer first since
/// refreshing the one closest to going cold keeps more alive, then free ones with nothing to lose, then new
/// numbers no one stands on.
pub fn choose(
  agents: &[Agent],
  slots: &[i64],
  busy: &HashSet<i64>,
  want: usize,
  (mode, model): (&str, &str),
  now: i64,
  digest_for: impl Fn(i64) -> String,
) -> Choice {
  let mut expired: Vec<Agent> = agents.iter().filter(|agent| !alive(agent, now)).cloned().collect();
  let mut free: Vec<i64> = slots.iter().copied().filter(|slot| !busy.contains(slot)).collect();

  free.sort_unstable();

  // The last row of a slot for this pair is the one read, as a map keyed by the slot keeps it.
  let mine = |slot: i64| {
    agents
      .iter()
      .rfind(|agent| agent.slot == slot && agent.mode == mode && agent.model == model && alive(agent, now))
  };
  let mut warmed: Vec<Agent> = vec![];

  for slot in &free {
    let Some(agent) = mine(*slot) else { continue };

    if warm(agent, now, &digest_for(*slot)) {
      warmed.push(agent.clone());
    } else {
      expired.push(agent.clone());
    }
  }

  warmed.sort_by_key(|agent| agent.last_reply_at);

  let reuse: Vec<i64> = warmed.iter().take(want).map(|agent| agent.slot).collect();
  let warm_slots: HashSet<i64> = warmed.iter().map(|agent| agent.slot).collect();
  let cold: Vec<i64> = free
    .iter()
    .copied()
    .filter(|slot| !warm_slots.contains(slot))
    .take(want.saturating_sub(reuse.len()))
    .collect();
  let mut held: HashSet<i64> = slots.iter().chain(busy.iter()).copied().collect();
  let mut opened = vec![];
  let mut slot = 1;

  while opened.len() < want.saturating_sub(reuse.len() + cold.len()) {
    if held.insert(slot) {
      opened.push(slot);
    }

    slot += 1;
  }

  Choice {
    reuse,
    cold,
    opened,
    expired,
  }
}

/// The places worth taking away: free, with no row still naming a cache, and either past the number the
/// pool keeps or rested longer than it keeps one.
pub fn prunable(
  agents: &[Agent],
  slots: &[i64],
  busy: &HashSet<i64>,
  now: i64,
  cap: i64,
  keep_ms: i64,
) -> Vec<i64> {
  let mut going: Vec<i64> = slots
    .iter()
    .copied()
    .filter(|slot| !busy.contains(slot))
    .filter(|slot| {
      let rows: Vec<&Agent> = agents.iter().filter(|agent| agent.slot == *slot).collect();

      if rows.iter().any(|agent| alive(agent, now)) {
        return false;
      }

      if *slot > cap {
        return true;
      }

      let newest = rows.iter().map(|agent| agent.last_reply_at).max().unwrap_or(0);

      newest > 0 && now - newest * 1000 >= keep_ms
    })
    .collect();

  going.sort_unstable();
  going
}

// A store's or a refusal's words, as a place's fault carries them.
fn faulted(why: impl std::fmt::Display) -> Fault {
  why.to_string()
}

/// The rows of one pool, oldest answer first.
pub async fn agents(db: &DatabaseConnection, pool: &str) -> Result<Vec<Agent>, Fault> {
  all(
    db,
    r#"SELECT "slot", "mode", "model", "role", "upstream", "prefix_digest", "last_reply_at" FROM "branch_agents"
       WHERE "pool" = $1 ORDER BY "last_reply_at""#,
    vec![pool.into()],
  )
  .await
  .map_err(faulted)?
  .iter()
  .map(|row| {
    Ok(Agent {
      slot: row.try_get("", "slot")?,
      mode: row.try_get("", "mode")?,
      model: row.try_get("", "model")?,
      role: row.try_get("", "role")?,
      upstream: row.try_get("", "upstream")?,
      prefix_digest: row.try_get("", "prefix_digest")?,
      last_reply_at: row.try_get("", "last_reply_at")?,
    })
  })
  .collect::<Result<_, sea_orm::DbErr>>()
  .map_err(|why| why.to_string())
}

/// A row of the list written, or written again over the one for the same place, provider, model and
/// upstream.
pub async fn set_agent(db: &DatabaseConnection, pool: &str, agent: &Agent) -> Result<(), Fault> {
  run(
    db,
    r#"INSERT INTO "branch_agents" ("pool", "slot", "mode", "model", "role", "upstream", "prefix_digest", "last_reply_at")
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
       ON CONFLICT ("pool", "slot", "mode", "model", "upstream") DO UPDATE SET "role" = excluded."role",
         "prefix_digest" = excluded."prefix_digest", "last_reply_at" = excluded."last_reply_at""#,
    vec![
      pool.into(),
      agent.slot.into(),
      agent.mode.clone().into(),
      agent.model.clone().into(),
      agent.role.clone().into(),
      agent.upstream.clone().into(),
      agent.prefix_digest.clone().into(),
      agent.last_reply_at.into(),
    ],
  )
  .await
  .map(drop)
  .map_err(faulted)
}

async fn delete_agent(db: &DatabaseConnection, pool: &str, agent: &Agent) -> Result<(), Fault> {
  run(
    db,
    r#"DELETE FROM "branch_agents" WHERE "pool" = $1 AND "slot" = $2 AND "mode" = $3 AND "model" = $4 AND "upstream" = $5"#,
    vec![
      pool.into(),
      agent.slot.into(),
      agent.mode.clone().into(),
      agent.model.clone().into(),
      agent.upstream.clone().into(),
    ],
  )
  .await
  .map(drop)
  .map_err(faulted)
}

async fn delete_agents(db: &DatabaseConnection, pool: &str, slot: i64) -> Result<(), Fault> {
  run(
    db,
    r#"DELETE FROM "branch_agents" WHERE "pool" = $1 AND "slot" = $2"#,
    vec![pool.into(), slot.into()],
  )
  .await
  .map(drop)
  .map_err(faulted)
}

/// The places taken in a pool, or in every pool where none is named: the ones the consumer says its tasks
/// sit in now, each counted in the pool its order's checkout sets. An order of another repository holds
/// numbers of another pool, and no pool counts every order's together.
pub async fn busy_slots(pool: Option<&str>, held: &[Held]) -> HashSet<i64> {
  let mut busy = HashSet::new();

  for one in held.iter().filter(|one| !one.cwd.is_empty()) {
    if let Some(pool) = pool
      && pool_of(&one.cwd).await != pool
    {
      continue;
    }

    busy.insert(one.slot);
  }

  busy
}

// Taking a place off the repository it is a worktree of, whichever that is, and off the disk; the
// registration is pruned rather than removed by name, since the directory is gone by then.
async fn detach(cwd: &Path) {
  let owner = if cwd.join(".git").exists() { common_dir(cwd).await } else { None };

  let _ = tokio::fs::remove_dir_all(cwd).await;

  if let Some(owner) = owner {
    let parent = cwd.parent().unwrap_or(cwd);
    let _ = git(&["--git-dir", &owner, "worktree", "prune"], parent, None).await;
  }
}

async fn common_dir(cwd: &Path) -> Option<String> {
  let output = tokio::process::Command::new("git")
    .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
    .current_dir(cwd)
    .stdin(Stdio::null())
    .stderr(Stdio::null())
    .output()
    .await
    .ok()?;

  if !output.status.success() {
    return None;
  }

  let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();

  Some(std::fs::canonicalize(&path).map_or(path, |real| real.to_string_lossy().into_owned()))
}

// A git command in a directory, failing with what git said.
async fn git(args: &[&str], cwd: &Path, input: Option<&[u8]>) -> Result<Vec<u8>, Fault> {
  let mut child = tokio::process::Command::new("git")
    .args(args)
    .current_dir(cwd)
    .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|why| format!("git: {why}"))?;

  if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
    use tokio::io::AsyncWriteExt;

    stdin.write_all(input).await.map_err(|why| format!("git: {why}"))?;
    drop(stdin);
  }

  let done = child.wait_with_output().await.map_err(|why| format!("git: {why}"))?;

  if done.status.success() {
    return Ok(done.stdout);
  }

  let err = String::from_utf8_lossy(&done.stderr).into_owned();
  let out = String::from_utf8_lossy(&done.stdout).into_owned();

  Err(format!("git: {}", if err.is_empty() { out } else { err }))
}

/// Places standing straight under the root by number, which belong to no pool and which nothing borrows:
/// each taken off its repository and removed with whatever rows name its number outside every pool.
pub async fn tidy_legacy(ground: &Ground, held: &[Held]) -> Result<Vec<i64>, Fault> {
  let root = slot_root(&ground.roots.data);
  let numbered: Vec<i64> = std::fs::read_dir(&root)
    .map(|entries| {
      entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.is_empty() && name.chars().all(|char| char.is_ascii_digit()))
        .filter_map(|name| name.parse().ok())
        .collect()
    })
    .unwrap_or_default();

  if numbered.is_empty() {
    return Ok(vec![]);
  }

  let busy = busy_slots(None, held).await;
  let mut gone = vec![];

  for slot in numbered {
    if busy.contains(&slot) {
      continue;
    }

    detach(&root.join(slot.to_string())).await;
    delete_agents(&ground.db, "", slot).await?;
    gone.push(slot);
  }

  Ok(gone)
}

/// Pools nothing can reach any more — every pool but the ones the workers' directories name and the one the
/// caller is about to hand a place out of — taken away as a place of a live pool is. A place a task still
/// sits in keeps its pool standing until that task has ended.
pub async fn tidy_orphan_pools(
  ground: &Ground,
  keep: Option<&str>,
  held: &[Held],
) -> Result<Vec<String>, Fault> {
  let mut live: HashSet<String> = HashSet::new();

  for worker in AgentRow::workers(&ground.db).await.map_err(faulted)? {
    if let Some(cwd) = worker.cwd.filter(|cwd| !cwd.is_empty()) {
      live.insert(pool_of(&cwd).await);
    }
  }

  live.extend(keep.map(str::to_owned));

  let mut gone = vec![];

  for pool in orphan_pools(ground, &live).await? {
    let busy = busy_slots(Some(&pool), held).await;
    let mut slots: Vec<i64> = pool_slots(&ground.roots.data, &pool);

    slots.extend(agents(&ground.db, &pool).await?.iter().map(|agent| agent.slot));
    slots.sort_unstable();
    slots.dedup();

    let mut held = false;

    for slot in slots {
      if busy.contains(&slot) {
        held = true;
        continue;
      }

      detach(&slot_dir(&ground.roots.data, &pool, slot)).await;
      delete_agents(&ground.db, &pool, slot).await?;
      retire_place(ground, &pool, slot).await;
    }

    if held {
      continue;
    }

    let _ = tokio::fs::remove_dir_all(slot_root(&ground.roots.data).join(&pool)).await;
    gone.push(pool);
  }

  Ok(gone)
}

// The pools standing under the root or named by a row, less the live ones; a name no pool is ever given —
// a number, or one that would climb out of the root — is not this sweep's.
async fn orphan_pools(ground: &Ground, live: &HashSet<String>) -> Result<Vec<String>, Fault> {
  let mut pools: Vec<String> = std::fs::read_dir(slot_root(&ground.roots.data))
    .map(|entries| {
      entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
    })
    .unwrap_or_default();

  pools.extend(
    all(&ground.db, r#"SELECT DISTINCT "pool" FROM "branch_agents" ORDER BY "pool""#, vec![])
      .await
      .map_err(faulted)?
      .iter()
      .filter_map(|row| row.try_get::<String>("", "pool").ok()),
  );
  pools.sort();
  pools.dedup();

  Ok(
    pools
      .into_iter()
      .filter(|pool| {
        !live.contains(pool)
          && !pool.is_empty()
          && pool.chars().all(|char| char.is_ascii_alphanumeric() || "._-".contains(char))
          && !pool.chars().all(|char| char.is_ascii_digit())
          && pool != "."
          && pool != ".."
      })
      .collect(),
  )
}

/// A place taken away from herdr: its program ended and its workspace closed, so nothing stands for a
/// checkout that is gone. A herdr this machine does not name leaves the workspace to the next standing
/// pass.
pub async fn retire_place(ground: &Ground, pool: &str, slot: i64) {
  let agent = place_agent(pool, slot);

  Place::of(&ground.roots.data, &agent).release().await;

  if let Some(home) =
    branch::home(ground, &slot_dir(&ground.roots.data, pool, slot).to_string_lossy(), None)
    && let Err(fault) = home.close().await
  {
    eprintln!("place {pool}/{slot}: {fault}");
  }
}

/// The places an order that has ended worked in — the ones the consumer names, its order working in `cwd` —
/// that no task sits in now; the places themselves stay, only what stands in herdr for them goes.
pub async fn places_to_retire(cwd: &str, used: &[i64], held: &[Held]) -> Option<(String, Vec<i64>)> {
  if cwd.is_empty() {
    return None;
  }

  let pool = pool_of(cwd).await;
  let busy = busy_slots(Some(&pool), held).await;
  let mut used: Vec<i64> = used.iter().copied().filter(|slot| !busy.contains(slot)).collect();

  used.sort_unstable();
  used.dedup();

  Some((pool, used))
}

/// What stands in herdr for the places a done or stopped order worked in taken down, as the consumer asks
/// once its order is no longer active; a place that refuses to go is left standing, since a stage must
/// never fail over tidying.
pub async fn retire_places(ground: &Ground, cwd: &str, used: &[i64], held: &[Held]) {
  if let Some((pool, slots)) = places_to_retire(cwd, used, held).await {
    for slot in slots {
      retire_place(ground, &pool, slot).await;
    }
  }
}

// Places nothing uses and nothing keeps, taken away; one that refuses to go is left for the next time.
async fn tidy_slots(
  ground: &Ground,
  cwd: &str,
  slots: &[i64],
  busy: &HashSet<i64>,
  now: i64,
) -> Result<Vec<i64>, Fault> {
  let pool = pool_of(cwd).await;
  let going = prunable(&agents(&ground.db, &pool).await?, slots, busy, now, CAP, KEEP_MS);

  for slot in &going {
    let place = slot_dir(&ground.roots.data, &pool, *slot);
    let place_text = place.to_string_lossy().into_owned();

    if git(&["worktree", "remove", "--force", &place_text], Path::new(cwd), None)
      .await
      .is_err()
    {
      detach(&place).await;
    }

    delete_agents(&ground.db, &pool, *slot).await?;
    retire_place(ground, &pool, *slot).await;
  }

  Ok(going)
}

// What a place would open on now for a model and effort, or nothing where it cannot be read.
async fn digest_of(ground: &Ground, cwd: &Path, model: &str, effort: Option<&str>) -> Option<String> {
  match worker_prefix_digest(ground, &Trees::of(ground), &cwd.to_string_lossy(), model, effort).await {
    Ok(digest) => Some(digest),
    Err(why) => {
      eprintln!("branch prefix digest: {}", why.detail);
      None
    }
  }
}

/// Which place a branch task works in: a free place whose row for its provider and model still names a
/// cache, then a free one with nothing to lose, then a new one; rows the clock or a moved prefix has
/// finished with deleted on the way past, and the pools and places nothing will use taken away first.
pub async fn take_slot(ground: &Ground, asked: &Asked<'_>, held: &[Held], now: i64) -> Result<i64, Fault> {
  let db = &ground.db;
  let selection = asked.selection;
  let (mode, model) =
    routes::choice(selection).map_or(("claude-code", selection), |choice| (choice.provider, choice.model));
  let cwd = asked.cwd;
  let pool = pool_of(cwd).await;

  tidy_legacy(ground, held).await?;
  tidy_orphan_pools(ground, Some(&pool), held).await?;

  let busy = busy_slots(Some(&pool), held).await;
  let standing = pool_slots(&ground.roots.data, &pool);
  let gone: HashSet<i64> = tidy_slots(ground, cwd, &standing, &busy, now).await?.into_iter().collect();
  let slots: Vec<i64> = standing.into_iter().filter(|slot| !gone.contains(slot)).collect();
  let rows = agents(db, &pool).await?;
  let mut digests: Vec<(i64, String)> = vec![];

  for agent in &rows {
    if agent.mode != mode
      || agent.model != model
      || busy.contains(&agent.slot)
      || !slots.contains(&agent.slot)
    {
      continue;
    }

    let digest =
      digest_of(ground, &slot_dir(&ground.roots.data, &pool, agent.slot), model, asked.effort).await;

    digests.retain(|(slot, _)| *slot != agent.slot);
    digests.push((agent.slot, digest.unwrap_or_default()));
  }

  let decided = choose(&rows, &slots, &busy, 1, (mode, model), now, |slot| {
    digests
      .iter()
      .find(|(held, _)| *held == slot)
      .map(|(_, digest)| digest.clone())
      .unwrap_or_default()
  });

  for dead in &decided.expired {
    delete_agent(db, &pool, dead).await?;
  }

  decided
    .reuse
    .first()
    .or(decided.cold.first())
    .or(decided.opened.first())
    .copied()
    .ok_or_else(|| QUEUE_WORDS.pool.no_place.clone())
}

/// The place a branch works in, for the order it is a task of, working in `cwd`.
pub async fn slot_cwd(ground: &Ground, cwd: &str, slot: i64) -> PathBuf {
  slot_dir(&ground.roots.data, &pool_of(cwd).await, slot)
}

/// A place brought back to the shape a task starts in: rewound to the checkout's commit — or made again
/// where it is no worktree of that repository — with the checkout's open change laid on top, tracked and
/// new files alike, so the branch reads the tree the item is being done in. Ignored files stay with the
/// place.
pub async fn prepare_slot(source: &str, cwd: &Path) -> Result<(), Fault> {
  let source = Path::new(source);
  let head = git(&["rev-parse", "HEAD"], source, None)
    .await
    .map_err(|_| QUEUE_WORDS.pool.head_unread.clone())?;
  let commit = String::from_utf8_lossy(&head).trim().to_owned();

  rewind(source, &commit, cwd).await?;

  let diff = git(&["diff", "HEAD", "--binary"], source, None)
    .await
    .map_err(|_| QUEUE_WORDS.pool.change_unread.clone())?;

  if !diff.is_empty() {
    git(&["apply", "--binary", "-"], cwd, Some(&diff)).await?;
  }

  let extra = git(&["ls-files", "--others", "--exclude-standard", "-z"], source, None)
    .await
    .map_err(|_| QUEUE_WORDS.pool.new_files_unread.clone())?;

  for path in String::from_utf8_lossy(&extra).split('\0').filter(|path| !path.is_empty()) {
    let from = source.join(path);

    if !tokio::fs::symlink_metadata(&from).await.is_ok_and(|meta| meta.is_file()) {
      continue;
    }

    let to = cwd.join(path);

    if let Some(parent) = to.parent() {
      tokio::fs::create_dir_all(parent).await.map_err(|why| why.to_string())?;
    }

    tokio::fs::copy(&from, &to).await.map_err(|why| why.to_string())?;
  }

  Ok(())
}

// A place brought to one commit of the repository `source` is a checkout of: rewound where it already is a
// worktree of that repository, made again where it is not.
async fn rewind(source: &Path, commit: &str, cwd: &Path) -> Result<(), Fault> {
  let repository = common_dir(source).await;
  let standing = cwd.join(".git").exists();

  if standing && repository.is_some() && common_dir(cwd).await == repository {
    git(&["reset", "--hard", "-q"], cwd, None).await?;
    git(&["clean", "-fdq"], cwd, None).await?;
    git(&["checkout", "-q", "--detach", commit], cwd, None).await?;
    return bring_submodules(cwd).await;
  }

  if standing {
    detach(cwd).await;
  }

  // A place registered but gone, or one never made: whatever stands in the way of the checkout is what a
  // failed attempt left behind.
  if let Some(parent) = cwd.parent() {
    tokio::fs::create_dir_all(parent).await.map_err(|why| why.to_string())?;
  }

  git(&["worktree", "prune"], source, None).await?;

  let place = cwd.to_string_lossy().into_owned();

  if git(&["worktree", "add", "--detach", &place, commit], source, None).await.is_err() {
    let _ = tokio::fs::remove_dir_all(cwd).await;
    git(&["worktree", "add", "--detach", &place, commit], source, None).await?;
  }

  bring_submodules(cwd).await
}

// A place's submodules brought onto the commit's pins, as the place itself is.
async fn bring_submodules(cwd: &Path) -> Result<(), Fault> {
  let place = cwd.to_path_buf();

  tokio::task::spawn_blocking(move || crate::places::checkouts::bring_submodules(&place, true))
    .await
    .map_err(|why| why.to_string())?
}

/// What a finished branch leaves in the list, as the consumer names it — the checkout its order works in,
/// the place it worked in and its role — with the provider and model that answered there, the upstream that
/// took it, and the prefix the place now opens on; or nothing where no cache can be named. Only a branch
/// task leaves a line: the consumer asks it of one.
#[allow(clippy::too_many_arguments)]
pub async fn remember_slot(
  ground: &Ground,
  cwd: &str,
  slot: i64,
  role: &str,
  mode: Option<&str>,
  model: Option<&str>,
  effort: Option<&str>,
  upstream: Option<&str>,
) -> Result<(), Fault> {
  let (Some(mode), Some(model)) = (mode.filter(|mode| harness_providers::provider(mode).is_some()), model)
  else {
    return Ok(());
  };
  let place = slot_cwd(ground, cwd, slot).await;
  let digest = if mode == DIGESTED { digest_of(ground, &place, model, effort).await } else { None };
  let Some(row) = record(slot, mode, model, role, upstream, digest.as_deref(), crate::now_ms()) else {
    return Ok(());
  };
  let pool = pool_of(cwd).await;

  set_agent(&ground.db, &pool, &row).await
}
