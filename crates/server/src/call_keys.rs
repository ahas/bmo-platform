//! The gateway's call keys (docs/own-llm.md, 「The keys」): every account but the deployment's own goes out on
//! the gateway under a key of its own, issued the first time a link or a launch needs it, its value written once
//! into a private file under the data root's `env/` and its SHA-256 digest alone kept in the store. The payer of a
//! request is the account its key is, so nothing a caller writes in a header moves its cost.

use std::path::{Path, PathBuf};

use sea_orm::ConnectionTrait;
use sha2::{Digest, Sha256};
use store::{Fault, one, run};

/// The folder under the data root's `env/` the call keys stand in.
pub const FOLDER: &str = "llm-gateway-calls";

/// Where an account's call key stands under the data root.
pub fn path(data_root: &Path, account: i64) -> PathBuf {
  data_root.join("env").join(FOLDER).join(format!("{account}.key"))
}

/// The digest the store keeps of a key, as the gateway's front computes it of the key a request carries.
pub fn digest(key: &str) -> String {
  hex::encode(Sha256::digest(key.trim().as_bytes()))
}

// A fresh key: two version-4 identifiers, 244 random bits, under a mark that tells it from the gateway's own.
fn fresh() -> String {
  format!("bmo-call-{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

fn written(at: &Path, key: &str) -> Result<(), Fault> {
  let folder = at.parent().expect("a call key's path stands in a folder");

  std::fs::create_dir_all(folder).map_err(|why| Fault::new(format!("{}: {why}", folder.display())))?;
  std::fs::set_permissions(folder, std::os::unix::fs::PermissionsExt::from_mode(0o700))
    .map_err(|why| Fault::new(format!("{}: {why}", folder.display())))?;

  let fresh = folder.join(format!(".{}.{}", account_file(at), std::process::id()));

  agent_host::disk::private_file(&fresh)
    .and_then(|mut file| std::io::Write::write_all(&mut file, format!("{key}\n").as_bytes()))
    .and_then(|()| std::fs::rename(&fresh, at))
    .map_err(|why| Fault::new(format!("{}: {why}", at.display())))
}

fn account_file(at: &Path) -> String {
  at.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

// The key the file holds where it holds one the store's live digest names.
fn standing(at: &Path, kept: Option<&str>) -> Option<String> {
  let key = std::fs::read_to_string(at).ok()?.trim().to_owned();

  (!key.is_empty() && kept == Some(digest(&key).as_str())).then_some(key)
}

/// The account's call key file, issued where the account holds no live key or its file does not hold it.
pub async fn ensure<C: ConnectionTrait>(db: &C, data_root: &Path, account: i64) -> Result<PathBuf, Fault> {
  let at = path(data_root, account);
  let kept = one(
    db,
    "SELECT key_hash FROM gateway_call_keys WHERE account = $1 AND revoked_at IS NULL",
    vec![account.into()],
  )
  .await?
  .and_then(|row| row.try_get::<String>("", "key_hash").ok());

  if standing(&at, kept.as_deref()).is_some() {
    return Ok(at);
  }

  let key = fresh();

  written(&at, &key)?;
  run(
    db,
    "INSERT INTO gateway_call_keys (account, key_hash, issued_at, revoked_at) VALUES ($1, $2, $3, NULL) \
     ON CONFLICT (account) DO UPDATE SET key_hash = EXCLUDED.key_hash, issued_at = EXCLUDED.issued_at, \
     revoked_at = NULL",
    vec![account.into(), digest(&key).into(), crate::now_ms().into()],
  )
  .await?;

  Ok(at)
}

/// The account's call key revoked and its file taken away, so the gateway refuses it from its next reading.
pub async fn revoke<C: ConnectionTrait>(db: &C, data_root: &Path, account: i64) -> Result<bool, Fault> {
  let revoked = run(
    db,
    "UPDATE gateway_call_keys SET revoked_at = $2 WHERE account = $1 AND revoked_at IS NULL",
    vec![account.into(), crate::now_ms().into()],
  )
  .await?;
  let at = path(data_root, account);

  match std::fs::remove_file(&at) {
    Ok(()) => {}
    Err(why) if why.kind() == std::io::ErrorKind::NotFound => {}
    Err(why) => return Err(Fault::new(format!("{}: {why}", at.display()))),
  }

  Ok(revoked > 0)
}
