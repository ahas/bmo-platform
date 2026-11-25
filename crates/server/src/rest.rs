//! The deployment's rest as agency's store holds it (docs/api.md, 「Accounts」, the rest): it rests while the
//! service's own outage rest stands, or while every enabled Claude account the platform's own work runs on
//! has reached its own ceiling, until the first of them comes back. Agency reads it and writes it; the
//! silence is published once as it begins, whatever closed the last login, and its end is answered once to
//! whoever clears the passed rests. Whether a supervisor whose road is on another provider answers a room
//! meanwhile — read off its road (`GET /supervisors/{supervisorName}/road`) — and what anyone is told of it,
//! are its consumers'. Beside the logins' rests stand the models' (docs/cache-fallback.md, 「The account
//! judgement」): one model resting on one login, which closes that login to that model's runs alone and
//! never counts toward the deployment's rest.

use axum::http::StatusCode;
use cache_siren::claude::MODE as CLAUDE_CODE_MODE;
use cache_siren::{FULL, Slot};
use policy::rest::REST;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use store::one;
use store::settings::epoch_seconds;

use crate::Refusal;
use crate::control::claude_accounts::{self, Carrying, Judging, SEED_ACCOUNT, full_window_until};
use crate::door::record::SessionUsage;
use crate::events::Event;
use crate::places::Ground;
use crate::queue::settings;

// A rest that names no account: the service's outage, or a limit rest of this machine's own login.
const REST_UNTIL: &str = "rest_until";
// Why the rest began: `limit` or `outage`.
const REST_KIND: &str = "rest_kind";
// One model's rest on one login, by the login and the model.
const MODEL_REST: &str = "model_rest:";
// The kind of the silence told as it began, kept until its end is read.
const TOLD: &str = "rest_told";

async fn setting(db: &DatabaseConnection, key: &str) -> Result<Option<String>, Refusal> {
  let row = one(db, r#"SELECT "value" FROM "settings" WHERE "key" = $1"#, vec![key.into()]).await?;

  Ok(row.map(|row| row.try_get("", "value")).transpose()?)
}

/// The moment a login's own session usage rests it until, in epoch seconds: its session window's reset where
/// that window reads full, its week's where the week does — the policy's fallback while from now where the
/// week names none — and none while every window has any left (docs/usage-windows.md, 「What reads the
/// windows」).
pub fn rest_until_after(usage: &SessionUsage, now: i64) -> Option<f64> {
  if usage.utilization >= FULL {
    return Some(seconds_of(usage.resets_at));
  }

  if usage.weekly_utilization.is_some_and(|weekly| weekly >= FULL) {
    return Some(usage.weekly_resets_at.map_or((now + REST.fallback_seconds) as f64, seconds_of));
  }

  None
}

/// A moment in epoch seconds, whichever unit it was reported in (docs/api.md, 「accounts, rest and usage」): one
/// counted in milliseconds is brought to the second it names, its fraction kept.
pub fn seconds_of(moment: f64) -> f64 {
  if moment.abs() >= store::settings::MILLISECOND_MOMENTS as f64 {
    moment / 1000.0
  } else {
    moment
  }
}

/// A rest's moment as its value is written: the leading whole number, a sign before it taken with it.
pub fn leading_number(value: &str) -> Option<i64> {
  let value = value.trim_start();
  let digits = value.strip_prefix(['-', '+']).unwrap_or(value);
  let length = value.len() - digits.len() + digits.chars().take_while(char::is_ascii_digit).count();

  value[..length].parse().ok()
}

// A moment a rest is kept until, in epoch seconds, one kept in milliseconds read as the second it names.
async fn moment(db: &DatabaseConnection, key: &str) -> Result<Option<i64>, Refusal> {
  Ok(setting(db, key).await?.and_then(|value| leading_number(&value)).map(epoch_seconds))
}

/// Whether the deployment rests now, `now` in epoch seconds.
pub async fn resting(judging: Judging<'_>, now: i64) -> Result<bool, Refusal> {
  Ok(rest_until(judging, now).await?.is_some())
}

// The service's own rest, which belongs to no account, where it stands past `now`.
async fn outage_rest(db: &DatabaseConnection, now: i64) -> Result<Option<i64>, Refusal> {
  if setting(db, REST_KIND).await?.as_deref() != Some("outage") {
    return Ok(None);
  }

  Ok(moment(db, REST_UNTIL).await?.filter(|until| *until > now))
}

/// Until when the deployment rests, in epoch seconds: the service's own rest where one stands, or else the
/// soonest end of the enabled accounts' rests where every one of them rests; none where it does not rest.
pub async fn rest_until(judging: Judging<'_>, now: i64) -> Result<Option<i64>, Refusal> {
  let db = judging.db;

  if let Some(until) = outage_rest(db, now).await? {
    return Ok(Some(until));
  }

  let mut soonest: Option<i64> = None;

  for id in platform_accounts(db).await? {
    match account_rest_until(judging, &id).await?.filter(|until| *until > now) {
      Some(until) => soonest = Some(soonest.map_or(until, |held| held.min(until))),
      None => return Ok(None),
    }
  }

  Ok(soonest)
}

// The enabled accounts the platform's own work runs on, whose rests alone make the deployment's: a person's or
// an organisation's own login neither holds the rest nor lifts it.
async fn platform_accounts(db: &DatabaseConnection) -> Result<Vec<String>, Refusal> {
  let carried = claude_accounts::carried_for_user(db, Carrying::Platform)
    .await
    .map_err(|refused| Refusal::new(refused.status, "accounts-unread", refused.error))?;

  Ok(carried.into_iter().map(|account| account.id).collect())
}

/// Why the deployment rests: the service's outage where its own rest stands, a usage limit otherwise.
pub async fn rest_kind(db: &DatabaseConnection, now: i64) -> Result<&'static str, Refusal> {
  Ok(if outage_rest(db, now).await?.is_some() { "outage" } else { "limit" })
}

