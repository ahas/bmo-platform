//! The gateway's call keys (docs/own-llm.md, 「The keys」): issued into a private file whose SHA-256 digest
//! alone the store keeps, the same key given again while it stands, and a fresh one issued after it is revoked.
//! What a revocation leaves on disk and in the store is the off-boarding route's to show
//! (`control::personal_supply`).

use std::os::unix::fs::PermissionsExt;

use agency_server::call_keys::{ensure, path, revoke};
use sha2::{Digest, Sha256};

use super::PERSON;
use crate::support::scratch::Scratch;

// The key a file holds and the digest the store keeps of the account's live key.
async fn issued(scratch: &Scratch, file: &std::path::Path) -> (String, String) {
  let key = std::fs::read_to_string(file).unwrap().trim().to_owned();
  let kept = store::one(
    scratch.db(),
    r#"SELECT "key_hash" FROM "gateway_call_keys" WHERE "account" = $1 AND "revoked_at" IS NULL"#,
    vec![PERSON.into()],
  )
  .await
  .unwrap()
  .expect("the account holds a live key")
  .try_get("", "key_hash")
  .unwrap();

  (key, kept)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_key_is_issued_privately_kept_as_its_digest_and_issued_afresh_after_a_revocation() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");

  let file = ensure(scratch.db(), &data, PERSON).await.unwrap();
  let (key, kept) = issued(&scratch, &file).await;

  assert_eq!(file, path(&data, PERSON));
  assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
  assert_eq!(kept, hex::encode(Sha256::digest(key.as_bytes())));

  ensure(scratch.db(), &data, PERSON).await.unwrap();

  assert_eq!(
    issued(&scratch, &file).await,
    (key.clone(), kept.clone()),
    "a standing key is given again"
  );
  assert!(revoke(scratch.db(), &data, PERSON).await.unwrap());

  ensure(scratch.db(), &data, PERSON).await.unwrap();

  let (fresh, fresh_kept) = issued(&scratch, &file).await;

  assert_ne!(fresh, key);
  assert_eq!(fresh_kept, hex::encode(Sha256::digest(fresh.as_bytes())));
}
