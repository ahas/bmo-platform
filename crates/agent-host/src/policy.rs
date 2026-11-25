//! The store's word on whether a child may start, read again at the moment it would: the owner's switch on
//! its provider and the cancellation markers, read as the launcher reads them.

use sqlx::Row;
use store::{open_read, retrying};

use crate::envelope::Policy;

/// Why a child must not start.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
  /// A cancellation stands over one of its names: the stop reaching this run.
  Cancelled,
  /// Its provider is switched off.
  Disabled,
}

const SWITCH_PREFIX: &str = "provider_switch:";
const CANCEL_PREFIX: &str = "execution_cancel:";

/// Whether a provider is on under its stored switch: on unless the switch holds `off` (docs/provider-order.md,
/// 「The owner's switch on a provider」).
pub fn provider_on(stored: Option<&str>) -> bool {
  stored != Some("off")
}

// The settings the policy reads: the switch on the run's provider, and every cancellation marker standing over
// one of the run's names. The store is read at the address the policy names, on a runtime of this call's
// own, since the host waits on its children from plain threads.
fn read_settings(
  address: &str,
  provider: &str,
  names: &[String],
) -> Result<(Option<String>, bool), String> {
  let runtime = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .map_err(|why| format!("Error: {why}"))?;

  // Read again whole where the database went away under the reading (docs/store.md, 「Connecting」).
  runtime
    .block_on(retrying(|| async move {
      let mut db = open_read(address).await?;
      let switch: Option<Option<String>> = sqlx::query("SELECT value FROM settings WHERE key = $1")
        .bind(format!("{SWITCH_PREFIX}{provider}"))
        .fetch_optional(&mut db)
        .await?
        .map(|row| row.try_get(0))
        .transpose()?;
      let mut cancelled = false;

      for name in names {
        let marked = sqlx::query("SELECT 1 FROM settings WHERE key = $1")
          .bind(format!("{CANCEL_PREFIX}{name}"))
          .fetch_optional(&mut db)
          .await?;

        cancelled |= marked.is_some();
      }

      Ok::<_, sqlx::Error>((switch.flatten(), cancelled))
    }))
    .map_err(|why| format!("Error: {why}"))
}

/// The store's answer for this policy, or why it could not be read.
pub fn refusal(policy: &Policy) -> Result<Option<Refusal>, String> {
  let (switch, cancelled) = read_settings(&policy.address, &policy.provider, &policy.names)?;

  if cancelled {
    return Ok(Some(Refusal::Cancelled));
  }

  if !provider_on(switch.as_deref()) {
    return Ok(Some(Refusal::Disabled));
  }

  Ok(None)
}