// How long one account's own limit rest stands, as the one account judgement reads it
// (`control::claude_accounts`): its own rest, or a window of its own agency keeps read full.
async fn account_rest_until(judging: Judging<'_>, account: &str) -> Result<Option<i64>, Refusal> {
  crate::control::claude_accounts::rest_until(judging, account)
    .await
    .map_err(|refused| Refusal::new(refused.status, "account-rest-unread", refused.error))
}

/// Why the deployment rested, where its rest has just passed: every rest whose moment is behind `now` cleared,
/// every model's rest that closes nothing any longer among them, and the kind of the silence that ended —
/// the service's outage or a limit — where the deployment was silent and rests no longer, answered once: the
/// silence told as it began where one was, else the one the passed rests held; none otherwise.
pub async fn clear_passed_rests(judging: Judging<'_>, now: i64) -> Result<Option<RestKind>, Refusal> {
  let db = judging.db;
  let unnamed = moment(db, REST_UNTIL).await?;
  let outage_held = setting(db, REST_KIND).await?.as_deref() == Some("outage") && unnamed.is_some();
  let told = setting(db, TOLD).await?;
  let mut held = vec![];
  let mut passed = vec![];

  if unnamed.is_some_and(|until| until <= now) {
    passed.push(REST_UNTIL.to_owned());
    passed.push(REST_KIND.to_owned());
  }

  for id in platform_accounts(db).await? {
    let own_key = format!("{REST_UNTIL}:{id}");

    held.push(account_rest_until(judging, &id).await?);

    if moment(db, &own_key).await?.is_some_and(|until| until <= now) {
      passed.push(own_key);
    }
  }

  let was_silent = outage_held || (!held.is_empty() && held.iter().all(Option::is_some));

  for key in &passed {
    settings::forget(db, key).await?;
  }

  // A model's rest that closes nothing any longer goes too; it never silenced the deployment, so its passing
  // ends no silence.
  for whose in settings::named(db, MODEL_REST).await? {
    let key = format!("{MODEL_REST}{whose}");

    if let Some(rest) = settings::get(db, &key).await?
      && model_rest_closes(judging, &rest, now).await?.is_none()
    {
      settings::forget(db, &key).await?;
    }
  }

  if resting(judging, now).await? {
    return Ok(None);
  }

  if let Some(told) = told {
    settings::forget(db, TOLD).await?;

    return Ok(Some(if told == RestKind::Outage.word() { RestKind::Outage } else { RestKind::Limit }));
  }

  if passed.is_empty() || !was_silent {
    return Ok(None);
  }

  Ok(Some(if outage_held { RestKind::Outage } else { RestKind::Limit }))
}

