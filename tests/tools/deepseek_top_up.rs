//! The DeepSeek top-up's checks (docs/tools.md, 「The DeepSeek top-up」): a call naming no amount, a setting naming
//! no card or login secret, a secret lacking a field, an amount over the cap, a balance in a currency the card road
//! does not serve, a profile another call holds and a profile the owner's Firefox has open are refused or answered
//! before any browser starts; each secret is read from the one its setting names at the call; no secret value
//! reaches an answer. The owner's Firefox is found by the default profile its `profiles.ini` names, and started
//! with no bus, display or remote of his session. Agency is a stand-in on the loopback that answers its settings
//! and the balance and records every request, the profiles are a throwaway folder's, the browser an executable that
//! stands nowhere and the platform's address one nothing listens on, so a test here starts no browser, reaches no
//! network beyond the stand-in and pays nothing.

use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::{Method, StatusCode, Uri};
use axum::{Json, Router};
use bmo_agency_client::AgencyClient;
use deepseek_top_up_tool::{Payer, Place, TopUpInput, answer, default_profile, launch_line, top_up_with};
use serde_json::{Value, json};
use tool_call::{Asked, Handed, PROTOCOL, Session};

pub(super) const NUMBER: &str = "4000123412341234";
pub(super) const CVC: &str = "987";
pub(super) const HOLDER: &str = "TEST HOLDER";
pub(super) const EXPIRY: &str = "12/31";
pub(super) const USERNAME: &str = "tester@example.com";
pub(super) const PASSWORD: &str = "pass-Word 42";
const SEEN_AT: i64 = 1_794_700_000;
/// The console secrets the card and the login are saved in, as the stand-in's settings name them.
pub(super) const CARD: &str = "saved-card";
pub(super) const LOGIN: &str = "saved-login";

/// What the stand-in agency answers, and every request it was asked: the method and the path. `status` is
/// the balance read's; the settings are always answered. Where `ledger` holds a balance, the balance is read from it
/// and seen at the moment it is asked, as a balance reader that has just read it.
#[derive(Clone)]
pub(super) struct Stand {
  status: StatusCode,
  balance: Option<f64>,
  currency: &'static str,
  card_secret: Option<&'static str>,
  login_secret: Option<&'static str>,
  ledger: Option<Arc<Mutex<f64>>>,
  asked: Arc<Mutex<Vec<(Method, String)>>>,
}

impl Stand {
  pub(super) fn kept(balance: Option<f64>) -> Self {
    Self {
      status: StatusCode::OK,
      balance,
      currency: "USD",
      card_secret: Some(CARD),
      login_secret: Some(LOGIN),
      ledger: None,
      asked: Arc::default(),
    }
  }

  /// A stand-in reading the balance a fake platform keeps, freshly at each ask.
  pub(super) fn reading(ledger: Arc<Mutex<f64>>) -> Self {
    Self {
      ledger: Some(ledger),
      ..Self::kept(Some(0.0))
    }
  }

  fn naming(card_secret: Option<&'static str>, login_secret: Option<&'static str>) -> Self {
    Self {
      card_secret,
      login_secret,
      ..Self::kept(Some(3.5))
    }
  }

  fn failing() -> Self {
    Self {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      ..Self::kept(Some(3.5))
    }
  }

  pub(super) fn asked(&self) -> Vec<(Method, String)> {
    self.asked.lock().unwrap().clone()
  }
}

async fn route(State(stand): State<Stand>, method: Method, uri: Uri) -> (StatusCode, Json<Value>) {
  stand.asked.lock().unwrap().push((method, uri.path().to_owned()));

  if uri.path() == "/settings" {
    let settings = json!({
      "researchSupervisor": null, "downgradeReturn": null,
      "deepseekCardSecret": stand.card_secret, "deepseekLoginSecret": stand.login_secret,
      "deployFlushMinutes": 60,
    });

    return (StatusCode::OK, Json(settings));
  }

  if stand.status != StatusCode::OK {
    return (stand.status, Json(json!({ "code": "broken", "error": "the stand-in fails" })));
  }

  let (balance, seen) = match &stand.ledger {
    Some(ledger) => (Some(*ledger.lock().unwrap()), chrono::Utc::now().timestamp()),
    None => (stand.balance, SEEN_AT),
  };
  let read = balance.is_some();
  let answer = json!({
    "name": "deepseek", "code": "deepseek", "label": "DeepSeek", "enabled": true, "registered": false,
    "gateway": true, "ownersEngine": false, "lines": ["chat"], "reason": null, "reasonCode": null, "foldRatioDefault": 0.5,
    "quota": {
      "has": balance.is_some_and(|balance| balance > 0.0), "left": null, "window": null,
      "balance": balance, "currency": read.then_some(stand.currency),
      "balanceSeenAt": read.then_some(seen), "requests": null, "resetsAt": null,
    },
  });

  (StatusCode::OK, Json(answer))
}

