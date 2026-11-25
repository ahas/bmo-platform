//! The panes a consumer keeps in agency's herdr (docs/api.md, 「Hooks」, the pane claims): a slot it stands
//! an agent in, each claim naming the agent the pane holds, the label its home goes by, and whether it may
//! be taken down now for standing idle. Agency counts a claimed agent as standing, so its leftovers sweep
//! spares it, and on its heads' pass takes down each claim marked idle whose agent nothing is running in.
//! Which panes are whose, and when one has stood idle long enough, is the consumer's; the default claims
//! nothing.

use crate::hooks::HookPoint;

/// One pane a consumer claims.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct PaneClaim {
  pub agent: String,
  pub label: String,
  /// Whether the consumer has it stand idle long enough to be taken down now.
  pub idle: bool,
}

/// The point the pane claims are asked at.
pub struct PaneClaims;

impl HookPoint for PaneClaims {
  type Ask = ();
  type Answer = Vec<PaneClaim>;
  const NAME: &'static str = "pane-claims";

  fn default_answer(_ask: ()) -> Vec<PaneClaim> {
    vec![]
  }
}

crate::wired!(PaneClaims);