/// The silence told as it begins: published once as `rest.changed`, and kept as told so neither the watch
/// tells it again nor its end goes unanswered. `account` is the login whose limit closed it, where one did.
pub async fn tell_silence(
  ground: &Ground,
  kind: RestKind,
  account: Option<&str>,
  until: i64,
) -> Result<(), Refusal> {
  settings::set(&ground.db, TOLD, kind.word()).await?;
  ground.events.publish(DeploymentResting {
    account: account.filter(|_| kind.of_a_login()).map(str::to_owned),
    ..DeploymentResting::of(kind, "", until)
  });

  Ok(())
}

/// The silence nobody wrote a rest for, told on the rest policy's clock: where the deployment rests, no
/// silence has been told, and the service's outage or a window of the platform's own read full closed it.
/// A reading closes a login with nothing written, so no writer saw that silence begin. A silence the
/// written rests alone hold is told by whatever wrote them, and one held by refusals alone is never told as
/// a limit.
pub async fn watch_silence(ground: &Ground, now: i64) -> Result<(), Refusal> {
  let judging = Judging::of(ground);
  let db = &ground.db;
  let Some(until) = rest_until(judging, now).await? else {
    return Ok(());
  };

  if setting(db, TOLD).await?.is_some() {
    return Ok(());
  }

  let kind = if outage_rest(db, now).await?.is_some() {
    RestKind::Outage
  } else {
    let mut windowed = false;

    for id in platform_accounts(db).await? {
      windowed |= full_window_until(judging.windows, &id, now).await.is_some();
    }

    if !windowed {
      return Ok(());
    }

    RestKind::Limit
  };

  println!("the deployment rests until {until} by its windows or its outage: nothing answers now");

  tell_silence(ground, kind, None, until).await
}

/// The silence watched for as long as the process stands, on the rest policy's clock.
pub async fn keep_watching(ground: std::sync::Arc<Ground>) {
  let period = std::time::Duration::from_secs(REST.lift_check_seconds);

  crate::clock::every("rest watch", period, period, move || {
    let ground = ground.clone();

    async move { watch_silence(&ground, crate::now_ms() / 1000).await.map_err(|why| why.detail) }
  })
  .await;
}

/// The rest standing at `now` where a rest of `kind` would be written, in epoch seconds: the service's outage, or
/// the login's own rest, its windows' among them; none where nothing stands there.
pub async fn standing_rest(
  judging: Judging<'_>,
  kind: RestKind,
  account: &str,
  now: i64,
) -> Result<Option<i64>, Refusal> {
  match kind {
    RestKind::Outage => outage_rest(judging.db, now).await,
    RestKind::Limit | RestKind::Refused => {
      Ok(account_rest_until(judging, account).await?.filter(|until| *until > now))
    }
  }
}

