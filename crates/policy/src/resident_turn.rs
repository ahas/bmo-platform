//! One turn put into an agent's standing program (docs/agent-control.md, 「The order for putting in one
//! turn and waiting for it to end」): the lead its words go in behind, and the paces it keeps. None of them
//! ends a turn or a wait in it but the bound a program on a node is held to before it takes its turn.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentTurn {
  /// What the typed lead says about the words pasted after it.
  pub pasted_lead: String,
  /// What it says about a room's round pasted after it: nothing, since a round carries no typed lead.
  pub round_lead: String,
  /// What it says about a run apart's task pasted after it.
  pub apart_lead: String,
  pub poll_ms: u64,
  pub redraw_ms: u64,
  pub screen_answers: u32,
  pub submit_ms: u64,
  /// How often the record of the pane program that carries its own turn is looked at.
  pub inbox_poll_ms: u64,
  /// The most lines of a pane program's error stream one turn's run log takes.
  pub stderr_lines: usize,
  pub gone_looks: u32,
  pub gone_gap_ms: u64,
  pub end_poll_ms: u64,
  /// How long a turn its record closed waits for the reading of its price after its end, before it ends with
  /// none.
  pub status_after_ms: u64,
  /// How full a session's context may be before it is folded once its turn is over, where neither the
  /// fold-ratio table nor the session's provider names a share.
  pub fold_ratio: f64,
  /// How long a turn picked up across agency's restart that no job holds may go from its start before it is
  /// cut.
  pub adopted_bound_minutes: u64,
  /// Why such a turn was cut; `{minutes}` is the bound in minutes.
  pub adopted_cut: String,
  /// How long a program on a node may go without taking its turn, its node's link heard, before the turn's
  /// waiting names it, and before the turn ends on it.
  pub node_take_notice_ms: u64,
  pub node_take_bound_ms: u64,
  /// What the waiting says, and why the turn ends; `{node}` and `{seconds}` are filled in.
  pub node_take_waiting: String,
  pub node_take_cut: String,
  /// How long a program on a person's PC is waited on while the far end there could not listen on a port the
  /// program reaches home through, before the turn ends on it.
  pub node_listen_bound_ms: u64,
  /// What the waiting says, and why the turn ends; `{node}`, `{port}` and `{why}` are filled in.
  pub node_listen_waiting: String,
  pub node_listen_cut: String,
  /// How long a call to a node's herdr is waited on before it is answered as herdr silent.
  pub node_herdr_ceiling_ms: u64,
}

crate::part!(pub RESIDENT_TURN: ResidentTurn = "resident-turn.toml");