/// The stand-in agency on a loopback port, answering its settings and every other route as the provider's
/// read; the server task ends with the test's runtime.
pub(super) async fn agency(stand: &Stand) -> AgencyClient {
  let app = Router::new().fallback(route).with_state(stand.clone());
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  AgencyClient::new(&format!("http://127.0.0.1:{port}"), "test-token").unwrap()
}

/// A data root whose card and login secrets hold the lines given, as the console's store writes them.
pub(super) fn data_root(
  card: Option<&[(&str, &str)]>,
  login: Option<&[(&str, &str)]>,
) -> tempfile::TempDir {
  let root = tempfile::tempdir().unwrap();
  let env = root.path().join("env");

  std::fs::create_dir_all(&env).unwrap();

  for (name, lines) in [(CARD, card), (LOGIN, login)] {
    if let Some(lines) = lines {
      let text: String = lines.iter().map(|(key, value)| format!("{key}={value}\n")).collect();

      std::fs::write(env.join(name), text).unwrap();
    }
  }

  root
}

pub(super) fn whole_card() -> Vec<(&'static str, &'static str)> {
  vec![
    ("CARD_NUMBER", NUMBER),
    ("CARD_EXPIRY", EXPIRY),
    ("CARD_CVC", CVC),
    ("CARD_HOLDER_NAME", HOLDER),
  ]
}

pub(super) fn whole_login() -> Vec<(&'static str, &'static str)> {
  vec![("USERNAME", USERNAME), ("PASSWORD", PASSWORD)]
}

fn saved_root() -> tempfile::TempDir {
  data_root(Some(&whole_card()), Some(&whole_login()))
}

pub(super) fn amount(amount: f64) -> TopUpInput {
  TopUpInput {
    amount: Some(amount),
  }
}

/// The session and settings a throwaway profile is planted with, by their path in the profile, each a file of its
/// own words. The owner's preferences hold settings of his that equal what Firefox's remote agent recommends for
/// automation, which a browser applying those would leave out of what it writes as it ends, and keep the test's
/// browser off Mozilla's servers. He keeps no user file, as the owner's profile holds none.
const PLANTED: [(&str, &str); 3] = [
  ("sessionstore.jsonlz4", "the owner's last session"),
  ("sessionstore-backups/recovery.jsonlz4", "the owner's recovered tabs"),
  ("prefs.js", OWNER_PREFERENCES),
];

const OWNER_PREFERENCES: &str = r#"user_pref("browser.safebrowsing.malware.enabled", false);
user_pref("browser.urlbar.suggest.searches", false);
user_pref("browser.newtabpage.activity-stream.showSponsoredTopSites", false);
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("datareporting.policy.dataSubmissionPolicyBypassNotification", true);
user_pref("toolkit.telemetry.server", "data:,");
user_pref("services.settings.server", "data:,#remote-settings-dummy/v1");
user_pref("network.captive-portal-service.enabled", false);
user_pref("network.connectivity-service.enabled", false);
user_pref("app.normandy.enabled", false);
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("startup.homepage_welcome_url", "about:blank");
user_pref("browser.cache.disk.enable", false);
"#;

/// The name every throwaway profiles folder begins with, here and in the cache root Firefox mirrors one into.
const THROWAWAY: &str = ".top-up-test-";

/// How old a throwaway folder or mirror is before a later run takes it for one a killed run left.
const LEFT_BY_A_KILLED_RUN: Duration = Duration::from_secs(60 * 60);

