//! `deepseek_top_up`: the supervisor tops the DeepSeek balance up by an amount the owner names (docs/tools.md,
//! 「The DeepSeek top-up」). DeepSeek has no payment API, so a call that passes every check pays on DeepSeek's web
//! platform in the owner's own Firefox, on his own profile and its sign-in, with the platform's card, signing in
//! with the platform's DeepSeek login only where the platform asks for it, both read from the console secrets
//! agency's settings name. A secret value is typed into the platform's page and is never answered, logged or
//! written anywhere.

mod bidi;
mod browser;
mod mail;
mod platform;
mod profile;
mod secret;

use std::path::Path;
use std::time::Duration;

use bmo_agency_client::{AgencyClient, ProviderQuota, Settings};
use chrono::{DateTime, SecondsFormat};
use harness_providers::Provider;
use harness_providers::deepseek::DeepSeek;
use policy::fill_marks;
use policy::providers::deepseek::{DEEPSEEK, SavedSecret, TopUp};
use serde::Deserialize;
use serde_json::{Value, json};
use tool_call::{Answer, Asked, Control, Handed, Session};

pub use browser::{Place, firefox_environment, launch_line};
pub use mail::MailService;
pub use platform::{MailRoad, Paid, Platform, Stop, Waits};
pub use profile::default_profile;
pub use secret::{Saved, SecretFault};

/// The tool's namespace and entry.
pub const NAMESPACE: &str = "bmo";
pub const NAME: &str = "deepseek_top_up";
pub const TITLE: &str = "Top the DeepSeek balance up";
pub const DESCRIPTION: &str = "Top the DeepSeek balance up by the amount the owner names, in the currency DeepSeek keeps the balance in, paying with the card saved in the console's secret store on DeepSeek's platform in the owner's own Firefox on this machine, on his own profile and sign-in and out of his sight, signing in with the DeepSeek login saved there only where DeepSeek asks; call it only when the owner asks for a top-up, and again only on his new word, whatever it answered. It pays nothing while his Firefox is open, and says so. One call is held to a cap and pays at most once. Where DeepSeek or the card's issuer asks a person to confirm, it stops there and says what the person does. It answers the amount paid and the balance after, as the balance reader read it.";
pub const CAPABILITIES: &[&str] = &["room.tools"];
/// The administrator's grant a call needs: the card and the DeepSeek account are the platform's own.
pub const GRANT: &str = "platform.payment";
/// Agency's settings naming the console secrets the card and the login are saved in (docs/api.md, 「settings」).
pub const CARD_SETTING: &str = "deepseekCardSecret";
pub const LOGIN_SETTING: &str = "deepseekLoginSecret";

pub fn entry() -> String {
  format!("{NAMESPACE}/{NAME}")
}

/// What a call names: the amount alone. Neither the card nor the login is ever taken from a call.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopUpInput {
  #[serde(default)]
  pub amount: Option<f64>,
}

impl TopUpInput {
  /// The call's arguments; a call that hands none names no amount.
  pub fn read(arguments: Value) -> Result<Self, serde_json::Error> {
    if arguments.is_null() {
      Ok(Self::default())
    } else {
      serde_json::from_value(arguments)
    }
  }
}

/// Where a call pays and how long it waits for the balance reader after a payment.
#[derive(Debug, Clone)]
pub struct Payer {
  pub platform: Platform,
  /// For the balance reader to read the balance after a payment, and how often agency is asked meanwhile.
  pub balance_wait: Duration,
  pub balance_look: Duration,
}

impl Payer {
  /// The platform, the owner's Firefox under `$HOME` and the waits, as the policy holds them; the mail service as
  /// `data_root` names it.
  pub fn of(data_root: &Path) -> Self {
    let policy = &DEEPSEEK.top_up;
    let waits = &policy.waits;
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();

    Self {
      platform: Platform {
        address: policy.platform.address.clone(),
        top_up_path: policy.platform.top_up_path.clone(),
        sign_in_path: policy.platform.sign_in_path.clone(),
        place: Place {
          firefox: policy.platform.firefox.clone(),
          profiles: home.join(&policy.platform.profiles),
          scope: policy.platform.scope.clone(),
          screen: policy.platform.screen.clone(),
          start: Duration::from_secs(waits.start),
        },
        waits: Waits {
          step: Duration::from_secs(waits.step),
          confirm: Duration::from_secs(waits.confirm),
          look: Duration::from_millis(waits.look_ms),
        },
        mail: MailRoad {
          service: MailService::read(data_root, |name| std::env::var(name).ok()),
          senders: policy.mail.senders.clone(),
          link_words: policy.mail.link_words.clone(),
          code_digits: policy.mail.code_digits,
          wait: Duration::from_secs(waits.mail),
          look: Duration::from_secs(waits.mail_look),
        },
      },
      balance_wait: Duration::from_secs(waits.balance),
      balance_look: Duration::from_secs(waits.balance_look),
    }
  }
}

