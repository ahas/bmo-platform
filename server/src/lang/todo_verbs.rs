//! The answers and refusals of the TODO verbs as a person or the chat agent reads them (docs/todo.md), each line
//! with the `{name}` marks its file names; run inside a job, the same verbs answer in the worker's English.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
  pub both: String,
  pub after: String,
  pub after_itself: String,
  pub after_finished: String,
  pub no_commits: String,
  pub not_commit: String,
  pub not_on_main: String,
  pub live: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notes {
  pub none: String,
  pub withdrawn: String,
  pub withdrawn_note: String,
  pub removed: String,
  pub removed_delivered: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
  pub nothing: String,
  pub title: String,
  pub intent: String,
  pub order: String,
  pub done: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Queue {
  pub paused: String,
  pub resumed: String,
  pub hold_stands: String,
  pub no_hold: String,
  pub hold_released: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hold {
  pub after: String,
  pub held: String,
  pub stop_why: String,
  pub stopped: String,
  pub wind_down_why: String,
  pub wound_down: String,
  pub wound_down_idle: String,
  pub wound_down_running: String,
  pub stop_here_report: String,
  pub stop_here_closed: String,
  pub stopped_here: String,
  pub job_unhold: String,
  pub unheld: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Merge {
  pub usage: String,
  pub merged: String,
  pub unmerged: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Close {
  pub resolve_why: String,
  pub report: String,
  pub unfinished: String,
  pub written: String,
  pub written_joint: String,
  pub carried: String,
  pub handed: String,
  pub resumable: String,
  pub reopened: String,
  pub reopened_unmerged: String,
  pub reopened_merged: String,
  pub reopened_sources: String,
  pub live_claude_heading: String,
  pub live_claude_line: String,
  pub settled: String,
  pub item_word: String,
  pub merged_report: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
  pub mark_status: String,
  pub worker_usage: String,
  pub worker_support: String,
  pub worker_cleared: String,
  pub worker_set: String,
  pub repairs_with_add: String,
  pub repairs_usage: String,
  pub repairs_cleared: String,
  pub repairs_set: String,
  pub cowork_usage: String,
  pub cowork_on: String,
  pub cowork_off: String,
  pub impact_usage: String,
  pub impact_set: String,
  pub impact_new_home: String,
  pub impact_single: String,
  pub impact_cowork: String,
  pub impact_on: String,
  pub impact_off: String,
  pub impact_check_kept: String,
  pub impact_check_told: String,
  pub impact_check_differs: String,
  pub impact_check_unfiled: String,
  pub priority_usage: String,
  pub model_needed: String,
  pub model_set: String,
  pub model_wrong: String,
  pub no_verb: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Add {
  pub judgement_needs: String,
  pub missing: String,
  pub reply_part: String,
  pub filed: String,
  pub raised: String,
  pub raised_sources: String,
  pub raised_source: String,
  pub raised_reopened: String,
  pub cowork: String,
  pub wording: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Append {
  pub empty: String,
  pub told: String,
  pub own: String,
  pub fact_idle: String,
  pub fact_running: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Brief {
  pub not_bmo: String,
  pub no_change: String,
  pub empty: String,
  pub not_english: String,
  pub idle: String,
  pub running: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approve {
  pub noted: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindDown {
  pub finished: String,
  pub deployed: String,
  pub resume_plain: String,
  pub resume_condition: String,
  pub note: String,
  pub stopped_here_mark: String,
  pub reason_korean: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TodoVerbWords {
  pub condition: Condition,
  pub notes: Notes,
  pub edit: Edit,
  pub queue: Queue,
  pub hold: Hold,
  pub merge: Merge,
  pub close: Close,
  pub settings: Settings,
  pub add: Add,
  pub append: Append,
  pub brief: Brief,
  pub approve: Approve,
  pub wind_down: WindDown,
  pub usage: String,
}

crate::part!(pub TODO_VERBS: TodoVerbWords = "todo-verbs");
