use agency_server::control::harness::*;
use harness_providers::ARCHIVE_PROVIDERS;

use crate::support::scratch::Scratch;

#[test]
fn the_store_keeps_every_provider_this_build_carries() {
  let mut carried: Vec<&str> = harness_providers::providers().iter().map(|one| one.name()).collect();
  let mut stored = ARCHIVE_PROVIDERS.to_vec();

  carried.sort_unstable();
  stored.sort_unstable();
  assert_eq!(carried, stored);
}

#[tokio::test]
async fn a_provider_is_on_until_the_owner_switches_it_off_and_the_last_one_on_stays_on() {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  assert_eq!(enabled_providers(db).await.unwrap().len(), ARCHIVE_PROVIDERS.len());

  set_provider_enabled(db, "codex", false).await.unwrap();
  assert!(!provider_enabled(db, "codex").await.unwrap());

  for name in ARCHIVE_PROVIDERS.iter().filter(|name| **name != "deepseek") {
    set_provider_enabled(db, name, false).await.unwrap();
  }

  assert_eq!(enabled_providers(db).await.unwrap(), ["deepseek"]);
  assert!(
    set_provider_enabled(db, "deepseek", false).await.is_err(),
    "the last provider on stays on"
  );

  set_provider_enabled(db, "codex", true).await.unwrap();
  assert!(provider_enabled(db, "codex").await.unwrap());
}