/// The cache root Firefox mirrors a profile of the profiles root `profiles` into: a profile standing under
/// `<home>/.mozilla/firefox` keeps its local data, its startup cache and safe-browsing lists among them, in the
/// folder of the same path under `<home>/.cache/mozilla/firefox`, whatever its preferences say, and a snap's
/// Firefox takes no `XDG_CACHE_HOME` from its caller. None where `profiles` is no such root.
pub(super) fn cache_root(profiles: &Path) -> Option<PathBuf> {
  let mozilla = profiles.parent()?;

  if profiles.file_name()? != "firefox" || mozilla.file_name()? != ".mozilla" {
    return None;
  }

  Some(mozilla.parent()?.join(".cache/mozilla/firefox"))
}

/// Every throwaway folder in each of `folders` past the hour, by its prefix alone, removed: what killed runs left,
/// never a folder of another name.
pub(super) fn sweep_killed_runs(folders: &[&Path]) {
  for folder in folders {
    for entry in std::fs::read_dir(folder).into_iter().flatten().flatten() {
      let left = entry.file_name().to_str().is_some_and(|name| name.starts_with(THROWAWAY))
        && entry
          .metadata()
          .and_then(|meta| meta.modified())
          .is_ok_and(|at| at.elapsed().is_ok_and(|age| age >= LEFT_BY_A_KILLED_RUN));

      if left {
        let _ = std::fs::remove_dir_all(entry.path());
      }
    }
  }
}

/// A throwaway Firefox profiles folder, removed when dropped together with the folder of its name in the cache
/// root Firefox mirrors profiles into, where one stands.
pub(super) struct Throwaway {
  folder: Option<tempfile::TempDir>,
  mirror: Option<PathBuf>,
}

impl Throwaway {
  pub(super) fn path(&self) -> &Path {
    self.folder.as_ref().map_or(Path::new(""), |folder| folder.path())
  }

  /// The folder kept past the drop, as a test that renames it asks; its mirror is still removed.
  pub(super) fn keep(mut self) -> PathBuf {
    self.folder.take().map(tempfile::TempDir::keep).unwrap_or_default()
  }
}

impl Drop for Throwaway {
  fn drop(&mut self) {
    if let Some(mirror) = self.mirror.as_ref().filter(|mirror| mirror.is_dir()) {
      let _ = std::fs::remove_dir_all(mirror);
    }
  }
}

/// A throwaway Firefox profiles folder made in `near`, as the owner's Firefox keeps his: `profiles.ini` naming the
/// one profile, `profile`, its default, which holds the planted session. `mirrored` is the machine's profiles
/// root, where the folder is made for its Firefox: the leftovers of killed runs are swept from `near`, from that
/// root and from its cache root first, and the folder's mirror there is removed with it.
pub(super) fn firefox_profiles(near: &Path, mirrored: Option<&Path>) -> Throwaway {
  let cache = mirrored.and_then(cache_root);
  let swept: Vec<&Path> = [Some(near), mirrored, cache.as_deref()].into_iter().flatten().collect();

  sweep_killed_runs(&swept);

  let made = tempfile::Builder::new().prefix(THROWAWAY).tempdir_in(near).unwrap();
  let root = Throwaway {
    mirror: cache.zip(made.path().file_name()).map(|(cache, name)| cache.join(name)),
    folder: Some(made),
  };

  std::fs::write(
    root.path().join("profiles.ini"),
    "[Profile0]\nName=default\nIsRelative=1\nPath=profile\nDefault=1\n\n[General]\nStartWithLastProfile=1\nVersion=2\n",
  )
  .unwrap();

  for (path, words) in PLANTED {
    let planted = root.path().join("profile").join(path);

    std::fs::create_dir_all(planted.parent().unwrap()).unwrap();
    std::fs::write(planted, words).unwrap();
  }

  root
}

