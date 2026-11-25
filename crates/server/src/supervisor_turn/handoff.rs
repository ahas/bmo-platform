//! The session handed over at the handoff mark (docs/supervisors.md, 「Its limit」): one more turn of it, in no
//! round of the room's and on the very run its turns go out as, asking it for its handoff note; and where it
//! wrote one, a new generation holding the note for its first turn. An answer of none, an empty one or a
//! failed turn leaves the session as it was.

use std::time::Instant;

use indexmap::IndexMap;
use policy::fill_marks;
use policy::supervisor_runner::SUPERVISOR_RUNNER;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::going::going;
use super::overtaken::{self, ResetHold};
use super::run::{NoAnswer, Reply, run_turn};
use super::sessions::{self, Kept};
use crate::door::record::UsageSpent;
use crate::lines::pieces::{SupervisorTurn, compose};
use crate::places::Ground;
use crate::run_log::lines::Line;
use crate::run_log::{RunLog, Target};
use crate::text::{length, trim};
use crate::worker_session::records::{note_answer, slot_account};

/// The supervisor a handoff is for: its agent name, the label it is shown under, and the model it is pinned
/// to.
#[derive(Clone)]
pub struct Speaker {
  pub name: String,
  pub label: Option<String>,
  pub pinned_model: Option<String>,
}

impl Speaker {
  /// The name it is shown under.
  pub fn shown(&self) -> &str {
    self.label.as_deref().unwrap_or(&self.name)
  }
}

/// Why a session hands over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Why {
  /// The turn's context reached the session's handoff mark.
  Limit,
  /// The documents the session's prompt is composed from changed.
  #[default]
  Documents,
  /// The road the session was opened on is no longer the one its supervisor answers on.
  Road,
  /// A reset of the session was asked.
  Reset,
  /// A clean reset of the session was asked: it consolidates and writes no note (docs/session-resets.md).
  CleanReset,
}

impl Why {
  /// The cause the receiver's room line names for it.
  pub fn cause(self) -> crate::handoff::Cause {
    match self {
      Self::Limit => crate::handoff::Cause::Full,
      Self::Documents => crate::handoff::Cause::Documents,
      Self::Road => crate::handoff::Cause::Road,
      Self::Reset | Self::CleanReset => crate::handoff::Cause::Reset,
    }
  }
}

/// The receiver told that a room's own session was handed over and its new generation opened, so the room shows
/// its line (docs/sessions.md, 「A restart hands the session over」): the cause, and the account that asked where a
/// reset waiting for the session names one.
pub async fn told(ground: &Ground, agent: &str, chat_id: i64, why: Why) {
  let by = match why {
    Why::Reset | Why::CleanReset => sessions::waiting_reset(&ground.db, agent, Some(chat_id))
      .await
      .ok()
      .flatten()
      .and_then(|reset| reset.by),
    Why::Limit | Why::Documents | Why::Road => None,
  };
  let reason = match why {
    Why::Limit => "the session's window filled",
    Why::Documents => "the documents the session's prompt is composed from changed",
    Why::Road => "the road its session went out on changed",
    Why::Reset => "a reset of its session was asked",
    Why::CleanReset => "a clean reset of its session was asked",
  };
  let asking = crate::handoff::Asking {
    reason,
    room: Some(chat_id),
    cause: why.cause(),
    clean: why == Why::CleanReset,
    by,
  };

  crate::handoff::tell(ground.vars(), agent, asking).await;
}

/// A turn that ended at or past the handoff mark, or a session whose prompt documents moved, whose road moved
/// or whose reset was asked: the session it answered in and what its runs are built from, so the handoff turn
/// after it goes out on the very session and prompt.
pub struct Due {
  pub provider: &'static dyn harness_providers::Provider,
  /// The model the turn it hands over ran on, which the handoff turn goes out on.
  pub model: String,
  pub system: String,
  pub generation: IndexMap<String, Value>,
  pub session: String,
  pub placement: Option<String>,
  /// The share of the window the turn's context reached, where the handoff mark is why it hands over.
  pub ratio: f64,
  pub why: Why,
}

// What the handoff turn spent is the supervisor's own: its last answer written and its usage published.
async fn record_usage(ground: &Ground, agent: &str, reply: &Reply, run: &str) {
  if let Err(why) =
    note_answer(&ground.db, agent, &reply.mode, Some(&reply.model), reply.effort.as_deref()).await
  {
    eprintln!("supervisor {agent}: its last answer was not written: {}", why.detail);
  }

  let account = slot_account(&reply.mode, reply.account.as_deref());

  if let Some(spent) =
    UsageSpent::of(agent, Some(run), crate::now_ms() / 1000, &reply.spend, |_| account.clone())
  {
    ground.events.publish(spent);
  }
}

