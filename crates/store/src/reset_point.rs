//! A supervisor's reset point (docs/supervisors.md, 「Starting it afresh」): the moment its last reset cut it off
//! at, in epoch milliseconds, kept in the settings under the supervisor's name. Agency's server moves it, and
//! every reader holds a turn to it: a turn begun at or before the point is overtaken.

/// The settings key a supervisor's reset point stands under.
pub fn key(agent: &str) -> String {
  format!("reset_point:{agent}")
}

/// Whether the point overtakes a turn begun at `began`.
pub fn overtakes(point: Option<i64>, began: i64) -> bool {
  point.is_some_and(|point| began <= point)
}