/// The profile's session and settings stand as they were planted, nothing beside the session, no user file, and
/// neither the call's endpoint note nor its folder is left.
pub(super) fn assert_profile_as_planted(root: &Path) {
  let profile = root.join("profile");

  for (path, words) in PLANTED {
    assert_eq!(
      std::fs::read_to_string(profile.join(path)).unwrap(),
      words,
      "{path} was given back as it stood"
    );
  }

  let backups: Vec<_> = std::fs::read_dir(profile.join("sessionstore-backups"))
    .unwrap()
    .map(|entry| entry.unwrap().file_name())
    .collect();

  assert_eq!(backups, ["recovery.jsonlz4"], "nothing of the call's own session is left");
  assert!(!profile.join("user.js").exists(), "none of the call's preferences is left");
  assert!(
    !profile.join("WebDriverBiDiServer.json").exists(),
    "no note of the call's endpoint is left"
  );

  let left: Vec<_> = std::fs::read_dir(root)
    .unwrap()
    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
    .filter(|name| name.starts_with(".deepseek-top-up-"))
    .collect();

  assert!(left.is_empty(), "the call's folder is gone: {left:?}");
}

/// A payer on a throwaway profile under `root`'s `firefox`, with a browser that stands nowhere and a platform
/// nothing listens on: a call that reached the browser would fail there, not pay.
fn nowhere(root: &Path) -> Payer {
  let mut payer = Payer::of(root);
  let place = &mut payer.platform.place;

  place.profiles = root.join("firefox");
  place.firefox = root.join("no-firefox").display().to_string();
  place.scope = String::new();
  payer.platform.address = "http://127.0.0.1:9".to_owned();
  payer.balance_wait = Duration::ZERO;
  payer
}

/// The throwaway profiles folder `nowhere` pays from, made under `root`.
fn profiles_under(root: &Path) -> PathBuf {
  let made = firefox_profiles(root, None).keep();
  let named = root.join("firefox");

  std::fs::rename(&made, &named).unwrap();
  named
}

async fn top_up(stand: &Stand, root: &Path, input: TopUpInput) -> Result<String, String> {
  top_up_with(&agency(stand).await, root, &nowhere(root), &input)
    .await
    .map(|answered| answered.plain())
}

pub(super) fn holds_no_secret_value(said: &str) {
  if let Some(value) = secret_value_in(said) {
    panic!("a secret value stands in, {value}: {said}");
  }
}

/// The first secret value `said` holds. The card's short numbers, its last four and its code, are digits any
/// port, temporary folder or browser build an answer names may hold, so they count where they stand as a number
/// of their own outside a path or an address, as a card's ending is said.
fn secret_value_in(said: &str) -> Option<&'static str> {
  let last_four = &NUMBER[NUMBER.len() - 4..];
  let whole = [NUMBER, HOLDER, EXPIRY, USERNAME, PASSWORD]
    .into_iter()
    .find(|value| said.contains(value));
  let short = || {
    said
      .split_whitespace()
      .filter(|word| !word.contains('/'))
      .flat_map(|word| word.split(|letter: char| !letter.is_ascii_digit()))
      .find_map(|number| [last_four, CVC].into_iter().find(|value| *value == number))
  };

  whole.or_else(short)
}

#[test]
fn the_secret_check_finds_the_card_s_short_numbers_said_as_a_card_says_them_and_not_in_a_machine_s_places()
{
  let place = "A person signs the tool's browser in once with `$HOME/.cache/ms-playwright/chromium-1234/\
    chrome-linux64/chrome --user-data-dir=\"/tmp/.tmp1234ab/deepseek-top-up/profile\" \
    http://127.0.0.1:41234/sign_in`, then asks again on port 39870.";

  assert_eq!(secret_value_in(place), None);
  assert_eq!(secret_value_in("Paid 10 with the card ending 1234."), Some("1234"));
  assert_eq!(secret_value_in("The card's code 987 was refused."), Some(CVC));
}

fn reads_nothing_past_the_settings(stand: &Stand) {
  assert_eq!(
    stand.asked(),
    [(Method::GET, "/settings".to_owned())],
    "nothing past the settings is read"
  );
}

#[tokio::test]
async fn a_call_naming_no_amount_is_refused_and_reaches_nothing() {
  let stand = Stand::kept(Some(3.5));
  let root = saved_root();

  for input in [TopUpInput { amount: None }, amount(0.0), amount(-5.0), amount(f64::NAN)] {
    let refused = top_up(&stand, root.path(), input).await.unwrap_err();

    assert!(refused.starts_with("Name the amount"), "{refused}");
  }

  assert!(stand.asked().is_empty(), "a refused call asks agency nothing");
}

