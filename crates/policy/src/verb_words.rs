//! What agency's verbs print to a person at a shell: one section a verb, each `{name}` filled in by the
//! verb.

use serde::Deserialize;

/// One Claude account's token for the work nodes, made and kept on this machine by
/// `bmo-agency node claude-token` (docs/work-nodes.md, 「The logins on a node」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeTokenWords {
  pub unknown_account: String,
  pub endpoint_account: String,
  pub making: String,
  pub approve: String,
  pub approve_as: String,
  pub sign_in_failed: String,
  pub paste: String,
  pub stopped: String,
  pub refused: String,
  pub not_a_token: String,
  pub cut: String,
  pub kept: String,
  pub from_next_run: String,
}

/// The listing: the latest choice for each engine, then each entry over five lines.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelsListingWords {
  pub use_line: String,
  pub serving: String,
  pub file: String,
  pub where_line: String,
  pub defaults: String,
  pub thinking_on: String,
  pub thinking_off: String,
  pub note: String,
}

/// A choice carried out by its unit: the step it is at while the engine's installer runs, a choice that is
/// not running, {id}, and an installer that failed saying nothing.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelsRunWords {
  pub installing: String,
  pub not_running: String,
  pub failed: String,
}

/// The model catalogue as `bmo-agency model` tells it (docs/model-catalogue.md, 「Where it is set」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelsWords {
  pub usage: String,
  pub not_held: String,
  pub already_held: String,
  pub placed: String,
  pub removed: String,
  pub choosing: String,
  pub listing: ModelsListingWords,
  pub run: ModelsRunWords,
}

/// What a node's workspace in herdr shows (docs/work-nodes.md, 「In herdr」): the node as the node service
/// lists it, how it is reached, and what Enter does there.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodePaneWords {
  pub kind: String,
  pub state: String,
  pub detail: String,
  pub ssh_enter: String,
  pub no_login: String,
  pub gone: String,
}

/// A session left as it stands: no prefix word on its row, or a prefix that has not moved.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerFoldUntouchedWords {
  pub no_record: String,
  pub same: String,
}

/// What became of one fold: a line after the session's {name}, and the word the tally counts it under.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerFoldOutcomesWords {
  pub compacted: String,
  pub too_short: String,
  pub busy: String,
  pub failed: String,
}

/// The run's last line: nothing found, {count} sessions with each outcome counted, and the {count} left
/// standing because their prefix had not moved.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerFoldTallyWords {
  pub none: String,
  pub sessions: String,
  pub part: String,
  pub left: String,
  pub due: String,
  pub words: WorkerFoldOutcomesWords,
}

/// What `bmo-agency worker fold` says of the job workers' sessions it folds after a prefix change
/// (docs/agent-control.md): each line under its head, what it leaves alone and why, what it would fold,
/// what became of each fold, and the run's last line. {name} is a session's worker and model, {session} its
/// id, {count} how many.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerFoldWords {
  pub line: String,
  pub due: String,
  pub untouched: WorkerFoldUntouchedWords,
  pub outcomes: WorkerFoldOutcomesWords,
  pub tally: WorkerFoldTallyWords,
}

/// Why a base {base} cannot have its checkout on a node {node}, or lose it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentsNodeCheckoutWords {
  pub not_base: String,
  pub base_on_node: String,
  pub already: String,
  pub name_long: String,
  pub name_taken: String,
  pub path: String,
  pub missing: String,
  pub busy: String,
  pub not_removed: String,
}

/// Why a worker {worker} cannot stand on a node {node}.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentsNodePlacingWords {
  pub no_node: String,
  pub no_jobs: String,
  pub builtin: String,
  pub support: String,
  pub has_support: String,
  pub path: String,
}

/// The agents and their settings as `bmo-agency agent` tells them (docs/agent-control.md,
/// docs/worker-slots.md, docs/work-nodes.md, 「A checkout on a node」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentsWords {
  pub provider_order: String,
  pub provider_unknown: String,
  pub key_usable: String,
  pub key_none: String,
  pub key_unusable: String,
  pub on_off: String,
  pub heal_on_off: String,
  pub no_account: String,
  pub credential_kind: String,
  pub account_add: String,
  pub account_set: String,
  pub models_shape: String,
  pub no_worker: String,
  pub no_repository: String,
  pub support_count: String,
  pub support_usage: String,
  pub back_here_path: String,
  pub nothing_to_change: String,
  pub pre_gate: String,
  pub post_gate: String,
  pub node_checkout: AgentsNodeCheckoutWords,
  pub node_placing: AgentsNodePlacingWords,
}

