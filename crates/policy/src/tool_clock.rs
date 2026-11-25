//! The clock a run's tool calls are held to (docs/agent-control.md, 「A run's tool calls are held to a
//! clock」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolClock {
  pub call: CallClock,
  pub turn: TurnClock,
  pub stall: StallClock,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallClock {
  /// How long a call goes before the clock watches it at all.
  pub min_seconds: u64,
  /// How long a call's process tree may burn no processor time, begin no process and write no output
  /// before the call is stopped.
  pub stall_seconds: u64,
  /// How often the call's process tree is read.
  pub look_seconds: u64,
  /// How long a call with no process of its own may run before it is cut.
  pub in_process_seconds: u64,
  /// After such a call is cut, how long it may still stand unanswered before the turn is ended.
  pub in_process_grace_seconds: u64,
  /// The words put into the working turn when such a call is cut.
  pub in_process_cut: String,
  /// Why a job's turn was cut at the grace after a cut call did not take the words.
  pub in_process_grace_cut: String,
  /// When the first still-running line is written for a call.
  pub log_after_seconds: u64,
  /// How often the still-running line is repeated.
  pub still_every_seconds: u64,
  /// On a stop, how long a running call may burn no processor time before it is stopped.
  pub stop_stall_seconds: u64,
  /// A stop the job has not ended with itself within this long is ended by the interrupt.
  pub stop_bound_seconds: u64,
  /// The rule that asks the supervisor about a call gone silent and idle.
  pub silent: SilentCallClock,
}

/// A call that owns processes, wrote no output for its window and burned under its idle share is asked
/// about, never cut by the clock.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SilentCallClock {
  /// How long a call may write no output with its processes idle before it is asked about.
  pub window_seconds: u64,
  /// The share of one processor, in percent across the window, under which a call's processes are idle.
  pub idle_cpu_percent: u64,
  /// How many characters of the call's command and of its output's last line an ask carries.
  pub ask_chars: usize,
  /// What an ask says the call is; `{job}`, `{item}`, `{tool}`, `{cpu}` and `{minutes}`, its silence as
  /// counted, are filled.
  pub what: String,
  /// What an ask says the call is where its silence counts from the job's own work it waited on; the marks
  /// of `what` and `{awaited}` are filled.
  pub what_awaited: String,
  /// The job's own work in the background a call waited on; `{path}` and `{at}` are filled.
  pub awaited_work: String,
  /// The job's own question a call waited on; `{question}` and `{at}` are filled.
  pub awaited_question: String,
  /// The last activity an ask names for a call that has written nothing.
  pub nothing_written: String,
  /// The words put into the working turn when the supervisor cuts such a call; `{tool}`, `{seconds}` and
  /// `{why}` are filled.
  pub cut: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnClock {
  /// A job turn past this clock is cut and its item released.
  pub bound_minutes: u64,
  /// Why a job's turn was cut at its bound; `{minutes}` is the bound in minutes.
  pub cut: String,
}

/// A job's turn that has made no progress for its span is told to its item's orderer, never cut
/// (docs/agent-control.md, 「A run's tool calls are held to a clock」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StallClock {
  /// How long a job's turn may make no progress before it is told.
  pub after_seconds: u64,
  /// How many of the turn's own last lines, and of a call's last output lines, a new one is compared with.
  pub recent: usize,
  /// How many last lines of what a call waits on the notice carries.
  pub lines: usize,
  /// How many characters of a call's command and of each of its lines the notice carries.
  pub chars: usize,
}

crate::part!(pub TOOL_CLOCK: ToolClock = "tool-clock.toml");