/// One handoff, written down as a run of its own, published as going among the supervisor's runs while it
/// runs. One its reset overtakes hands nothing over and takes back what it wrote (docs/supervisors.md, 「What the
/// reset cuts off」). A clean reset's turn consolidates and writes no note, and its generation opens on none.
/// Answers whether a new generation opened, which the receiver is then told of.
pub async fn hand_over(
  ground: &Ground,
  hold: &ResetHold,
  supervisor: &Speaker,
  chat_id: i64,
  due: Due,
) -> bool {
  let why = due.why;
  let opened = handed(ground, hold, supervisor, chat_id, due).await;

  if opened {
    told(ground, &supervisor.name, chat_id, why).await;
  }

  opened
}

async fn handed(ground: &Ground, hold: &ResetHold, supervisor: &Speaker, chat_id: i64, due: Due) -> bool {
  let began = crate::now_ms();
  let db = ground.db.clone();
  let name = supervisor.shown();
  let now = crate::now_ms() / 1000;
  let mut log = RunLog::new(Target {
    agent: supervisor.name.clone(),
    run: fill_marks(
      &SUPERVISOR_RUNNER.runs.handoff,
      &[("chat", &chat_id.to_string()), ("at", &now.to_string())],
    ),
    chat_id: Some(chat_id),
    job_id: None,
  })
  .keeping_launch();
  let percent = (due.ratio * 100.0).round().to_string();
  let roots = harness_machine::supervisor_prompt::Roots {
    assets: ground.code_root(),
    data: ground.roots.data.clone(),
  };
  let took = Instant::now();
  let end = |text: String, state: &'static str| Line {
    kind: "end",
    text,
    state: Some(state),
    ..Line::default()
  };

  let clean = due.why == Why::CleanReset;
  let piece = if clean { "handoff-clean.md" } else { "handoff.md" };
  let instruction = match roots.agent_file("pieces", piece) {
    Ok(instruction) => instruction,
    Err(why) => {
      eprintln!("supervisor {} in chat {chat_id}: the handoff instruction: {why}", supervisor.name);
      return false;
    }
  };
  let model = due.model.clone();
  let run_name = log.run().to_owned();
  let machine = harness_providers::Machine::of_process(ground.roots.data.clone());
  // A registration whose engine the arbiter stands is asked a card for the handoff's turn, as every turn on
  // it is (docs/own-llm.md, 「An engine the arbiter stands」).
  let card = match crate::cards::hold_for_model(
    ground,
    due.provider.name(),
    &model,
    &supervisor.name,
    Some(&due.session),
    &mut log,
    None,
  )
  .await
  {
    Ok(card) => card,
    Err(refused) => {
      log.write(&db, end(refused.detail.clone(), "failed")).await;
      eprintln!(
        "supervisor {} in chat {chat_id}: the handoff failed: {}",
        supervisor.name, refused.detail
      );
      return false;
    }
  };
  let composed = compose(
    ground,
    &machine,
    &SupervisorTurn {
      agent: &supervisor.name,
      chat_id,
      provider: due.provider.name(),
      model: &model,
      system: &due.system,
      prompt: &instruction,
      privileged: true,
      resume: Some((&due.session, due.placement.as_deref())),
      card: card.as_ref().map(|card| card.node.as_str()),
      run: &run_name,
      answers: &[],
      generation: &due.generation,
      began,
    },
  )
  .await;

  let opening = match due.why {
    Why::Limit => {
      fill_marks(&SUPERVISOR_RUNNER.log.handoff_start, &[("name", name), ("percent", &percent)])
    }
    Why::Documents => fill_marks(&SUPERVISOR_RUNNER.log.handoff_documents, &[("name", name)]),
    Why::Road => fill_marks(&SUPERVISOR_RUNNER.log.handoff_road, &[("name", name)]),
    Why::Reset => fill_marks(&SUPERVISOR_RUNNER.log.handoff_reset, &[("name", name)]),
    Why::CleanReset => fill_marks(&SUPERVISOR_RUNNER.log.handoff_clean, &[("name", name)]),
  };

  log
    .write(
      &db,
      Line {
        kind: "start",
        text: opening,
        ..Line::default()
      },
    )
    .await;

  let composed = match composed {
    Ok(composed) => composed,
    Err(refused) => {
      if let Some(card) = &card {
        crate::cards::give_back(ground, card).await;
      }

      let _keeping = hold.keeping().await;

      if overtaken::overtaken(&db, &supervisor.name, began).await {
        overtaken::take_back(&db, &run_name).await;
        return false;
      }

      log.write(&db, end(refused.detail.clone(), "failed")).await;
      eprintln!(
        "supervisor {} in chat {chat_id}: the handoff failed: {}",
        supervisor.name, refused.detail
      );
      return false;
    }
  };

  log.launched(&db, Some(&composed.system_path), &instruction, &[]).await;
  println!(
    "supervisor {} in chat {chat_id}: its context reached {percent}% of the window, the session hands itself over",
    supervisor.name
  );

  let _going = going(&ground.events, &supervisor.name, &run_name, chat_id);
  let replied = run_turn(
    ground,
    &supervisor.name,
    crate::lines::chat::LINE,
    due.provider.name(),
    &composed,
    Some(&due.session),
    &mut log,
    true,
    None,
    None,
  )
  .await;

  if let Some(card) = &card {
    crate::cards::give_back(ground, card).await;
  }

  // What the handoff came to is kept under the hold its reset waits on, the point read again under it, as a turn's is.
  let keeping = hold.keeping().await;
  let replied = match replied {
    Ok(_) | Err(NoAnswer::Cut(_) | NoAnswer::Failed(_))
      if overtaken::overtaken(&db, &supervisor.name, began).await =>
    {
      Err(NoAnswer::Overtaken)
    }
    replied => replied,
  };
  let reply = match replied {
    Ok(reply) => reply,
    Err(NoAnswer::Overtaken) => {
      drop(keeping);
      overtaken::take_back(&db, &run_name).await;
      super::run::let_go_overtaken(ground, &supervisor.name, due.provider, &model, &composed.session).await;
      println!("supervisor {} in chat {chat_id}: its reset overtook the handoff", supervisor.name);

      return false;
    }
    Err(
      NoAnswer::Cut(reason)
      | NoAnswer::Stopped(reason)
      | NoAnswer::Failed(reason)
      | NoAnswer::Unsubmitted(reason)
      | NoAnswer::Errored { error: reason, .. },
    ) => {
      log.write(&db, end(reason.clone(), "failed")).await;
      eprintln!("supervisor {} in chat {chat_id}: the handoff failed: {reason}", supervisor.name);
      return false;
    }
  };
  let note = trim(&reply.text).to_owned();

  // What the handoff spent is the supervisor's own, and the turn is the last of the generation it closes.
  record_usage(ground, &supervisor.name, &reply, &run_name).await;

  if let Ok(Some(closing)) = sessions::session(&db, &supervisor.name, chat_id).await {
    let _ = sessions::record_turn(
      &db,
      &closing,
      &[
        Kept {
          role: "user",
          text: &instruction,
        },
        Kept {
          role: "assistant",
          text: &note,
        },
      ],
      None,
      None,
    )
    .await;
  }

  // A clean reset opens its generation on no note, whatever the turn answered.
  if clean {
    return match sessions::open_generation(&db, &supervisor.name, chat_id).await {
      Ok(opened) => {
        let seconds = (took.elapsed().as_millis() as f64 / 1000.0).round().to_string();
        let text = fill_marks(
          &SUPERVISOR_RUNNER.log.started_clean,
          &[("seconds", &seconds), ("generation", &opened.generation.to_string())],
        );

        log.write(&db, end(text, "succeeded")).await;
        true
      }
      Err(why) => {
        log.write(&db, end(why.detail.clone(), "failed")).await;
        eprintln!(
          "supervisor {} in chat {chat_id}: the clean reset failed: {}",
          supervisor.name, why.detail
        );
        false
      }
    };
  }

  if note.is_empty() || note.to_lowercase() == SUPERVISOR_RUNNER.no_note {
    log.write(&db, end(SUPERVISOR_RUNNER.log.no_handoff.clone(), "failed")).await;
    eprintln!(
      "supervisor {} in chat {chat_id}: the session wrote no handoff note, it goes on as it was",
      supervisor.name
    );
    return false;
  }

  match sessions::hand_over(&db, &supervisor.name, chat_id, &note).await {
    Ok(opened) => {
      let seconds = (took.elapsed().as_millis() as f64 / 1000.0).round().to_string();
      let text = fill_marks(
        &SUPERVISOR_RUNNER.log.handed_over,
        &[
          ("chars", &length(&note).to_string()),
          ("seconds", &seconds),
          ("generation", &opened.generation.to_string()),
        ],
      );

      log.write(&db, end(text, "succeeded")).await;
      true
    }
    Err(why) => {
      log.write(&db, end(why.detail.clone(), "failed")).await;
      eprintln!("supervisor {} in chat {chat_id}: the handoff failed: {}", supervisor.name, why.detail);
      false
    }
  }
}
