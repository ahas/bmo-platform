//! The warming of an agent's kept session (docs/keepalive-schedule.md, 「An agent's kept session」): one
//! session an agent that keeps its cache warm, the one its last answered run kept, pinged before its cache
//! cools by one more turn into it: on its run's own command for a role's session, and through the worker's
//! own session turn for a job's session at its repository's checkout. What a ping goes out on is held in
//! memory alone, since a command carries its run's whole environment; a restart lets the warm sessions cool.

use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

use bmo_cache_warmer_schedule::{Clocks, Record};
use harness_providers::PlaceCache;
use policy::pool_warm::POOL_WARM;

use crate::apart_sessions::{self, Kept};
use crate::lines::claude::Command;
use crate::places::{AgentRow, Ground};
use crate::role_agents::RoleAgent;

/// What a ping of an agent's kept session goes out on: the run that kept it.
#[derive(Clone, Debug, PartialEq)]
pub struct Warm {
  /// The agent, the seat the run was let in on.
  pub agent: String,
  pub session: String,
  pub role: String,
  pub provider: String,
  pub model: String,
  pub how: How,
}

/// How a kept session is pinged, by the kind of run that kept it.
#[derive(Clone, Debug, PartialEq)]
pub enum How {
  /// A role's session: one more turn on the command its run went out on, as it went out.
  Apart {
    line: String,
    /// The name the run stood under.
    run_name: String,
    effort: String,
    account: Option<String>,
    command: Command,
    parent: Option<String>,
    root: Option<String>,
  },
  /// A job's session at its repository's checkout, the worker's own: a turn on that session, which composes
  /// the worker's command afresh as the agent's.
  Worker { row: AgentRow },
}

// The warm sessions, one an agent, by the agent's name.
static WARMS: LazyLock<Mutex<BTreeMap<String, Warm>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));

fn warms() -> std::sync::MutexGuard<'static, BTreeMap<String, Warm>> {
  WARMS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The session an agent's run just kept remembered as the agent's warm one, in the place of any it held.
pub fn remember(warm: Warm) {
  warms().insert(warm.agent.clone(), warm);
}

/// What is remembered of an agent let go; whether anything was.
pub fn forget(agent: &str) -> bool {
  warms().remove(agent).is_some()
}

/// Every warm session remembered, by agent.
pub fn remembered() -> Vec<Warm> {
  warms().values().cloned().collect()
}

/// What a pass makes of one remembered session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Judged {
  /// Nothing is left to keep warm, and what is remembered is let go, with why.
  Forget(&'static str),
  /// Nothing to do this pass.
  Wait,
  /// A ping is due.
  Due,
}

/// What a pass makes of a remembered session at `now`: forgotten where its agent is gone, switched off or no
/// longer keeps its cache warm, where its session was dropped, its provider publishes no lifetime or its cache
/// has already gone; passed over while a run or a fold holds it; and due once it has stood idle its provider's
/// idle stretch with its prefix still there.
pub fn judged(
  agent: Option<&RoleAgent>,
  kept: Option<&Kept>,
  cache: Option<PlaceCache>,
  now: i64,
) -> Judged {
  let Some(agent) = agent else { return Judged::Forget("its agent is gone") };

  if !agent.enabled {
    return Judged::Forget("its agent is switched off");
  }

  if !agent.keeps_cache {
    return Judged::Forget("its agent no longer keeps its cache warm");
  }

  let Some(kept) = kept.filter(|kept| !kept.dropped) else {
    return Judged::Forget("its session was dropped or left behind");
  };
  let Some(cache) = cache else {
    return Judged::Forget("its provider publishes no lifetime");
  };
  let clocks = Clocks {
    idle_ms: cache.idle_minutes * 60_000,
    lifetime_ms: Some(cache.ttl_minutes * 60_000),
    margin_ms: POOL_WARM.margin_minutes * 60_000,
    failure_touches: false,
    honours_idle_override: false,
  };
  let Some(answered) = kept.answered_at else {
    return Judged::Forget("its session never answered");
  };

  if !clocks.kept(now.saturating_sub(answered)) {
    return Judged::Forget("its cache has already gone");
  }

  if kept.held {
    return Judged::Wait;
  }

  let record = Record {
    last_used_at: Some(answered),
    ..Record::default()
  };

  match clocks.due(&record, now, None) {
    true => Judged::Due,
    false => Judged::Wait,
  }
}

