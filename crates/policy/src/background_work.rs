//! A supervisor's background work (docs/supervisors.md, 「A supervisor's long work」): how often its places are
//! read, how a stop ends it, and the words the room's wake carries.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackgroundWork {
  /// How often every supervisor's place is read.
  pub look_seconds: u64,
  /// How much of a finished notice's result the wake carries, in characters.
  pub result_chars: usize,
  /// How long a stop gives the work it ends after the polite end before killing what stands.
  pub stop_grace_ms: u64,
  /// How long the whole ending on a stop may take.
  pub stop_bound_seconds: u64,
  /// How long after the first stop of its session that leaves a running task out it is taken as dropped, where no
  /// notice of it came.
  pub gone_seconds: u64,
  pub words: BackgroundWords,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackgroundWords {
  /// The wake once the work ended; `{launched}`, `{kind}`, `{description}`, `{status}`, `{cuts}`, `{result}`
  /// and `{output}` are filled.
  pub ended: String,
  /// The result where the work handed back nothing.
  pub nothing_handed: String,
  /// The wake once the work was lost; `{launched}`, `{kind}`, `{description}`, `{why}` and `{cuts}` are filled.
  pub lost: String,
  /// Why it was lost where its program ended or was stood afresh.
  pub program_gone: String,
  /// Why it was lost where a stop ended it.
  pub stopped: String,
  /// What a background shell command is called.
  pub shell: String,
  /// What a background subagent is called.
  pub subagent: String,
  /// One line for each call the clock cut; `{call}`, `{minutes}` and `{why}` are filled.
  pub cut: String,
  /// Why a call was cut for its stall: its processes burned no processor time, began no process and wrote
  /// no output.
  pub stalled: String,
  /// Why a call was cut that wrote nothing and stood idle.
  pub idle: String,
  /// The wake's one line once the work ended; `{description}` is filled.
  pub summary_ended: String,
  /// The wake's one line once the work was lost; `{description}` is filled.
  pub summary_lost: String,
}

crate::part!(pub BACKGROUND_WORK: BackgroundWork = "background-work.toml");