/// A worker's dev server as `bmo-agency dev-server`, `bmo-agency dev-server list` and the dev-server pane
/// tell it (docs/worker-bench.md, 「Dev server」).
/// `bmo-agency dev-server start|restart|stop <worker>`: {worker} the worker, {group} the server's process
/// group, {log} its log.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevServersServerWords {
  pub no_worker: String,
  pub not_up: String,
  pub down: String,
  pub no_command: String,
  pub no_job: String,
  pub already_up: String,
  pub not_started: String,
  pub up: String,
  pub started: String,
  pub restarted: String,
}

/// `bmo-agency dev-server list [--sweep]`: every worker's dev server and every run's record with what the
/// run still holds, with the memory each holds.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevServersListingWords {
  pub head: String,
  pub worker: String,
  pub job_running: String,
  pub no_job: String,
  pub left_head: String,
  pub run: String,
  pub program_running: String,
  pub program_ended: String,
  pub none: String,
  pub total: String,
  pub nothing_to_end: String,
}

/// The dev-server pane, line by line as it writes to its screen and its log.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevServersPaneWords {
  pub head: String,
  pub leftover: String,
  pub start: String,
  pub ended: String,
  pub quick: String,
  pub job_over: String,
  pub not_asked: String,
  pub no_command: String,
  pub closing: String,
  pub stopping: String,
  pub pressed: String,
  pub restarting: String,
  pub moved: String,
  pub starting_now: String,
  pub pane_closed: String,
}

/// A worker's dev server, as its verbs and its pane tell it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevServersWords {
  pub server: DevServersServerWords,
  pub listing: DevServersListingWords,
  pub pane: DevServersPaneWords,
}

/// A supervisor's session in a room followed as its standing pane prints it: an {agent} not in the agents
/// table, a {room} it holds no session in with the {rooms} it does, each turn's heading and each
/// generation's.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorFollowWords {
  pub usage: String,
  pub unknown_agent: String,
  pub no_session: String,
  pub turn: String,
  pub room: String,
  pub generation: String,
  pub generation_opened: String,
}

/// How much memory each program on this machine holds, as `bmo-agency machine memory` prints it
/// (docs/agent-control.md, 「What each program holds」): the machine's figures, the processes counted, the
/// table of each runtime's programs and of each unit's, and an empty one.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineMemoryWords {
  pub machine: String,
  pub counted: String,
  pub runtimes: String,
  pub programs: String,
  pub none: String,
  pub head: String,
}

/// A supervisor started afresh from the shell, as `bmo-agency supervisor reset` prints it
/// (docs/supervisors.md, 「Starting it afresh」): its heading, its rooms, its rows, its files, what the brain
/// would take, and what was left.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorResetWords {
  pub no_token: String,
  pub unanswered: String,
  pub refused: String,
  pub head: String,
  pub preview: String,
  pub run: String,
  pub conversation: String,
  pub person: String,
  pub alone: String,
  pub shared: String,
  pub none: String,
  pub room: String,
  pub room_untitled: String,
  pub rows: String,
  pub row: String,
  pub files: String,
  pub file: String,
  pub brain: String,
  pub failure: String,
}

/// What is running and what is waiting, as `bmo-agency run list` prints it
/// (docs/agent-control.md, 「Reading everything running at once」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunListWords {
  pub runs: String,
  pub run: String,
  pub executions: String,
  pub execution: String,
  pub jobs: String,
  pub job: String,
  pub panes: String,
  pub pane: String,
  pub panes_unreached: String,
  pub panes_unnamed: String,
  pub machine: String,
  pub machine_line: String,
  pub none: String,
}

/// Stopping a run from the shell, as `bmo-agency run stop` prints it
/// (docs/agent-control.md, 「Stopping one from a shell」): what is listed with every target, and what a
/// stop reached.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunStopWords {
  pub bad_target: String,
  pub unknown: String,
  pub no_reason: String,
  pub nothing: String,
  pub left_out: String,
  pub asked: String,
  pub confirmed: String,
  pub not_confirmed: String,
  pub all_done: String,
  pub row: String,
}

/// Every verb's words.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerbWords {
  pub claude_token: ClaudeTokenWords,
  pub overtime: OvertimeWords,
  pub models: ModelsWords,
  pub node_pane: NodePaneWords,
  pub worker_fold: WorkerFoldWords,
  pub agents: AgentsWords,
  pub dev_servers: DevServersWords,
  pub supervisor_follow: SupervisorFollowWords,
  pub supervisor_reset: SupervisorResetWords,
  pub machine_memory: MachineMemoryWords,
  pub run_list: RunListWords,
  pub run_stop: RunStopWords,
}

/// The supervisor's verdict on a run past its clock (docs/agent-control.md, 「A clock asks, it never cuts」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OvertimeWords {
  pub usage: String,
  pub length: String,
  pub none_open: String,
  pub open: String,
  pub kept: String,
  pub cut: String,
  pub unknown: String,
  pub closed: String,
}

crate::part!(pub VERB_WORDS: VerbWords = "verb-words.toml");
