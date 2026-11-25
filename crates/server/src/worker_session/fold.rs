//! A worker's full session folded inside its own resident program (docs/agent-control.md, 「Folding
//! travels with the resident program」): one more turn of it, told the fold word, on the prefix every
//! other turn stands on and on the login and model the session was opened on. The record says whether
//! it folded: the program writes its compacted marker after the fold word and keeps the session's id.
//! Nothing here fails the caller: the fold is upkeep after the turn it follows is done, and a fold that
//! fails is a line in the log and a record left as it stands.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use harness_providers::claude_code::ClaudeCode;
use policy::door_words::DOOR_WORDS;
use regex::Regex;

use super::held::{self, MODE, scope_of};
use super::records::{self, note_spend, record};
use super::told::SessionFolded;
use super::{Composed, account_named, compose};
use crate::Refusal;
use crate::control::claude_accounts::SEED_ACCOUNT;
use crate::control::harness::provider_enabled;
use crate::door::record::{Tail, command_words, compacted_since};
use crate::door::turn::{Ending, Quiet, put};
use crate::lines::standing::worker_prefix_digest;
use crate::lines::worker::{configured, worker_run};
use crate::lines::{Trees, fault};
use crate::places::{AgentRow, Ground};

/// The fold word Claude Code takes.
pub const FOLD_WORD: &str = "/compact";

/// Whether a worker session is full once its turn is over: at or past its fold ratio of its model's window
/// (docs/sessions.md, 「Session compaction」). A turn that reports no share is not full.
pub fn worker_session_full(context_ratio: Option<f64>, fold_ratio: f64) -> bool {
  context_ratio.is_some_and(|ratio| ratio >= fold_ratio)
}

/// The compaction threshold of a job's session (docs/sessions.md, 「Session compaction」): the one the agent
/// the job is let in on sets, where it sets one — the seat of the slot the job holds at `path`, the folder it
/// works in — else the ratio its provider declares, which also stands where the store cannot be read, since
/// a fold is upkeep that fails nothing.
pub async fn worker_fold_ratio(
  ground: &Ground,
  provider: &str,
  role: Option<&str>,
  path: Option<&str>,
) -> f64 {
  let seat = crate::role_agents::seated_at(&ground.db, role, path).await.unwrap_or_else(|why| {
    eprintln!(
      "agency: the agent at {} did not read, its provider's threshold stands: {why}",
      path.unwrap_or("no path")
    );

    None
  });

  if let Some(own) = seat.as_ref().and_then(|agent| agent.fold_ratio) {
    return own;
  }

  // An agent that sets none, and a job whose agent cannot be told, folds at the ratio its provider declares.
  harness_providers::provider(provider)
    .and_then(|provider| provider.declared().fold_ratio)
    .unwrap_or(crate::order::DEFAULT_FOLD_RATIO)
}

// What the program answers in its own words for a session with too little in it to fold.
static TOO_SHORT: LazyLock<Regex> =
  LazyLock::new(|| Regex::new("(?i)not enough messages to compact").expect("a pattern"));

/// What became of a fold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Folded {
  Compacted,
  TooShort,
  Busy,
  Failed,
}

impl Folded {
  pub fn word(self) -> &'static str {
    match self {
      Self::Compacted => "compacted",
      Self::TooShort => "too-short",
      Self::Busy => "busy",
      Self::Failed => "failed",
    }
  }
}

// What a consumer sent a session to hold — a co-work result among it — is folded away with the detail,
// so the fold that reached it is published for the consumer to let its own record of it go.
fn forget_loaded(ground: &Ground, session: &str) {
  ground.events.publish(SessionFolded {
    session: session.to_owned(),
  });
}

// The record a session's turns are read out of, under the login its program goes out on.
fn record_of(
  ground: &Ground,
  env: &std::collections::BTreeMap<String, String>,
  cwd: &str,
  session: &str,
) -> PathBuf {
  let dir = env
    .get("CLAUDE_CONFIG_DIR")
    .filter(|dir| !dir.is_empty())
    .map_or_else(|| ClaudeCode::machine_config_dir(&ground.machine()), PathBuf::from);

  ClaudeCode::records_under(&dir, Path::new(cwd), session).swap_remove(0)
}

