use policy::gate_places::GATE_PLACES;

#[test]
fn a_gate_s_places_are_bounded_and_one_program_stands_inside_the_whole() {
  assert!(GATE_PLACES.places > 0);
  assert!((1..=GATE_PLACES.places).contains(&GATE_PLACES.per_program));
  assert!(GATE_PLACES.session_tokens > 0);
}