#[tokio::test]
async fn a_call_handed_no_arguments_or_a_card_is_refused_before_anything_is_read() {
  let root = saved_root();
  let handed = |arguments: Value| Handed {
    protocol: PROTOCOL,
    asked: Asked::Call {
      entry: deepseek_top_up_tool::entry(),
      arguments,
    },
    session: Session {
      variables: [("BMO_DATA_DIR".to_owned(), root.path().display().to_string())].into(),
      ..Session::default()
    },
  };

  let none = answer(handed(Value::Null)).await.unwrap();

  assert_eq!(none["isError"], true);
  assert!(none["content"][0]["text"].as_str().unwrap().contains("Name the amount"), "{none}");

  for wrong in [
    json!({ "amount": 10, "CARD_NUMBER": NUMBER }),
    json!({ "amount": 10, "PASSWORD": PASSWORD }),
  ] {
    let refused = answer(handed(wrong)).await.unwrap();

    assert_eq!(refused["isError"], true);
    assert_eq!(
      refused["content"][0]["text"].as_str().unwrap(),
      "ERROR: the call names amount, a number, and nothing else"
    );
  }

  let wrong = answer(handed(json!({ "amount": NUMBER }))).await.unwrap();

  holds_no_secret_value(&wrong.to_string());
}

#[tokio::test]
async fn a_setting_naming_no_secret_is_refused_with_the_setting_to_fill() {
  let root = saved_root();

  for unset in [None, Some(""), Some("  ")] {
    let card = Stand::naming(unset, Some(LOGIN));
    let refused = top_up(&card, root.path(), amount(10.0)).await.unwrap_err();

    assert!(
      refused.contains("fill agency's setting deepseekCardSecret") && refused.contains("kind card,"),
      "{refused}"
    );
    reads_nothing_past_the_settings(&card);

    let login = Stand::naming(Some(CARD), unset);
    let refused = top_up(&login, root.path(), amount(10.0)).await.unwrap_err();

    assert!(
      refused.starts_with("No DeepSeek login secret is named")
        && refused.contains("fill agency's setting deepseekLoginSecret")
        && refused.contains("kind account-login"),
      "{refused}"
    );
    reads_nothing_past_the_settings(&login);
  }
}

#[tokio::test]
async fn a_secret_standing_nowhere_is_refused_with_the_secret_to_ask_for() {
  let stand = Stand::kept(Some(3.5));

  let card = top_up(&stand, data_root(None, Some(&whole_login())).path(), amount(10.0))
    .await
    .unwrap_err();

  assert!(
    card.starts_with("No card is saved") && card.contains(CARD) && card.contains("kind card"),
    "{card}"
  );

  let login = top_up(&stand, data_root(Some(&whole_card()), None).path(), amount(10.0))
    .await
    .unwrap_err();

  assert!(
    login.starts_with("No DeepSeek login is saved")
      && login.contains(LOGIN)
      && login.contains("kind account-login"),
    "{login}"
  );
}

#[tokio::test]
async fn each_secret_is_read_from_the_one_its_setting_names_at_the_call() {
  let root = saved_root();

  let elsewhere = top_up(&Stand::naming(Some("other-card"), Some(LOGIN)), root.path(), amount(10.0))
    .await
    .unwrap_err();

  assert!(
    elsewhere.starts_with("No card is saved") && elsewhere.contains("other-card"),
    "{elsewhere}"
  );

  let elsewhere = top_up(&Stand::naming(Some(CARD), Some("other-login")), root.path(), amount(10.0))
    .await
    .unwrap_err();

  assert!(
    elsewhere.starts_with("No DeepSeek login is saved") && elsewhere.contains("other-login"),
    "{elsewhere}"
  );
}

#[tokio::test]
async fn a_secret_lacking_its_fields_is_refused_naming_the_keys_alone() {
  let stand = Stand::kept(Some(3.5));
  let card = [("CARD_NUMBER", NUMBER), ("CARD_CVC", " "), ("CARD_EXPIRY", EXPIRY)];
  let root = data_root(Some(&card), Some(&whole_login()));

  let refused = top_up(&stand, root.path(), amount(10.0)).await.unwrap_err();

  assert!(refused.contains("lacks CARD_CVC, CARD_HOLDER_NAME:"), "{refused}");
  holds_no_secret_value(&refused);
  reads_nothing_past_the_settings(&stand);

  let stand = Stand::kept(Some(3.5));
  let root = data_root(Some(&whole_card()), Some(&[("USERNAME", USERNAME)]));

  let refused = top_up(&stand, root.path(), amount(10.0)).await.unwrap_err();

  assert!(
    refused.starts_with("The DeepSeek login saved as saved-login lacks PASSWORD:"),
    "{refused}"
  );
  holds_no_secret_value(&refused);
  reads_nothing_past_the_settings(&stand);
}

