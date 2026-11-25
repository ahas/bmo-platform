//! What the settings table holds that more than one reader takes (docs/store.md, 「The settings table」): a
//! whole number a setting was written as, read as `parseInt` reads one, and the user who administers the
//! platform, as the gateway reads it on a connection of its own.

use sqlx::{AnyConnection, Row};

/// The setting naming the user who administers the platform.
pub const ADMINISTRATOR: &str = "users_administrator";

/// A whole number at the head of a text, as `parseInt` reads one.
pub fn leading_integer(text: &str) -> Option<i64> {
  let text = text.trim_start();
  let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
  let length = digits.bytes().take_while(u8::is_ascii_digit).count();

  if length == 0 {
    return None;
  }

  text[..text.len() - digits.len() + length].parse().ok()
}

/// The first moment, counted in seconds, past every date a deployment meets: a moment at or past it is read as
/// counted in milliseconds.
pub const MILLISECOND_MOMENTS: i64 = 100_000_000_000;

/// A moment in epoch seconds, whichever unit it was written in: one at or past `MILLISECOND_MOMENTS` is taken as
/// milliseconds and brought to the second it names.
pub fn epoch_seconds(moment: i64) -> i64 {
  if moment.abs() >= MILLISECOND_MOMENTS { moment.div_euclid(1000) } else { moment }
}

/// The user who administers the platform, where the settings name one.
pub async fn administrator(store: &mut AnyConnection) -> Result<Option<i64>, sqlx::Error> {
  let named = sqlx::query("SELECT value FROM settings WHERE key = $1")
    .bind(ADMINISTRATOR)
    .fetch_optional(&mut *store)
    .await?
    .map(|row| row.try_get::<String, _>("value"))
    .transpose()?;

  Ok(named.as_deref().and_then(leading_integer))
}
