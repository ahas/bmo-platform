use crate::support::wording::assert_no_instruction;
use policy::supervisor_runner::*;

#[test]
fn reads_and_marks_what_each_line_carries() {
  let runner = &*SUPERVISOR_RUNNER;

  assert!(runner.limits.calls > 0);
  assert!(runner.cut.notice.contains("{reason}"));
  assert!(runner.blocks.reference_opening.contains("{count}"));
  assert_eq!(runner.cut.instruction.len(), 3);

  for line in &runner.cut.instruction {
    assert_no_instruction(line);
  }
}
