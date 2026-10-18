//! What the deployment's verbs tell the owner in their own chat, on the road that needs no bot
//! (docs/deploy.md), each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentWords {
  pub rollback: RollbackWords,
  pub down: DownWords,
  pub suite: SuiteWords,
  pub backlog: BacklogWords,
}

/// What the owner is told of a deployment whose bot did not come up, and the reason the bot that comes
/// back says.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackWords {
  pub restart_refused: String,
  pub not_answering: String,
  pub died_after_start: String,
  pub archive_short: String,
  pub untouched: String,
  pub answers: String,
  pub silent: String,
  pub first: String,
  pub before_start: String,
  pub tree: String,
  pub bot_down: String,
  pub cause: String,
  pub output: String,
  pub reason: String,
}

/// The line the owner reads when systemd has given up on the bot's unit.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DownWords {
  pub given_up: String,
  pub failed: String,
  pub exit: String,
  pub last_error: String,
  pub deploying_unread: String,
  pub deploying: String,
  pub look: String,
}

/// What the chat agent is told of a hand deployment that put nothing live.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteWords {
  pub refused_unread: String,
  pub refused: String,
  pub check_failed: String,
  pub suite_failed: String,
  pub files: String,
  pub failed: String,
  pub locked: String,
}

/// What the chat agent is told of the backlog's deployment, and what the owner is told when the bot does not stand.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BacklogWords {
  pub item: String,
  pub no_item: String,
  pub running: String,
  pub running_unread: String,
  pub passing: String,
  pub waiting_gap: String,
  pub hold_released: String,
  pub gap_missed: String,
  pub gap_missed_unread: String,
  pub gap_missed_running: String,
  pub batch_failed: String,
  pub check_broke: String,
  pub suite_unstarted: String,
  pub suite_broke: String,
  pub broken_files: String,
  pub batch_cut: String,
  pub first_job: String,
  pub first_broke: String,
  pub cut_broke: String,
  pub main_ahead: String,
  pub main_ahead_more: String,
  pub not_contained: String,
  pub deploy_failed: String,
  pub deployed: String,
  pub deployed_items: String,
  pub still_waiting: String,
  pub still_waiting_last: String,
  pub warnings: WarningWords,
  pub bot_down_owner: String,
  pub bot_down: String,
  pub taken: TakenWords,
  pub gap: GapWords,
}

/// What the batch's check printed as warnings, beside the last deployment whose check ran.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WarningWords {
  pub counted: String,
  pub before: String,
  pub unrecorded: String,
  pub new: String,
  pub more: String,
  pub seen: String,
}

/// What a batch that takes something away is said to take.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TakenWords {
  pub breaks_unread: String,
  pub breaks: String,
  pub moved_unread: String,
  pub moved: String,
}

/// Why the deployment waits for a job at work, or lets it go on.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GapWords {
  pub breaks_unread: String,
  pub breaks_every_runner: String,
  pub moved_unread: String,
  pub processes_unread: String,
  pub pinned: String,
  pub reach_unread: String,
  pub reaches: String,
  pub reaches_none: String,
}

crate::part!(pub DEPLOYMENT: DeploymentWords = "deployment");
