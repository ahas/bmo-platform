use policy::tool_clock::*;

#[test]
fn a_call_is_watched_after_its_minimum_and_stopped_after_its_stall() {
  assert!(TOOL_CLOCK.call.stall_seconds > 0);
  assert!(TOOL_CLOCK.call.min_seconds > 0);
  assert!(TOOL_CLOCK.call.look_seconds > 0);
  assert!(TOOL_CLOCK.call.still_every_seconds >= TOOL_CLOCK.call.log_after_seconds / 2);
  assert!(TOOL_CLOCK.call.stop_bound_seconds >= TOOL_CLOCK.call.stop_stall_seconds);
  assert!(TOOL_CLOCK.turn.bound_minutes >= 60);
  assert!(TOOL_CLOCK.turn.cut.contains("{minutes}"));
}

#[test]
fn a_silent_call_is_asked_about_past_its_window_with_words_naming_it() {
  let silent = &TOOL_CLOCK.call.silent;

  assert!(silent.window_seconds >= TOOL_CLOCK.call.look_seconds);
  assert!((1..100).contains(&silent.idle_cpu_percent));
  assert!(silent.ask_chars >= 160);

  for mark in ["{job}", "{item}", "{tool}", "{cpu}", "{minutes}"] {
    assert!(silent.what.contains(mark), "the ask names {mark}");
  }

  for mark in ["{tool}", "{seconds}", "{why}"] {
    assert!(silent.cut.contains(mark), "the cut words name {mark}");
  }
}
