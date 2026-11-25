//! The handoff a supervisor's session going away is asked for before a fresh session replaces it
//! (docs/sessions.md, 「A restart hands the session over」): the receiver's `POST /handoffs/{agentName}`,
//! asked while the old program still stands in its pane and waited on up to the policy's ceiling; a handoff
//! already under way waited on and asked again; anything else let go, the turn going on.

use std::collections::BTreeMap;
use std::time::Duration;

use policy::handoff::HANDOFF;
use serde_json::{Value, json};

use crate::job_end::{address, token};

/// What the ask for a handoff came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Handed {
  /// The old session wrote its handoff file: where, and how long it is.
  Written { path: String, size: i64 },
  /// There was nothing to ask, or the session wrote nothing.
  Nothing,
  /// The deployment names no receiver, so nothing was asked.
  NoReceiver,
  /// The ask failed, was refused or ran past its bound; the turn goes on.
  Failed(String),
}

impl Handed {
  /// The line the turn's run log says it in.
  pub fn said(&self) -> String {
    match self {
      Self::Written { path, size } => {
        format!("The session going away wrote its handoff: {path} ({size} bytes).")
      }
      Self::Nothing => "The session going away had no handoff to write.".to_owned(),
      Self::NoReceiver => "No receiver is named, so no handoff was asked.".to_owned(),
      Self::Failed(why) => {
        format!("The handoff was not had, and the fresh session opens all the same: {why}")
      }
    }
  }
}

/// Whether a turn's session replaces one the agent had: it opens fresh — it resumes nothing — where the
/// agent kept a session, or its program still stands in its pane on one.
pub fn replaces(kept: bool, resuming: bool, standing: bool) -> bool {
  !resuming && (kept || standing)
}

/// Why a supervisor's session goes, as the receiver's room line names it (docs/sessions.md, 「A restart hands
/// the session over」, what the ask carries).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
  /// A reset of the session was asked.
  Reset,
  /// The documents its prompt is composed from moved.
  Documents,
  /// Its road moved, or a fresh session opens in place of one on another road.
  Road,
  /// Its window filled.
  Full,
  /// The session it had is gone.
  Other,
}

impl Cause {
  /// The word the receiver reads.
  pub fn word(self) -> &'static str {
    match self {
      Self::Reset => "reset",
      Self::Documents => "documents",
      Self::Road => "road",
      Self::Full => "full",
      Self::Other => "other",
    }
  }
}

/// One ask of the receiver's `POST /handoffs/{agentName}`: the reason for its journal, the room, the cause its
/// line names, whether the reset is clean — the session consolidating and writing no note — and the account
/// that asked where one did.
#[derive(Clone, Copy, Debug)]
pub struct Asking<'a> {
  pub reason: &'a str,
  pub room: Option<i64>,
  pub cause: Cause,
  pub clean: bool,
  pub by: Option<i64>,
}

impl Asking<'_> {
  // The body the receiver is handed, `ask` false where no session is to be asked and the line alone written.
  fn body(&self, ask: bool) -> Value {
    let mut body = json!({ "reason": self.reason, "cause": self.cause.word() });

    if let Some(room) = self.room {
      body["room"] = json!(room);
    }

    if let Some(by) = self.by {
      body["by"] = json!(by);
    }

    if self.clean {
      body["clean"] = json!(true);
    }

    if !ask {
      body["ask"] = json!(false);
    }

    body
  }
}

/// The receiver told of a handover no session is asked for — a room's own session agency handed over itself, or
/// a session gone with no program left to ask — so the room shows its line (docs/sessions.md, 「A restart hands
/// the session over」). What came of it is said in the journal alone.
pub async fn tell(vars: &BTreeMap<String, String>, agent: &str, asking: Asking<'_>) {
  let (Some(url), Some(token)) = (
    address(vars, "BMO_RECEIVER_URL"),
    token(vars, "BMO_RECEIVER_TOKEN", "BMO_RECEIVER_TOKEN_FILE"),
  ) else {
    return;
  };
  let told = reqwest::Client::new()
    .post(format!("{url}/handoffs/{agent}"))
    .bearer_auth(&token)
    .json(&asking.body(false))
    .timeout(Duration::from_secs(HANDOFF.retry_seconds.max(1) * 6))
    .send()
    .await;

  match told {
    Ok(answer) if answer.status().is_success() => {}
    Ok(answer) => {
      eprintln!("{agent}: the receiver was not told of the handover: it answered {}", answer.status())
    }
    Err(why) => eprintln!("{agent}: the receiver was not told of the handover: {why}"),
  }
}

/// The receiver asked for `agent`'s handoff, as `asking` says, under the deployment's address and token, and
/// waited on for its answer: a 409 waited out `retry` and asked again, and an error answer or a refused or
/// broken connection ends the ask without a handoff. The caller bounds the wait.
pub async fn ask(
  vars: &BTreeMap<String, String>,
  agent: &str,
  asking: Asking<'_>,
  retry: Duration,
) -> Handed {
  let (Some(url), Some(token)) = (
    address(vars, "BMO_RECEIVER_URL"),
    token(vars, "BMO_RECEIVER_TOKEN", "BMO_RECEIVER_TOKEN_FILE"),
  ) else {
    return Handed::NoReceiver;
  };
  let body = asking.body(true);

  loop {
    let answer = reqwest::Client::new()
      .post(format!("{url}/handoffs/{agent}"))
      .bearer_auth(&token)
      .json(&body)
      .send()
      .await;
    let answer = match answer {
      Ok(answer) => answer,
      Err(why) => return Handed::Failed(why.to_string()),
    };

    match answer.status().as_u16() {
      200 => {
        let read: Value = answer.json().await.unwrap_or(Value::Null);

        return Handed::Written {
          path: read["path"].as_str().unwrap_or_default().to_owned(),
          size: read["bytes"].as_i64().unwrap_or_default(),
        };
      }
      204 => return Handed::Nothing,
      409 => tokio::time::sleep(retry).await,
      status => return Handed::Failed(format!("the receiver answered {status}")),
    }
  }
}

/// The handoff asked as the policy paces and bounds it: an ask the receiver has not answered within the
/// policy's ceiling was not had, so a handoff that waits on something that will not come never holds the turn.
pub async fn asked(vars: &BTreeMap<String, String>, agent: &str, asking: Asking<'_>) -> Handed {
  let ceiling = Duration::from_secs(HANDOFF.wait_seconds);
  let asking = ask(vars, agent, asking, Duration::from_secs(HANDOFF.retry_seconds));

  tokio::time::timeout(ceiling, asking).await.unwrap_or_else(|_| {
    Handed::Failed(policy::fill_marks(
      &policy::supervisor_runner::SUPERVISOR_RUNNER.log.handoff_waited_out,
      &[("seconds", &HANDOFF.wait_seconds.to_string())],
    ))
  })
}