/// One pass over the warm sessions remembered: each judged afresh against its agent's row and its session's,
/// what is no longer to be kept warm let go, and each one due pinged in turn, one at a time. Answers how many
/// pings answered. A session whose provider cannot answer now is passed over, since no run goes out on it
/// until it can, and so is one in whose folder a run works now, which keeps it warm itself.
pub async fn pass(ground: &Ground, now: impl Fn() -> i64) -> Result<usize, String> {
  let mut warmed = 0;

  for warm in remembered() {
    let agent = crate::role_agents::named(&ground.db, &warm.agent)
      .await
      .map_err(|why| why.to_string())?;
    let kept = apart_sessions::kept(&ground.db, &warm.session).await.map_err(|why| why.to_string())?;
    let cache = harness_providers::provider(&warm.provider)
      .and_then(|provider| provider.place_cache(&warm.model, None).ok());

    // A later run that kept another session at the same key has left this one behind.
    let newest = match &kept {
      Some(kept) => apart_sessions::newest(&ground.db, kept).await.map_err(|why| why.to_string())?,
      None => false,
    };

    match judged(agent.as_ref(), kept.as_ref().filter(|_| newest), cache, now()) {
      Judged::Forget(why) => {
        // A later run of the agent may have put another session in this one's place meanwhile.
        let mut held = warms();

        if held.get(&warm.agent).is_some_and(|now| now.session == warm.session) {
          held.remove(&warm.agent);
          println!("keepalive {} {}: let go, {why}", warm.agent, warm.session);
        }
      }
      Judged::Wait => {}
      Judged::Due => {
        let path = kept.as_ref().map(|kept| kept.path.clone()).unwrap_or_default();

        if crate::road::provider_closed(ground, &warm.provider).await?.is_some()
          || crate::role_agents::worked_in(&ground.db, &path).await.map_err(|why| why.to_string())?
        {
          continue;
        }

        let answered = match &warm.how {
          How::Apart { .. } => crate::door::apart::ping(ground, &warm).await,
          How::Worker { row } => worker_ping(ground, &warm, row).await,
        };

        if answered {
          warmed += 1;
        }
      }
    }
  }

  Ok(warmed)
}

// A job's session pinged: one turn of the keepalive's word on the worker's own session, composed as the
// agent's, so the command it goes out on is the one the agent's job went out on. The turn records its own
// answer on the session.
async fn worker_ping(ground: &Ground, warm: &Warm, row: &AgentRow) -> bool {
  use crate::door::turn::Quiet;
  use crate::worker_session::{Asked, Side, turn};

  let asked = Asked {
    worker: row,
    model: warm.model.clone(),
    session: Some(warm.session.clone()),
    prompt: policy::keepalive::KEEPALIVE.prompt.clone(),
    name: format!("keepalive-{}", warm.agent),
    title: policy::door_words::DOOR_WORDS.keepalive_title.clone(),
    label: "keepalive".to_owned(),
    clock: None,
  };
  let turned =
    crate::role_agents::composed_as(warm.agent.clone(), turn(&Side { ground }, asked, &mut Quiet)).await;

  match turned {
    Ok(turned) if turned.reached => {
      println!("keepalive {} {}: warmed", warm.agent, warm.session);

      true
    }
    Ok(_) => {
      eprintln!(
        "keepalive {} {}: the ping did not answer, the session stands as it was",
        warm.agent, warm.session
      );

      false
    }
    Err(why) => {
      eprintln!("keepalive {} {}: the ping was refused: {}", warm.agent, warm.session, why.detail);

      false
    }
  }
}
