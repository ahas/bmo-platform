//! What DeepSeek declares (docs/provider-presence.md): its own API, reached by Claude Code through agency's
//! gateway member.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::Declared;

#[derive(Debug, Deserialize)]
pub struct DeepSeek {
  #[serde(flatten)]
  pub declared: Declared,
  pub presence: Presence,
  pub balance: super::Balance,
  pub top_up: TopUp,
  /// The Claude Code effort word a run of each DeepSeek word goes out with.
  #[serde(default)]
  pub claude: Option<super::Claude>,
}

/// The road's key file, and what the provider says where it cannot serve a run.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presence {
  pub key_file: String,
  pub missing_code: String,
  pub missing: String,
  pub not_held: String,
}

/// The top-up tool's reading (docs/tools.md, 「The DeepSeek top-up」): the kinds and fields of its two secrets, the
/// cap on one call by currency, the platform it pays on, its waits, the words it finds the platform's controls
/// and outcomes by, and what it answers. Which secrets hold the card and the login are agency's settings, not
/// policy.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopUp {
  pub card: SavedSecret,
  pub login: SavedSecret,
  pub cap_reason: String,
  /// The largest amount one call names, by currency.
  pub cap: BTreeMap<String, f64>,
  pub platform: Platform,
  pub waits: Waits,
  pub mail: MailRoad,
  pub page: PageWords,
  pub words: TopUpWords,
}

/// A secret of the console's store the tool reads: what its answers call it, the console's kind it is saved in
/// and the field keys it holds, each one line.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedSecret {
  pub called: String,
  pub kind: String,
  pub fields: Vec<String>,
}

/// DeepSeek's web platform and the owner's own Firefox that pays on it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Platform {
  /// The platform's origin.
  pub address: String,
  pub top_up_path: String,
  pub sign_in_path: String,
  /// The currencies a balance is paid in by card.
  pub card_currencies: Vec<String>,
  /// Firefox's executable, by its absolute path.
  pub firefox: String,
  /// The folder Firefox keeps its profiles and their list, `profiles.ini`, in, from `$HOME`.
  pub profiles: String,
  /// The user scope a snap's launcher expects to find Firefox started in, without its unique suffix; empty for a
  /// Firefox that is no snap.
  pub scope: String,
  /// The virtual screen Firefox is started on.
  pub screen: String,
}

/// How long each wait of a call lasts, in seconds, and how often a waited condition is looked at, in
/// milliseconds.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Waits {
  /// For the browser to announce its WebDriver BiDi endpoint, and to answer each command on it.
  pub start: u64,
  /// For each step's page or control to show.
  pub step: u64,
  /// For the outcome of a payment pressed, a person's confirmation included.
  pub confirm: u64,
  /// For the balance reader to read the balance after a payment.
  pub balance: u64,
  /// For DeepSeek's mail approving the sign-in's check.
  pub mail: u64,
  pub look_ms: u64,
  pub balance_look: u64,
  pub mail_look: u64,
}

/// The mail that approves the sign-in's check: the domains DeepSeek mails from and links to, the words an approval
/// link's address or text holds, and how many digits a sign-in code has.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailRoad {
  pub senders: Vec<String>,
  pub link_words: Vec<String>,
  pub code_digits: usize,
}

/// The words the platform's controls and outcomes are found by, as its English pages show them; a control is
/// found by any of its words, matched without regard to case.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageWords {
  /// The sign-in form's fields, by their placeholders, and its button.
  pub identity: Vec<String>,
  pub password: Vec<String>,
  pub password_tab: Vec<String>,
  pub sign_in: Vec<String>,
  /// The top-up form's controls, by their own words or the names their marks are given for a reader.
  pub custom_amount: Vec<String>,
  pub amount: Vec<String>,
  pub card_method: Vec<String>,
  pub next_step: Vec<String>,
  pub pay: Vec<String>,
  /// The card's fields in the payment processor's frames, by their placeholders, labels or names, beside their
  /// autocomplete purposes.
  pub card_number: Vec<String>,
  pub card_expiry: Vec<String>,
  pub card_cvc: Vec<String>,
  pub card_holder: Vec<String>,
  /// What a page shows: a payment made, a payment refused, a sign-in refused, a person asked for before the
  /// sign-in goes on, by the words a page shows or the frame a human check stands in, and an identity check asked
  /// for before a payment.
  pub paid: Vec<String>,
  pub refused: Vec<String>,
  pub sign_in_refused: Vec<String>,
  pub person_asked: Vec<String>,
  /// The hosts of the frames a human check stands in.
  pub person_asked_frames: Vec<String>,
  pub identity_asked: Vec<String>,
  /// The sign-in's check of the account, by the words it shows; its control that sends a code, its code field by
  /// its placeholder or label, and its confirmation.
  pub check_asked: Vec<String>,
  pub send_code: Vec<String>,
  pub code_field: Vec<String>,
  pub code_submit: Vec<String>,
}

/// What the top-up tool answers, each `{name}` filled in with what it names.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopUpWords {
  pub no_amount: String,
  pub settings_unasked: String,
  pub secret_unnamed: String,
  pub secret_missing: String,
  pub secret_lacks: String,
  pub balance_unasked: String,
  pub balance_unread: String,
  pub no_cap: String,
  pub over_cap: String,
  pub no_card_road: String,
  pub busy: String,
  pub firefox_open: String,
  pub browser_failed: String,
  pub browser_lost: String,
  pub page_unread: String,
  pub person_asked: String,
  pub sign_in_refused: String,
  pub identity_asked: String,
  pub refused: String,
  pub unconfirmed: String,
  pub outcome: String,
  pub balance_stale: String,
}

crate::part!(pub DEEPSEEK: DeepSeek = "providers/deepseek.toml");
