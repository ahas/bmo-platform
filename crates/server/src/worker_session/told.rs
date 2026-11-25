//! What a worker's session tells its consumers (docs/api.md, 「Events」): a session that answered on a
//! lesser model, and a fold that reached a session. Agency publishes them and voices nothing.

use serde::Serialize;

use crate::events::Event;

/// A job's run answered on a lesser model than it was launched on: the checkout it ran in, the role it ran
/// as, the model it was launched on, the models it answered on, whether its kept session was dropped, and the
/// job it ran for where one is known (docs/cache-fallback.md, 「Healing differs for a job」).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerDowngraded {
  pub worker: String,
  /// The repository the checkout is of, none for a checkout no repository holds any longer.
  pub repository: Option<String>,
  pub role: Option<String>,
  pub launched: String,
  pub answered_on: Vec<String>,
  pub dropped: bool,
  pub job: Option<i64>,
}

impl Event for WorkerDowngraded {
  const NAME: &'static str = "worker.changed";
}

/// A fold reached, or may have reached, a worker's session: whatever a consumer sent that session to hold
/// is folded away with it (docs/sessions.md, 「Session compaction」).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFolded {
  pub session: String,
}

impl Event for SessionFolded {
  const NAME: &'static str = "session.folded";
}