#[tokio::test]
async fn an_amount_over_the_cap_is_refused_with_the_cap_and_its_reason() {
  let stand = Stand::kept(Some(3.5));
  let root = saved_root();

  let refused = top_up(&stand, root.path(), amount(50.01)).await.unwrap_err();

  assert!(refused.starts_with("50.01 USD is over the single top-up cap of 50 USD: "), "{refused}");
  assert!(refused.contains("a mistaken amount or a repeated call"), "{refused}");
  holds_no_secret_value(&refused);
}

#[tokio::test]
async fn a_balance_never_read_leaves_the_currency_unknown_and_is_refused() {
  let stand = Stand::kept(None);
  let root = saved_root();

  let refused = top_up(&stand, root.path(), amount(10.0)).await.unwrap_err();

  assert!(refused.starts_with("The DeepSeek balance has not been read yet"), "{refused}");
}

#[tokio::test]
async fn an_agency_that_cannot_answer_the_balance_refuses_the_call() {
  let stand = Stand::failing();
  let root = saved_root();

  let refused = top_up(&stand, root.path(), amount(10.0)).await.unwrap_err();

  assert!(refused.starts_with("The DeepSeek balance could not be read from agency:"), "{refused}");
}

#[tokio::test]
async fn a_balance_kept_in_yuan_is_paid_by_no_card_and_says_what_a_person_does() {
  let stand = Stand {
    currency: "CNY",
    ..Stand::kept(Some(3.5))
  };
  let root = saved_root();

  let said = top_up(&stand, root.path(), amount(100.0)).await.unwrap();

  assert!(
    said.starts_with("Nothing was paid: DeepSeek takes no card for a balance kept in CNY"),
    "{said}"
  );
  assert!(said.contains("https://platform.deepseek.com/top_up"), "{said}");
  assert!(
    said.ends_with(
      "Paid: 0 CNY. Balance: 3.5 CNY, as the balance reader last read it at 2026-11-14T23:46:40Z."
    ),
    "{said}"
  );
}

#[tokio::test]
async fn a_profile_another_call_holds_refuses_the_call_and_leaves_the_session_as_it_stood() {
  let stand = Stand::kept(Some(3.5));
  let root = saved_root();
  let profiles = profiles_under(root.path());
  let held = std::fs::File::create(profiles.join("profile/.parentlock")).unwrap();

  held.lock().unwrap();

  let refused = top_up(&stand, root.path(), amount(10.0)).await.unwrap_err();

  assert!(
    refused.starts_with("Nothing was paid: another DeepSeek top-up is under way"),
    "{refused}"
  );
  assert_profile_as_planted(&profiles);
}

/// A record lock on the whole file, as a Firefox that has its profile open holds one on the profile's
/// `.parentlock`, held as long as the file stays open.
fn record_lock(file: &std::fs::File) {
  // SAFETY: `flock` is plain integers, for which zero is a valid value of every field.
  let mut lock: libc::flock = unsafe { std::mem::zeroed() };

  lock.l_type = libc::F_WRLCK as libc::c_short;
  lock.l_whence = libc::SEEK_SET as libc::c_short;
  // SAFETY: `fcntl` reads the descriptor and the lock description, which outlives the call.
  assert_eq!(unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLK, &raw const lock) }, 0);
}

#[tokio::test]
async fn a_profile_the_owners_firefox_has_open_refuses_the_call_and_leaves_the_session_as_it_stood() {
  let stand = Stand::kept(Some(3.5));
  let root = saved_root();
  let profiles = profiles_under(root.path());
  let open = std::fs::File::create(profiles.join("profile/.parentlock")).unwrap();

  record_lock(&open);

  let refused = top_up(&stand, root.path(), amount(10.0)).await.unwrap_err();

  assert!(
    refused.starts_with("Nothing was paid: the owner's Firefox is open on this machine")
      && refused.ends_with("Call again once Firefox is closed."),
    "{refused}"
  );
  assert_profile_as_planted(&profiles);
}

