//! What a work order's editing answers the owner and the chat agent with (docs/work-orders.md), each line with the
//! `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkOrderEdits {
  pub plan: Plan,
  pub steps: Steps,
  pub orders: Orders,
  pub script: Script,
}

/// A plan that does not read.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
  pub not_object: String,
  pub empty: String,
  pub stages: String,
  pub stage_tasks: String,
  pub stage_title: String,
  pub task_title: String,
  pub role: String,
  pub model: String,
  pub model_form: String,
  pub task_instruction: String,
  pub builds: String,
  pub build_instruction: String,
}

/// A step verb the order's state does not allow.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Steps {
  pub no_task: String,
  pub control: String,
  pub finished: String,
  pub draft: String,
  pub closing: String,
  pub instruction_size: String,
  pub model_tier: String,
  pub coworker_tier: String,
  pub history_edit: String,
  pub history_rerun: String,
  pub history_restore: String,
  pub running_edit: String,
  pub done_edit: String,
  pub nothing_to_edit: String,
  pub skip_reason: String,
  pub skipped_already: String,
  pub running_builder: String,
  pub not_yet: String,
  pub stage_running: String,
  pub build_running: String,
  pub research_over: String,
  pub phase: String,
  pub running: String,
  pub pending: String,
  pub not_skipped: String,
  pub same_anchor: String,
  pub add_anchor: String,
  pub build_move: String,
  pub task_move: String,
  pub stage_move_past: String,
  pub stage_move_pending: String,
  pub move_kinds: String,
  pub stop_reason: String,
  pub close_reason: String,
  pub close_builder: String,
  pub recover_done: String,
  pub recover_no_jobs: String,
  pub recover_known: String,
  pub recover_running: String,
  pub recover_not_step: String,
  pub recover_unread: String,
  pub recover_twice: String,
  pub recover_none: String,
  pub recover_no_builder: String,
}

/// The control route's verbs on a work order and an item.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Orders {
  pub paused: String,
  pub not_text: String,
  pub missing: String,
  pub still_running: String,
  pub document_there: String,
  pub reopen_done: String,
  pub draft_only: String,
  pub order_and_model: String,
  pub approved_only: String,
  pub active: String,
  pub done: String,
  pub model: String,
  pub chat: String,
  pub not_checkout: String,
  pub not_checkout_run: String,
  pub item_number: String,
  pub no_item: String,
  pub resume_held: String,
  pub resume_paused: String,
  pub before_after: String,
  pub checkout_done: String,
  pub job_range: String,
  pub action: String,
  pub stop_done: String,
}

/// The work order script's own flags and a bot it could not reach.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
  pub value: String,
  pub verbs: String,
  pub recover: String,
  pub stop_no_token: String,
  pub stop_unreachable: String,
  pub stop_refused: String,
  pub edit_no_token: String,
  pub edit_unreachable: String,
  pub edit_refused: String,
}

crate::part!(pub WORK_ORDER_EDITS: WorkOrderEdits = "work-order-edits");
