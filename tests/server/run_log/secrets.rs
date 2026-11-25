//! No credential reaches the run log: a header, a key, an address, a flag and an assignment lose their
//! values, a word that merely looks like one keeps what follows, and a call that opens a credential file is
//! known by its path.

use agency_server::run_log::secrets::{MASKED_VALUE, mask_secret_text, mask_secrets, reads_secret_file};

#[test]
fn a_header_a_key_an_address_a_flag_and_an_assignment_lose_their_values() {
  assert_eq!(
    mask_secret_text("Authorization: Bearer abcdefgh1234"),
    format!("Authorization: Bearer {MASKED_VALUE}")
  );
  assert_eq!(mask_secret_text("key sk-proj-abcdef123"), format!("key sk-proj-{MASKED_VALUE}"));
  assert_eq!(
    mask_secret_text("postgres://agency:hunter2@db"),
    format!("postgres://agency:{MASKED_VALUE}@db")
  );
  assert_eq!(mask_secret_text("run --token abc123 now"), format!("run --token {MASKED_VALUE} now"));
  assert_eq!(mask_secret_text("API_KEY=xyz other"), format!("API_KEY={MASKED_VALUE} other"));
  assert_eq!(
    mask_secret_text("bun client/scripts/user.ts set qa pw1"),
    format!("bun client/scripts/user.ts set qa {MASKED_VALUE}")
  );
}

#[test]
fn a_word_that_merely_ends_in_a_scheme_or_a_flag_keeps_what_follows() {
  assert_eq!(mask_secret_text("--token-file x"), "--token-file x");
  assert_eq!(mask_secret_text("myToken abcdefghij"), "myToken abcdefghij");
  assert_eq!(mask_secret_text("--token --verbose"), "--token --verbose");
}

#[test]
fn arguments_are_masked_as_the_json_they_are_in_their_own_order() {
  assert_eq!(
    mask_secrets(r#"{"command":"curl -H 'Authorization: Bearer abcdefgh123'","password":"p","tokens":5}"#),
    format!(
      r#"{{"command":"curl -H 'Authorization: Bearer {MASKED_VALUE}'","password":"{MASKED_VALUE}","tokens":5}}"#
    )
  );
}

#[test]
fn a_call_that_opens_a_credential_file_is_known_by_its_path() {
  assert!(reads_secret_file(r#"{"file_path":"/home/tester/.env"}"#));
  assert!(reads_secret_file("cat ~/data/env/bot.env"));
  assert!(!reads_secret_file(r#"{"file_path":"/home/tester/.env.example"}"#));
  assert!(!reads_secret_file(r#"{"file_path":"/home/tester/README.md"}"#));
}
