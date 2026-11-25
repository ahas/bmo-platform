use policy::record_secrets::RECORD_SECRETS;

#[test]
fn the_record_secrets_read_with_the_words_a_secret_is_named_by() {
  assert!(RECORD_SECRETS.secret_words.iter().any(|word| word == "token"));
  assert!(RECORD_SECRETS.least_characters > 0);
}