#[tokio::test]
async fn a_browser_that_cannot_start_gives_the_owners_session_and_settings_back_as_they_stood() {
  let stand = Stand::kept(Some(3.5));
  let root = saved_root();
  let profiles = profiles_under(root.path());

  let refused = top_up(&stand, root.path(), amount(10.0)).await.unwrap_err();

  assert!(
    refused.starts_with("Nothing was paid: the browser the DeepSeek top-up pays in could not be started"),
    "{refused}"
  );
  holds_no_secret_value(&refused);
  assert_profile_as_planted(&profiles);
}

/// The default profile a `profiles.ini` of the lines given names, in a folder of its own.
fn default_of(lines: &str) -> Result<PathBuf, String> {
  let root = tempfile::tempdir().unwrap();

  std::fs::write(root.path().join("profiles.ini"), lines).unwrap();
  default_profile(root.path())
    .map(|found| found.strip_prefix(root.path()).map_or(found.clone(), Path::to_owned))
}

#[test]
fn the_owners_profile_is_the_one_firefox_opens_by_default() {
  let two = "[Profile1]\nName=work\nIsRelative=1\nPath=b.work\n\n[Profile0]\nName=default\nIsRelative=1\nPath=a.default\n";

  assert_eq!(
    default_of(&format!("{two}Default=1\n")).unwrap(),
    Path::new("a.default"),
    "the marked one"
  );
  assert_eq!(
    default_of(&format!("[Install4F96D1932A9F858E]\nDefault=b.work\nLocked=1\n\n{two}Default=1\n"))
      .unwrap(),
    Path::new("b.work"),
    "the install's default before the marked one"
  );
  assert_eq!(
    default_of("[Profile0]\nName=default\nIsRelative=1\nPath=only.default\n").unwrap(),
    Path::new("only.default"),
    "the only one"
  );
  assert_eq!(
    default_of("[Profile0]\nName=default\nIsRelative=0\nPath=/elsewhere/x.default\nDefault=1\n").unwrap(),
    Path::new("/elsewhere/x.default"),
    "an absolute path as it stands"
  );
  assert!(default_of(two).unwrap_err().ends_with("names no default profile"));
  assert!(default_profile(Path::new("/nowhere-at-all")).unwrap_err().contains("could not be read"));
}

#[test]
fn firefox_starts_in_the_snaps_scope_with_no_bus_display_or_remote_of_the_owners_session() {
  let place = Place {
    firefox: "/opt/firefox/firefox".to_owned(),
    profiles: PathBuf::from("/profiles"),
    scope: "snap.firefox.firefox".to_owned(),
    screen: "1280x900x24".to_owned(),
    start: Duration::from_secs(1),
  };
  let line = launch_line(&place, Path::new("/profiles/.call"), Path::new("/profiles/a.default"), "unique");
  let at = |word: &str| line.iter().position(|said| said == word).unwrap();
  let environment = &line[at("-i") + 1..at("xvfb-run")];

  assert_eq!(
    line[..at("-i") + 1],
    [
      "systemd-run",
      "--user",
      "--scope",
      "--quiet",
      "--collect",
      "--unit=snap.firefox.firefox-unique",
      "--",
      "env",
      "-i"
    ]
  );

  for set in [
    "DBUS_SESSION_BUS_ADDRESS=unix:path=/profiles/.call/no-bus",
    "XDG_RUNTIME_DIR=/profiles/.call/run",
    "DISABLE_WAYLAND=1",
    "GDK_BACKEND=x11",
  ] {
    assert!(environment.iter().any(|said| said == set), "{set} in {environment:?}");
  }

  assert!(
    !environment
      .iter()
      .any(|said| said.starts_with("WAYLAND_DISPLAY=") || said.starts_with("DISPLAY=")),
    "no display of the owner's session: {environment:?}"
  );
  assert!(line.contains(&"-f".to_owned()) && line.contains(&"/profiles/.call/Xauthority".to_owned()));
  assert_eq!(
    line[at("/opt/firefox/firefox")..],
    [
      "/opt/firefox/firefox",
      "--no-remote",
      "--profile",
      "/profiles/a.default",
      "--remote-debugging-port",
      "0",
      "about:blank"
    ]
  );

  let unscoped = launch_line(
    &Place {
      scope: String::new(),
      ..place
    },
    Path::new("/profiles/.call"),
    Path::new("/profiles/a.default"),
    "unique",
  );

  assert_eq!(unscoped[..2], ["env", "-i"], "no scope for a Firefox that is no snap");
}

