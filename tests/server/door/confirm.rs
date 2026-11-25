use agency_server::door::confirm::*;

const PERMISSION: &str = "some output\n────────────────────\n Bash command\n\n   rm -rf build\n   Remove the build\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. No, and tell Claude what to do differently (esc)\n\n Esc to cancel · Tab to amend\n";

#[test]
fn a_permission_question_is_read_between_the_rule_and_its_choices() {
  assert_eq!(
    confirmation_of(PERMISSION),
    Some(Confirmation::Permission {
      text: "Bash command\n\n   rm -rf build\n   Remove the build\n\n Do you want to proceed?".to_owned(),
      amendable: true,
    })
  );
}

#[test]
fn the_keys_move_the_cursor_to_the_choice_named() {
  assert_eq!(choice_keys(PERMISSION, "No"), Some(vec!["down"]));
  assert_eq!(choice_keys(PERMISSION, "Yes"), Some(vec![]));
  assert_eq!(choice_keys("no choices here", "Yes"), None);
}

#[test]
fn a_question_it_does_not_know_is_kept_as_the_foot_of_the_screen_without_its_countdown() {
  let screen = "a\n\nb\nThis will automatically deny this request in 0:42\n";

  assert_eq!(
    confirmation_of(screen),
    Some(Confirmation::Unknown {
      screen: "a\nb".to_owned()
    })
  );
}

#[test]
fn a_refusal_names_who_refused_and_what_to_do_instead() {
  let asked = Confirmation::Unknown {
    screen: "x".to_owned(),
  };
  let words = refusal_words(&asked, Some("No."));

  assert!(words.starts_with(&format!(
    "Refused by {}, who said: No. Do not retry it as written.",
    policy::deployment::DEPLOYMENT.owner_name
  )));
  assert!(words.ends_with("What stood on the screen is in the run's log."));
}
