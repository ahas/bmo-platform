//! The words a running job's line is said in, and the name of each step of a work order (docs/jobs.md, 「The
//! item is the only number」; docs/work-orders.md, 「What a step is called」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkLines {
  /// A job's item: `{id}`.
  pub item: String,
  /// A run of an item past its first: `{item}`, `{attempt}`.
  pub attempt: String,
  /// A job with no item behind it and no line of why it was launched.
  pub itemless: String,
  /// A work order's round past the first: `{round}`.
  pub round: String,
  /// Each step of a work order by its name.
  pub steps: Steps,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Steps {
  pub plan: String,
  pub investigate: String,
  pub review: String,
  pub synthesis: String,
  pub build: String,
  pub r#final: String,
}

impl Steps {
  /// A stage's name by the kind its plan gave it: a stage of no kind the order knows finds things out.
  pub fn stage(&self, kind: Option<&str>) -> &str {
    if kind == Some("review") { &self.review } else { &self.investigate }
  }
}

crate::part!(pub WORK_LINES: WorkLines = "work-lines");

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_stage_is_named_by_its_kind_and_one_of_no_known_kind_finds_things_out() {
    assert_eq!(WORK_LINES.steps.stage(Some("review")), WORK_LINES.steps.review);
    assert_eq!(WORK_LINES.steps.stage(Some("investigate")), WORK_LINES.steps.investigate);
    assert_eq!(WORK_LINES.steps.stage(None), WORK_LINES.steps.investigate);
    assert_eq!(WORK_LINES.steps.stage(Some("anything")), WORK_LINES.steps.investigate);
  }
}
