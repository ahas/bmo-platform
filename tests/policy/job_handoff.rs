use policy::job_handoff::*;

#[test]
fn the_handoff_s_templates_carry_every_mark_their_readers_fill() {
  assert!(JOB_HANDOFF.opened.contains("{handoff}") && JOB_HANDOFF.opened.contains("{prompt}"));
  assert!(JOB_HANDOFF.from_log.contains("{why}") && JOB_HANDOFF.from_log.contains("{lines}"));
  assert!(JOB_HANDOFF.instruction.ends_with("the single word: none"));
  assert!(JOB_HANDOFF.log_lines > 0 && JOB_HANDOFF.line_chars > 0);
}
