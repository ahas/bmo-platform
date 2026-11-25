use std::path::Path;

use agency_server::control::claude_account_changes::*;
use agency_server::control::claude_accounts::{Credential, CredentialKind, EndpointModels};
use policy::control_words::CONTROL_WORDS;

#[test]
fn an_id_is_minted_from_the_name_or_else_stands_as_account() {
  assert_eq!(minted_id("Main Login!", &[]).unwrap(), "main-login");
  assert_eq!(minted_id("--.x", &[]).unwrap(), "account");
  assert_eq!(minted_id("Cuenta-ñ", &["cuenta"]).unwrap(), "cuenta-2");
  assert_eq!(minted_id("ñ ü", &["account"]).unwrap(), "account-2");
  assert_eq!(minted_id("a.b_c-", &[]).unwrap(), "a.b_c");
}

#[test]
fn a_name_is_refused_empty_too_long_or_taken() {
  assert!(name_fault("  ", &[]).is_some());
  assert!(name_fault(&"é".repeat(41), &[]).is_some());
  assert_eq!(
    name_fault(" Spare ", &["Spare"]).unwrap(),
    "There is already an account named \"Spare\"."
  );
  assert_eq!(name_fault("Main", &["Spare"]), None);
}

#[test]
fn a_credential_must_be_an_absolute_path_of_its_own_kind() {
  let dir = tempfile::tempdir().unwrap();
  let file = dir.path().join("token");
  let directory = |path: &Path| Credential::Directory { home: path.into() };
  let token = |path: &Path| Credential::Token { file: path.into() };

  std::fs::write(&file, "t").unwrap();

  assert_eq!(credential_fault(&directory(dir.path())), None);
  assert_eq!(credential_fault(&token(&file)), None);
  assert!(credential_fault(&token(dir.path())).is_some());
  assert!(credential_fault(&directory(&file)).is_some());
  assert!(credential_fault(&directory(Path::new("relative"))).is_some());
  assert!(credential_fault(&directory(&dir.path().join("nowhere"))).is_some());
}

#[test]
fn an_endpoint_needs_a_home_an_http_address_and_a_key_file_none_of_which_is_opened() {
  let dir = tempfile::tempdir().unwrap();
  let key = dir.path().join("endpoint.key");
  let endpoint = |url: &str, key_file: &Path| Credential::Endpoint {
    home: dir.path().into(),
    url: url.into(),
    key_file: key_file.into(),
    models: EndpointModels::default(),
  };
  let words = &CONTROL_WORDS.accounts;

  std::fs::write(&key, "k").unwrap();

  assert_eq!(credential_fault(&endpoint("https://endpoint.test/anthropic", &key)), None);
  assert_eq!(credential_fault(&endpoint("endpoint.test", &key)), Some(words.endpoint_url.clone()));
  assert_eq!(
    credential_fault(&endpoint("ftp://endpoint.test", &key)),
    Some(words.endpoint_url.clone())
  );
  assert_eq!(
    credential_fault(&endpoint("https://endpoint.test", dir.path())),
    Some(words.key_not_file.clone())
  );
  assert!(credential_fault(&endpoint("https://endpoint.test", Path::new("key"))).is_some());
}

#[test]
fn a_credential_given_in_part_keeps_the_rest_of_the_one_held() {
  let words = &CONTROL_WORDS.accounts;
  let held = Credential::Endpoint {
    home: "/homes/endpoint".into(),
    url: "https://endpoint.test".into(),
    key_file: "/keys/endpoint.key".into(),
    models: EndpointModels {
      opus: Some("big".into()),
      ..EndpointModels::default()
    },
  };
  let path_only = CredentialGiven {
    path: Some(" /homes/other ".into()),
    ..CredentialGiven::default()
  };

  assert_eq!(
    path_only.over(Some(&held)).unwrap(),
    Credential::Endpoint {
      home: "/homes/other".into(),
      url: "https://endpoint.test".into(),
      key_file: "/keys/endpoint.key".into(),
      models: EndpointModels {
        opus: Some("big".into()),
        ..EndpointModels::default()
      },
    }
  );
  assert_eq!(
    path_only.over(None).unwrap(),
    Credential::Directory {
      home: "/homes/other".into()
    }
  );

  let endpoint = CredentialGiven {
    kind: Some(CredentialKind::Endpoint),
    ..path_only.clone()
  };

  assert_eq!(endpoint.over(None), Err(words.endpoint_needed.clone()));
  assert_eq!(
    CredentialGiven {
      endpoint: Some("https://endpoint.test".into()),
      ..endpoint.clone()
    }
    .over(None),
    Err(words.key_file_needed.clone())
  );
  assert_eq!(CredentialGiven::default().over(None), Err(words.credential_path.clone()));
}
