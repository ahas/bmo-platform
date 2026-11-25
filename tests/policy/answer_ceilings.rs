use policy::answer_ceilings::*;

#[test]
fn the_ceilings_rise() {
  assert!(ANSWER_CEILINGS.choices.windows(2).all(|pair| pair[0] < pair[1]));
  assert!(ANSWER_CEILINGS.holds(2048));
}