/// The administrator's end of the rest (docs/api.md, 「accounts, rest and usage」): the service's outage, every
/// rest written under the platform's own logins and every model's rest on them cleared, and the full windows
/// kept for those logins set aside until their reader reads them again. Answers whether the deployment rested
/// before; the silence told as it began stays told, so its end is answered once to whoever clears the passed
/// rests.
pub async fn end_rest(judging: Judging<'_>, now: i64) -> Result<bool, Refusal> {
  let db = judging.db;
  let was_resting = resting(judging, now).await?;

  settings::forget(db, REST_UNTIL).await?;
  settings::forget(db, REST_KIND).await?;

  for id in platform_accounts(db).await? {
    settings::forget(db, &format!("{REST_UNTIL}:{id}")).await?;

    for whose in settings::named(db, &format!("{MODEL_REST}{id}:")).await? {
      settings::forget(db, &format!("{MODEL_REST}{id}:{whose}")).await?;
    }

    let slot = Slot {
      mode: CLAUDE_CODE_MODE,
      account: &id,
    };
    let reading = judging.windows.read(slot).await?;

    if reading.windows.iter().any(|kept| kept.window.is_full()) {
      let seen_at = reading.windows.iter().map(|kept| kept.seen_at).max().unwrap_or(now);
      let open: Vec<_> = reading
        .windows
        .into_iter()
        .filter(|kept| !kept.window.is_full())
        .map(|kept| kept.window)
        .collect();

      judging.windows.replace(slot, &open, seen_at).await?;
    }
  }

  println!("the rest was ended by hand");

  Ok(was_resting)
}

/// What a rest is: one login's limit, one login's refusal of a run, or the service's own outage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestKind {
  Limit,
  /// A login that refused a run, a request for `/login` or a 403, rests under its own name like a limit; the
  /// owner hears of it as the login notice tells it, never as a limit.
  Refused,
  Outage,
}

impl RestKind {
  /// The word the rest's kind is kept and published under.
  pub fn word(self) -> &'static str {
    match self {
      Self::Limit => "limit",
      Self::Refused => "refused",
      Self::Outage => "outage",
    }
  }

  // Whether the rest is one login's own, written under its name.
  fn of_a_login(self) -> bool {
    self != Self::Outage
  }
}

/// The rest that closed the last login that would answer, published once as it is written (docs/api.md,
/// 「Events」): the account it was written under, none for the service's outage, its kind, until when, in
/// epoch seconds, and the words the owner is told it in.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentResting {
  pub account: Option<String>,
  pub kind: &'static str,
  pub until: i64,
  pub notice: String,
}

impl Event for DeploymentResting {
  const NAME: &'static str = "rest.changed";
}

impl DeploymentResting {
  /// The rest as it is published, in the policy's words.
  pub fn of(kind: RestKind, account: &str, until: i64) -> Self {
    Self {
      account: kind.of_a_login().then(|| account.to_owned()),
      kind: kind.word(),
      until,
      notice: REST.resting.clone(),
    }
  }
}

/// A rest written down, answering whether it is the one that silenced the deployment: nothing was resting
/// before it and nothing answers now. A limit's rest and a refusal's are their login's, and a login already
/// resting is left as it stands; an outage is the service's, and a second one while the first goes on writes nothing. `until`
/// is in epoch seconds, written in plain decimal, with no fraction where it is whole.
pub async fn write_rest(
  judging: Judging<'_>,
  until: f64,
  kind: RestKind,
  account: &str,
  now: i64,
) -> Result<bool, Refusal> {
  let db = judging.db;
  let until = seconds_of(until);
  let was_resting = resting(judging, now).await?;

  match kind {
    RestKind::Outage => {
      if outage_rest(db, now).await?.is_some() {
        return Ok(false);
      }

      // A limit rest written in the unnamed pair is this machine's login's, put under its own name before
      // the service's rest takes the pair.
      let unnamed = if setting(db, REST_KIND).await?.as_deref() == Some("outage") {
        None
      } else {
        moment(db, REST_UNTIL).await?
      };
      let seed = format!("{REST_UNTIL}:{SEED_ACCOUNT}");

      if let Some(unnamed) = unnamed
        && moment(db, &seed).await?.is_none()
      {
        settings::set(db, &seed, &unnamed.to_string()).await?;
      }

      settings::set(db, REST_UNTIL, &until.to_string()).await?;
      settings::set(db, REST_KIND, "outage").await?;
    }
    RestKind::Limit | RestKind::Refused => {
      if account_rest_until(judging, account).await?.is_some_and(|standing| standing > now) {
        return Ok(false);
      }

      settings::set(db, &format!("{REST_UNTIL}:{account}"), &until.to_string()).await?;
    }
  }

  let silenced = !was_resting && resting(judging, now).await?;
  let whose = match kind {
    RestKind::Outage => "the service".to_owned(),
    RestKind::Limit | RestKind::Refused => format!("account {account}"),
  };

  println!(
    "{whose} rests until {}{}",
    chrono::DateTime::from_timestamp(until as i64, 0)
      .map(|moment| moment.to_rfc3339())
      .unwrap_or_default(),
    if silenced { ": nothing answers now" } else { "" }
  );

  Ok(silenced)
}