#[tokio::test]
async fn no_refused_call_moves_the_balance_or_asks_agency_anything_but_its_reads() {
  let stand = Stand::kept(Some(3.5));
  let root = saved_root();

  for input in [amount(500.0), TopUpInput { amount: None }] {
    let _ = top_up(&stand, root.path(), input).await;
  }

  let failing = Stand::failing();
  let _ = top_up(&failing, root.path(), amount(10.0)).await;

  for (method, path) in stand.asked().into_iter().chain(failing.asked()) {
    assert_eq!(method, Method::GET);
    assert!(["/settings", "/providers/deepseek"].contains(&path.as_str()), "{path}");
  }
}

/// A folder made under `parent` and dated `age` ago.
fn dated(parent: &Path, name: &str, age: Duration) -> PathBuf {
  let folder = parent.join(name);

  std::fs::create_dir_all(folder.join("startupCache")).unwrap();
  std::fs::File::open(&folder)
    .unwrap()
    .set_modified(std::time::SystemTime::now() - age)
    .unwrap();
  folder
}

#[test]
fn firefox_mirrors_a_profile_of_its_profiles_root_into_the_cache_root_beside_it() {
  let snap = Path::new("/home/someone/snap/firefox/common");

  assert_eq!(cache_root(&snap.join(".mozilla/firefox")), Some(snap.join(".cache/mozilla/firefox")));
  assert_eq!(cache_root(Path::new("/tmp")), None);
}

#[test]
fn a_throwaway_profile_stands_outside_the_profiles_root_and_takes_its_mirror_with_it() {
  let home = tempfile::tempdir().unwrap();
  let (beside, profiles) = (home.path().join(".mozilla"), home.path().join(".mozilla/firefox"));
  let cache = home.path().join(".cache/mozilla/firefox");
  let owners = dated(&cache, "abcd1234.default", Duration::from_secs(7200));

  std::fs::create_dir_all(&profiles).unwrap();

  let throwaway = firefox_profiles(&beside, Some(&profiles));
  let name = throwaway.path().file_name().unwrap().to_owned();
  let mirror = dated(&cache, name.to_str().unwrap(), Duration::ZERO);

  assert_eq!(
    throwaway.path().parent(),
    Some(beside.as_path()),
    "the folder stands beside the root, outside it"
  );
  drop(throwaway);

  assert!(!beside.join(&name).exists());
  assert!(!mirror.exists(), "a mirror of its name went with it");
  assert!(owners.exists(), "the owner's own cache is never reached");
}

#[test]
fn a_run_sweeps_what_killed_runs_left_past_the_hour_by_its_prefix_alone() {
  let home = tempfile::tempdir().unwrap();
  let (beside, profiles) = (home.path().join(".mozilla"), home.path().join(".mozilla/firefox"));
  let cache = home.path().join(".cache/mozilla/firefox");
  let hours = Duration::from_secs(7200);
  let killed = [
    dated(&beside, ".top-up-test-killed", hours),
    dated(&profiles, ".top-up-test-older", hours),
    dated(&cache, ".top-up-test-killed", hours),
  ];
  let kept = [
    dated(&beside, ".top-up-test-running", Duration::ZERO),
    dated(&cache, ".top-up-test-running", Duration::ZERO),
    dated(&profiles, "abcd1234.default", hours),
    dated(&cache, "abcd1234.default", hours),
    dated(&cache, ".bmo-web-firefox-other", hours),
  ];

  let throwaway = firefox_profiles(&beside, Some(&profiles));

  for left in killed {
    assert!(!left.exists(), "{} was swept", left.display());
  }

  for standing in kept {
    assert!(standing.exists(), "{} stands", standing.display());
  }

  drop(throwaway);
}
