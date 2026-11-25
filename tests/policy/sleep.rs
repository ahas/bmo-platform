use policy::sleep::*;

#[test]
fn the_sleep_falls_in_a_day_and_its_instruction_names_the_date_the_budget_and_the_four_acts() {
  assert!(SLEEP.hour < 24);
  assert!((1..=24).contains(&SLEEP.window_hours));
  assert!((-12..=14).contains(&SLEEP.utc_offset_hours));
  assert!(
    SLEEP.tick_seconds > 0 && SLEEP.tick_seconds <= 3_600,
    "a pass comes round within every hour"
  );

  for word in [
    "{date}",
    "{budget}",
    "brain_unconsolidated",
    "brain_consolidated",
    "brain_duplicates",
    "brain_merge",
    "brain_move",
    "state",
    "followup",
    "promise",
    "\"summary\"",
    "\"learned\"",
    "\"merged\"",
    "\"closed\"",
  ] {
    assert!(SLEEP.instruction.contains(word), "the instruction names {word}");
  }
}