/// The worker's session on `model` and the login `account` names, folded; what came of it, also told to
/// the machine's log.
pub async fn fold(ground: &Ground, row: &AgentRow, model: &str, account: &str, session: &str) -> Folded {
  let worker = &row.name;
  let (outcome, detail) = match folded(ground, row, model, account, session).await {
    Ok(done) => done,
    Err(why) => {
      eprintln!("compact {worker} {model}: failed — {}", why.detail);
      // A fold that broke off may have reached the session all the same.
      forget_loaded(ground, session);

      return Folded::Failed;
    }
  };

  println!(
    "compact {worker} {model}: {}{}",
    outcome.word(),
    if detail.is_empty() { String::new() } else { format!(" — {detail}") }
  );

  outcome
}

async fn folded(
  ground: &Ground,
  row: &AgentRow,
  model: &str,
  account: &str,
  session: &str,
) -> Result<(Folded, String), Refusal> {
  let worker = &row.name;
  let named = account_named(ground, account).await?;
  let recorded = named.as_ref().map_or_else(|| SEED_ACCOUNT.to_owned(), |account| account.id.clone());

  if !provider_enabled(&ground.db, MODE).await.map_err(|refused| fault(refused.error))? {
    return Ok((Folded::Busy, String::new()));
  }

  // The fold operates on the session's own configuration, as its held record names it, and goes back to
  // the login it was opened under.
  let cwd = ground.checkout_of(row);
  let trees = Trees::of(ground);
  let kept = held::held(ground, &scope_of(worker)).await?;

  if let Some(kept) = &kept
    && (kept.id.as_deref() != Some(session) || kept.mode != MODE)
  {
    return Err(fault("Compaction session identity changed"));
  }

  let login = match kept.as_ref().and_then(|kept| kept.account.clone()).filter(|id| !id.is_empty()) {
    Some(id) => account_named(ground, &id).await?,
    None => named,
  };
  let mut line = worker_run(ground, &trees, Some(worker), login.as_ref(), model, &cwd).await?;

  if let Some(kept) = &kept {
    line.model.clone_from(&kept.model);
    line.effort.clone_from(&kept.effort);
    line.tier = kept.tier.clone().and_then(|tier| tier.as_str().map(str::to_owned));
  }

  // A login that is off, at its limit or waiting for a person would only fail the fold, which waits for it.
  if let Some(held) = &login
    && !crate::control::claude_accounts::account_open(
      crate::control::claude_accounts::Judging::of(ground),
      &held.id,
      crate::now_ms(),
    )
    .await
    .map_err(|refused| fault(refused.error))?
  {
    return Ok((Folded::Busy, String::new()));
  }

  if line.tier.is_none() {
    line = configured(ground, &ground.machine(), line, MODE).await?;
  }

  let turn = compose(
    ground,
    row,
    &line,
    login.as_ref(),
    Some(session),
    Composed {
      name: format!("bmo-fold-{worker}"),
      title: DOOR_WORDS.fold_title.clone(),
      prompt: FOLD_WORD.to_owned(),
      clock: None,
    },
  )
  .await?;
  let mut tail = Tail::at_end(record_of(ground, &turn.env, &turn.cwd, session));
  let from = tail.count();
  let ending = put(ground, &turn, &mut Quiet).await.map_err(fault)?;
  let written = tail.look();
  let compacted = compacted_since(written, from);
  let thin = command_words(written, from).is_some_and(|said| TOO_SHORT.is_match(&said.text));
  let (spend, answered_by) = match &ending {
    Ending::Answered { reply, .. } => (reply.spend.clone(), reply.account.clone()),
    _ => (vec![], None),
  };

  note_spend(&ground.events, "compact", &spend, &answered_by.unwrap_or_else(|| recorded.clone()));

  let (outcome, detail) = if compacted {
    (Folded::Compacted, String::new())
  } else if thin {
    (Folded::TooShort, String::new())
  } else {
    let detail = format!("the fold turn ended {}: {}", ending.word(), ending.detail());

    (
      if matches!(ending, Ending::Blocked { .. } | Ending::Stopped { .. }) {
        Folded::Busy
      } else {
        Folded::Failed
      },
      detail,
    )
  };

  if !matches!(outcome, Folded::Busy | Folded::TooShort) {
    forget_loaded(ground, session);
  }

  if outcome == Folded::Compacted {
    let digest = worker_prefix_digest(ground, &trees, &cwd, &line.model, Some(&line.effort)).await?;
    // The fold ran on the model the session stands on, so a mark it carried still holds.
    let answered = records::sessions(&ground.db, worker)
      .await?
      .into_iter()
      .find(|one| one.model == model && one.account == recorded)
      .and_then(|one| one.answered_model);

    record(&ground.db, worker, model, &recorded, session, Some(&digest), answered.as_deref()).await?;
  }

  Ok((outcome, detail))
}