/// One model's rest on one login as it is kept: the login and the model, until when in epoch seconds,
/// none where the login's next reading decides, and the moment it was written.
#[derive(Debug, Deserialize, Serialize)]
struct ModelRest {
  account: String,
  model: String,
  until: Option<i64>,
  written: i64,
}

fn model_rest_key(account: &str, model: &str) -> String {
  format!("{MODEL_REST}{account}:{model}")
}

/// One model's rest on one login written down, until a moment in epoch seconds, or, with none, until a
/// reading of the login's windows is seen after `now`, and said once in the machine's log.
pub async fn write_model_rest(
  db: &DatabaseConnection,
  account: &str,
  model: &str,
  until: Option<i64>,
  now: i64,
) -> Result<(), Refusal> {
  let until = until.map(epoch_seconds);
  let rest = ModelRest {
    account: account.to_owned(),
    model: model.to_owned(),
    until,
    written: now,
  };
  let value = serde_json::to_string(&rest).map_err(|why| {
    Refusal::new(
      StatusCode::INTERNAL_SERVER_ERROR,
      "model-rest-unwritten",
      format!("a model's rest: {why}"),
    )
  })?;

  settings::set(db, &model_rest_key(account, model), &value).await?;
  println!(
    "account {account}: model {model} rests until {}",
    until
      .map_or_else(|| "its next usage reading".to_owned(), crate::control::claude_accounts::moment_words)
  );

  Ok(())
}

/// Until when, in epoch seconds, a model rests on a login at `now`; none where it does not.
pub async fn model_rest_until(
  judging: Judging<'_>,
  account: &str,
  model: &str,
  now: i64,
) -> Result<Option<i64>, Refusal> {
  match settings::get(judging.db, &model_rest_key(account, model)).await? {
    Some(rest) => model_rest_closes(judging, &rest, now).await,
    None => Ok(None),
  }
}

// Until when a kept model's rest closes its model at `now`, in epoch seconds: its own moment while it is
// ahead; with none, until a reading of the login's windows is seen after it was written, no longer than
// such a reading stands, so the next reading decides. A rest that cannot be read closes nothing.
async fn model_rest_closes(judging: Judging<'_>, kept: &str, now: i64) -> Result<Option<i64>, Refusal> {
  let Ok(rest) = serde_json::from_str::<ModelRest>(kept) else {
    eprintln!("a model's rest could not be read: {kept}");
    return Ok(None);
  };

  if let Some(until) = rest.until.map(epoch_seconds) {
    return Ok((until > now).then_some(until));
  }

  let reading = judging
    .windows
    .read(Slot {
      mode: CLAUDE_CODE_MODE,
      account: &rest.account,
    })
    .await?;

  if reading.windows.iter().any(|kept| kept.seen_at > rest.written) {
    return Ok(None);
  }

  let until = rest.written + i64::try_from(reading.fresh.as_secs()).unwrap_or(i64::MAX);

  Ok((until > now).then_some(until))
}