/// The moment a balance was seen, in epoch seconds, as an answer shows it: the UTC time, or the seconds where
/// they stand outside the calendar.
fn seen_at(seconds: i64) -> String {
  DateTime::from_timestamp(seconds, 0)
    .map_or_else(|| seconds.to_string(), |at| at.to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// The amount a call names, above zero.
fn amount_of(input: &TopUpInput, policy: &TopUp) -> Result<f64, String> {
  input
    .amount
    .filter(|amount| amount.is_finite() && *amount > 0.0)
    .ok_or_else(|| policy.words.no_amount.clone())
}

/// The console secret a setting names, or the refusal naming the setting to fill where it names none.
fn named(
  name: Option<&str>,
  setting: &str,
  secret: &SavedSecret,
  policy: &TopUp,
) -> Result<String, String> {
  name.map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).ok_or_else(|| {
    fill_marks(
      &policy.words.secret_unnamed,
      &[("called", &secret.called), ("setting", setting), ("kind", &secret.kind)],
    )
  })
}

/// The secret read from the administrator's space, or the refusal saying what a person saves.
fn saved(data_root: &Path, name: &str, secret: &SavedSecret, policy: &TopUp) -> Result<Saved, String> {
  let (line, fields) = match Saved::read(&data_root.join("env").join(name), &secret.fields) {
    Ok(saved) => return Ok(saved),
    Err(SecretFault::Missing) => (&policy.words.secret_missing, String::new()),
    Err(SecretFault::Lacks(lacking)) => (&policy.words.secret_lacks, lacking.join(", ")),
  };

  Err(fill_marks(
    line,
    &[
      ("called", &secret.called),
      ("secret", name),
      ("kind", &secret.kind),
      ("fields", &fields),
    ],
  ))
}

/// Agency's settings, asked afresh at each call so the owner's change holds from the next one.
async fn settings_of(agency: &AgencyClient, policy: &TopUp) -> Result<Settings, String> {
  agency
    .get_settings()
    .await
    .map_err(|why| fill_marks(&policy.words.settings_unasked, &[("why", &why.to_string())]))
}

/// What the balance reader last read of DeepSeek's own road, as agency answers it.
async fn quota_of(agency: &AgencyClient, policy: &TopUp) -> Result<ProviderQuota, String> {
  agency
    .get_provider(DeepSeek.name())
    .await
    .map(|provider| provider.quota)
    .map_err(|why| fill_marks(&policy.words.balance_unasked, &[("why", &why.to_string())]))
}

/// A balance as the reader read it: the amount, its currency and when it was seen, in epoch seconds.
struct Reading {
  balance: f64,
  currency: String,
  seen: i64,
}

impl Reading {
  fn of(quota: ProviderQuota) -> Option<Self> {
    Some(Self {
      balance: quota.balance?,
      currency: quota.currency?,
      seen: quota.balance_seen_at?,
    })
  }

  /// The amount paid and this balance, as an answer says them.
  fn said(&self, paid: &str, policy: &TopUp) -> String {
    fill_marks(
      &policy.words.outcome,
      &[
        ("paid", paid),
        ("balance", &self.balance.to_string()),
        ("currency", &self.currency),
        ("seen", &seen_at(self.seen)),
      ],
    )
  }
}

/// The balance as the reader last read it; the reading taken before the call where agency answers none now.
async fn reading_now(agency: &AgencyClient, policy: &TopUp, before: Reading) -> Reading {
  match quota_of(agency, policy).await.ok().and_then(Reading::of) {
    Some(now) if now.seen >= before.seen => now,
    _ => before,
  }
}

/// The balance the reader read since a payment made at `at`, waited for within the payer's balance wait; the last
/// reading and false where none came.
async fn reading_after(
  agency: &AgencyClient,
  payer: &Payer,
  policy: &TopUp,
  at: i64,
  before: Reading,
) -> (Reading, bool) {
  let deadline = tokio::time::Instant::now() + payer.balance_wait;
  let mut last = before;

  loop {
    last = reading_now(agency, policy, last).await;

    if last.seen >= at {
      return (last, true);
    }

    if tokio::time::Instant::now() >= deadline {
      return (last, false);
    }

    tokio::time::sleep(payer.balance_look).await;
  }
}

/// The sentence a call that stopped before pay answers or is refused with: a person's step answered with the
/// balance, a fault refused.
fn stopped(stop: Stop, payer: &Payer, login: &str, policy: &TopUp) -> Result<String, String> {
  let words = &policy.words;

  match stop {
    Stop::Busy => Err(words.busy.clone()),
    Stop::FirefoxOpen => Err(words.firefox_open.clone()),
    Stop::Browser(why) => Err(fill_marks(&words.browser_failed, &[("why", &why)])),
    Stop::Lost(what) => Err(fill_marks(&words.browser_lost, &[("what", what)])),
    Stop::Unread(what) => Err(fill_marks(
      &words.page_unread,
      &[("what", what), ("seconds", &payer.platform.waits.step.as_secs().to_string())],
    )),
    Stop::PersonAsked => Ok(fill_marks(&words.person_asked, &[("address", &payer.platform.address)])),
    Stop::SignInRefused => {
      Ok(fill_marks(&words.sign_in_refused, &[("secret", login), ("kind", &policy.login.kind)]))
    }
    Stop::IdentityAsked => Ok(fill_marks(&words.identity_asked, &[("address", &payer.platform.address)])),
  }
}

/// One call checked and, where it passes every check, paid on DeepSeek's platform, answered with what was paid and
/// the balance after, or with what a person does where the platform or the card's issuer asks one.
pub async fn top_up_with(
  agency: &AgencyClient,
  data_root: &Path,
  payer: &Payer,
  input: &TopUpInput,
) -> Result<Answer, String> {
  let policy = &DEEPSEEK.top_up;
  let amount = amount_of(input, policy)?;
  let settings = settings_of(agency, policy).await?;
  let card_name = named(settings.deepseek_card_secret.as_deref(), CARD_SETTING, &policy.card, policy)?;
  let card = saved(data_root, &card_name, &policy.card, policy)?;
  let login_name = named(settings.deepseek_login_secret.as_deref(), LOGIN_SETTING, &policy.login, policy)?;
  let login = saved(data_root, &login_name, &policy.login, policy)?;

  let before =
    Reading::of(quota_of(agency, policy).await?).ok_or_else(|| policy.words.balance_unread.clone())?;
  let cap = policy
    .cap
    .get(&before.currency)
    .ok_or_else(|| fill_marks(&policy.words.no_cap, &[("currency", &before.currency)]))?;

  if amount > *cap {
    return Err(fill_marks(
      &policy.words.over_cap,
      &[
        ("amount", &amount.to_string()),
        ("cap", &cap.to_string()),
        ("currency", &before.currency),
        ("reason", &policy.cap_reason),
      ],
    ));
  }

  if !policy.platform.card_currencies.contains(&before.currency) {
    let road = fill_marks(
      &policy.words.no_card_road,
      &[("currency", &before.currency), ("top_up", &DEEPSEEK.balance.top_up)],
    );

    return Ok(Answer::text(format!("{road}\n{}", before.said("0", policy))));
  }

  let paid = match platform::pay(&payer.platform, &policy.page, &login, &card, &amount.to_string()).await {
    Ok(paid) => paid,
    Err(stop) => {
      let sentence = stopped(stop, payer, &login_name, policy)?;
      let now = reading_now(agency, policy, before).await;

      return Ok(Answer::text(format!("{sentence}\n{}", now.said("0", policy))));
    }
  };

  let answered = match paid {
    Paid::Paid { at } => {
      let (after, fresh) = reading_after(agency, payer, policy, at, before).await;
      let said = after.said(&amount.to_string(), policy);

      if fresh { said } else { format!("{said} {}", policy.words.balance_stale) }
    }
    Paid::Refused => format!(
      "{}\n{}",
      policy.words.refused,
      reading_now(agency, policy, before).await.said("0", policy)
    ),
    Paid::Unconfirmed => {
      let waited = payer.platform.waits.confirm.as_secs().to_string();
      let sentence = fill_marks(&policy.words.unconfirmed, &[("seconds", &waited)]);

      format!("{sentence}\n{}", reading_now(agency, policy, before).await.said("0", policy))
    }
  };

  Ok(Answer::text(answered))
}

async fn top_up(session: &Session, input: &TopUpInput) -> Result<Answer, String> {
  let data_root = session.data_root();
  let control = Control::read(&data_root, |name| std::env::var(name).ok());
  let agency = AgencyClient::new(&control.base(), control.token.unwrap_or_default())?;

  top_up_with(&agency, &data_root, &Payer::of(&data_root), input).await
}

/// The tool's entry point: one call, answered. A call that does not read is refused by a fixed sentence, never
/// by the reader's own words, which may quote what the call held.
pub async fn answer(handed: Handed) -> Result<Value, String> {
  let answered = match handed.asked {
    Asked::Call {
      entry: asked,
      arguments,
    } if asked == entry() => match TopUpInput::read(arguments) {
      Ok(input) => top_up(&handed.session, &input).await.unwrap_or_else(Answer::failure),
      Err(_) => Answer::failure("the call names amount, a number, and nothing else"),
    },
    Asked::Call { entry: asked, .. } => Answer::failure(format!("this executable holds no entry {asked}")),
    Asked::ResourcesList => return Ok(json!({ "resources": [] })),
    Asked::ResourcesRead { .. } => return Ok(json!({ "error": "this executable serves no resources" })),
    Asked::Other => return Err("this executable answers calls alone".to_owned()),
  };

  serde_json::to_value(answered).map_err(|why| format!("the answer could not be written: {why}"))
}
