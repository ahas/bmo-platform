//! The model registry read out of the store in the registry's order (docs/own-llm.md, 「Registered
//! models」).

use store::open_read;
use store::registry::{RegisteredAddress, Registration, registrations};

use crate::support::scratch::Scratch;

// Written out of order, so the reading's own order is what the test sees.
const ROWS: [&str; 5] = [
  "INSERT INTO registered_models VALUES ('local-0-1', 'Local 0.1', 'ko2', 32768, 4096, 1, 0, 200, 1)",
  "INSERT INTO registered_models VALUES ('local-model', 'Local model', 'qwen', 65536, 8192, 0, 1, 100, 0)",
  "INSERT INTO registered_model_addresses VALUES ('local-0-1', 'bravo', 2, 'http://127.0.0.1:18023/v1/chat/completions', NULL, 0, NULL)",
  "INSERT INTO registered_model_addresses (registration, node, position, url, key_file, key_owned, placed_at, role_files) VALUES ('local-0-1', 'alpha', 1, 'http://127.0.0.1:18021/v1/chat/completions', 'alpha-llm.token', 0, 1700000000, 1)",
  "INSERT INTO registered_model_addresses VALUES ('local-model', 'd-vex', 1, 'https://llm.example.com/v1/chat/completions', 'registered/local-model-d-vex.key', 1, NULL)",
];

fn expected() -> Vec<Registration> {
  vec![
    Registration {
      name: "local-model".into(),
      label: "Local model".into(),
      model: "qwen".into(),
      context_tokens: 65_536,
      answer_tokens: 8_192,
      thinking: false,
      walked: true,
      keeps_research: false,
      card_service: None,
      role_files: false,
      account: None,
      addresses: vec![RegisteredAddress {
        node: "d-vex".into(),
        url: "https://llm.example.com/v1/chat/completions".into(),
        key_file: Some("registered/local-model-d-vex.key".into()),
        key_owned: true,
        placed_at: None,
        role_files: false,
        reach_port: None,
      }],
    },
    Registration {
      name: "local-0-1".into(),
      label: "Local 0.1".into(),
      model: "ko2".into(),
      context_tokens: 32_768,
      answer_tokens: 4_096,
      thinking: true,
      walked: false,
      keeps_research: true,
      card_service: None,
      role_files: false,
      account: None,
      addresses: vec![
        RegisteredAddress {
          node: "alpha".into(),
          url: "http://127.0.0.1:18021/v1/chat/completions".into(),
          key_file: Some("alpha-llm.token".into()),
          key_owned: false,
          placed_at: Some(1_700_000_000),
          role_files: true,
          reach_port: None,
        },
        RegisteredAddress {
          node: "bravo".into(),
          url: "http://127.0.0.1:18023/v1/chat/completions".into(),
          key_file: None,
          key_owned: false,
          placed_at: None,
          role_files: false,
          reach_port: None,
        },
      ],
    },
  ]
}

#[tokio::test]
async fn the_registry_is_read_out_of_the_store_in_its_order() {
  let scratch = Scratch::new().await;

  for row in ROWS {
    scratch.run(row, vec![]).await;
  }

  let mut store = open_read(&scratch.address()).await.unwrap();
  assert_eq!(registrations(&mut store).await.unwrap(), expected());
}

#[tokio::test]
async fn a_store_whose_registry_is_not_made_yet_is_an_error_and_not_an_empty_registry() {
  let scratch = Scratch::blank().await;

  let mut store = open_read(&scratch.address()).await.unwrap();
  assert!(registrations(&mut store).await.is_err());
}
