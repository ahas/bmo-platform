//! The Claude accounts: a setting read as a whole number, a credential's stated end, a login closed by a
//! window of its own agency keeps read full — until the reset it names, or while its reading stands where
//! it names none — the chosen account serving only while it can answer, and each account's view carrying
//! its own two headline windows whichever road the chat stands on.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use agency_server::control::claude_accounts::*;
use agency_server::now_ms;
use cache_siren::reader::Passing;
use cache_siren::{Keeping, Reader, Slot, Window};

use crate::support::grounds::signed_in;
use crate::support::scratch::Scratch;

/// A reader of Claude Code's windows whose reading stands for half an hour, and which passes over nothing.
struct HalfHour;

impl Reader for HalfHour {
  fn mode(&self) -> &str {
    "claude-code"
  }

  fn cadence(&self) -> Duration {
    Duration::from_secs(300)
  }

  fn fresh(&self) -> Duration {
    Duration::from_secs(1800)
  }

  fn pass<'a>(&'a self, _keeping: &'a Keeping, _now_ms: i64) -> Passing<'a> {
    Box::pin(async {})
  }
}

const SPARE: Slot<'static> = Slot {
  mode: "claude-code",
  account: "spare",
};

// A full window that names its reset closes its login until that reset however old its reading is, since
// only a newer reading can say it came back early; one that names none closes it only while its reading
// stands (docs/cache-fallback.md, 「The account judgement」).
#[tokio::test]
async fn a_login_rests_until_a_full_window_s_reset_and_on_one_naming_none_only_while_its_reading_stands() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let now = now_ms() / 1000;
  let resets_at = now + 600;
  let read = Keeping::new(db.clone(), vec![Arc::new(HalfHour)]);
  let five_hour = |used: f64, resets_at: Option<i64>| Window {
    name: "five_hour".into(),
    utilization: used,
    resets_at,
    duration_minutes: Some(300),
    limit_name: None,
  };
  let rest = |windows| async move {
    rest_until(
      Judging {
        db,
        windows,
        data_root: Path::new("/nonexistent"),
      },
      "spare",
    )
    .await
    .unwrap()
  };

  read.replace(SPARE, &[five_hour(1.0, Some(resets_at))], now - 10).await.unwrap();

  assert_eq!(rest(&read).await, Some(resets_at));

  read.mark_unread("spare", now - 10).await.unwrap();

  assert_eq!(
    rest(&read).await,
    Some(resets_at),
    "an unread mark does not reopen a window full until its reset"
  );

  read.lift_unread("spare").await.unwrap();
  read.replace(SPARE, &[five_hour(1.0, Some(resets_at))], now - 3600).await.unwrap();

  assert_eq!(
    rest(&read).await,
    Some(resets_at),
    "a reading past its span still closes it until its reset"
  );

  read.replace(SPARE, &[five_hour(1.0, Some(now - 60))], now - 3600).await.unwrap();

  assert_eq!(rest(&read).await, None, "a window whose reset has passed closes nothing");

  read.replace(SPARE, &[five_hour(1.0, None)], now - 10).await.unwrap();

  assert!(
    rest(&read).await.is_some_and(|until| until > now),
    "a fresh full window naming no reset closes it"
  );

  read.replace(SPARE, &[five_hour(1.0, None)], now - 3600).await.unwrap();

  assert_eq!(rest(&read).await, None, "a stale full window naming no reset closes nothing");

  read.replace(SPARE, &[five_hour(0.995, Some(resets_at))], now - 10).await.unwrap();

  assert_eq!(rest(&read).await, None, "a newer reading short of full, half a percent left, opens it");

  scratch.run(r#"DROP TABLE "usage_windows""#, vec![]).await;

  assert_eq!(rest(&read).await, None, "a keeping that cannot be read closes nothing");
}

// A chosen account that cannot answer is not the one serving: the first open account is, and once the
// chosen one can answer again it heads the walk and serves (docs/cache-fallback.md, 「Running on one account
// now」).
#[tokio::test]
async fn a_chosen_account_spent_until_its_reset_is_not_serving_and_serves_again_once_it_reopens() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let now = now_ms() / 1000;
  let windows = Keeping::new(db.clone(), vec![Arc::new(HalfHour)]);
  let week = |used: f64| Window {
    name: "seven_day".into(),
    utilization: used,
    resets_at: Some(now + 3 * 86_400),
    duration_minutes: None,
    limit_name: None,
  };
  let judging = Judging {
    db,
    windows: &windows,
    data_root: Path::new("/nonexistent"),
  };
  let serving = || async {
    views(judging)
      .await
      .unwrap()
      .into_iter()
      .filter(|view| view["serving"] == true)
      .map(|view| view["id"].as_str().unwrap_or_default().to_owned())
      .collect::<Vec<_>>()
  };

  let logins = tempfile::tempdir().unwrap();
  let (first_home, second_home) = (logins.path().join("first"), logins.path().join("second"));

  signed_in(&first_home);
  signed_in(&second_home);
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "claude_accounts" ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at")
         VALUES ('first', 'first', 1, 1, 'directory', $1, 0),
                ('second', 'second', 2, 1, 'directory', $2, 0)"#,
      vec![
        first_home.to_string_lossy().into_owned().into(),
        second_home.to_string_lossy().into_owned().into(),
      ],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('claude_account_chosen', 'first'),
           ('claude_account_in_use', 'first')"#,
      vec![],
    )
    .await;

  // Its week is full until a day three days away, read an hour ago by a reading no reader renewed since.
  let first = Slot {
    mode: "claude-code",
    account: "first",
  };

  windows.replace(first, &[week(1.0)], now - 3600).await.unwrap();

  assert_eq!(serving().await, vec!["second".to_owned()]);
  assert_eq!(
    open_accounts(judging)
      .await
      .unwrap()
      .iter()
      .map(|one| one.id.as_str())
      .collect::<Vec<_>>(),
    vec!["second"]
  );

  // Its window reopens, and the next run goes out on the chosen account again, ahead of the one in use.
  scratch
    .run(
      r#"UPDATE "settings" SET "value" = 'second' WHERE "key" = 'claude_account_in_use'"#,
      vec![],
    )
    .await;
  windows.replace(first, &[week(0.1)], now - 10).await.unwrap();

  assert_eq!(serving().await, vec!["first".to_owned()]);
}

#[tokio::test]
async fn each_account_s_view_carries_its_own_headline_windows_and_an_endpoint_account_none() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let windows = Keeping::new(db.clone(), vec![Arc::new(HalfHour)]);
  let window = |name: &str, used: f64| Window {
    name: name.into(),
    utilization: used,
    resets_at: None,
    duration_minutes: None,
    limit_name: None,
  };

  scratch
    .run(
      r#"INSERT INTO "claude_accounts"
           ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at",
            "endpoint_url", "key_file")
         VALUES ('main', 'one', 1, 1, 'directory', '/logins/one', 0, NULL, NULL),
                ('spare', 'two', 2, 0, 'token', '/logins/two.token', 0, NULL, NULL),
                ('paid', 'three', 3, 1, 'endpoint', '/logins/paid', 0, 'https://endpoint.invalid', '/keys/paid')"#,
      vec![],
    )
    .await;
  windows
    .replace(
      Slot {
        mode: "claude-code",
        account: "main",
      },
      &[window("five_hour", 0.25), window("seven_day", 0.5)],
      100,
    )
    .await
    .unwrap();
  windows.replace(SPARE, &[window("seven_day", 0.75)], 100).await.unwrap();
  windows
    .replace(
      Slot {
        mode: "claude-code",
        account: "paid",
      },
      &[window("five_hour", 1.0)],
      100,
    )
    .await
    .unwrap();

  let views = views(Judging {
    db,
    windows: &windows,
    data_root: Path::new("/nonexistent"),
  })
  .await
  .unwrap();
  let of = |id: &str| views.iter().find(|view| view["id"] == id).unwrap();

  assert_eq!(of("main")["session"]["utilization"], 0.25);
  assert_eq!(of("main")["session"]["seenAt"], 100);
  assert_eq!(of("main")["week"]["utilization"], 0.5);
  assert_eq!(
    of("spare")["session"],
    serde_json::Value::Null,
    "an account off still answers what it holds"
  );
  assert_eq!(of("spare")["week"]["utilization"], 0.75);
  assert_eq!(of("paid")["session"], serde_json::Value::Null, "an endpoint account holds no windows");
  assert_eq!(of("paid")["week"], serde_json::Value::Null);
}

// An account's reported windows are its answering login's alone: none read at or before its unread mark or
// its mark of waiting for a person, its own back once a reading follows, and none kept once its home is
// pointed elsewhere (docs/usage-windows.md, 「What reads the windows」).
#[tokio::test]
async fn an_account_reports_only_the_windows_of_the_login_answering_for_it() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let now = now_ms() / 1000;
  let windows = Keeping::new(db.clone(), vec![Arc::new(HalfHour)]);
  let judging = Judging {
    db,
    windows: &windows,
    data_root: Path::new("/nonexistent"),
  };
  let week = |used: f64| Window {
    name: "seven_day".into(),
    utilization: used,
    resets_at: Some(now + 86_400),
    duration_minutes: None,
    limit_name: None,
  };
  let reported = || async {
    views(judging)
      .await
      .unwrap()
      .into_iter()
      .map(|view| (view["id"].as_str().unwrap_or_default().to_owned(), view["week"]["utilization"].clone()))
      .collect::<Vec<_>>()
  };
  let slot = |account| Slot {
    mode: "claude-code",
    account,
  };

  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "claude_accounts" ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at", "needs_login_at")
         VALUES ('main', 'main', 1, 1, 'directory', '/logins/main', 0, NULL),
                ('sub', 'sub', 2, 1, 'directory', '/logins/sub', 0, $1)"#,
      vec![(now - 60).into()],
    )
    .await;
  windows.replace(slot("main"), &[week(0.97)], now - 120).await.unwrap();
  windows.replace(slot("sub"), &[week(0.2)], now - 120).await.unwrap();
  windows.mark_unread("main", now - 90).await.unwrap();

  assert_eq!(
    reported().await,
    vec![
      ("main".to_owned(), serde_json::Value::Null),
      ("sub".to_owned(), serde_json::Value::Null)
    ],
    "a token found expired and a login waiting for a person each hide what was read before"
  );

  windows.replace(slot("main"), &[week(0.5)], now - 10).await.unwrap();
  windows.lift_unread("main").await.unwrap();

  assert_eq!(
    reported().await[0],
    ("main".to_owned(), serde_json::json!(0.5)),
    "its own reading comes back"
  );

  // Pointed at another home, the account keeps none of the windows the login before it was read with.
  let elsewhere = tempfile::tempdir().unwrap();

  agency_server::control::claude_account_changes::update(
    db,
    "main",
    None,
    Some(&agency_server::control::claude_account_changes::CredentialGiven {
      kind: None,
      path: Some(elsewhere.path().to_string_lossy().into_owned()),
      endpoint: None,
      key_file: None,
      models: None,
    }),
    None,
    agency_server::control::claude_account_changes::BasePromptGiven::Keep,
  )
  .await
  .unwrap();

  assert!(windows.read(slot("main")).await.unwrap().windows.is_empty());
  assert_eq!(windows.read(slot("sub")).await.unwrap().windows.len(), 1, "another account's stay");
}

#[test]
fn a_setting_reads_as_parseint_would() {
  assert_eq!(leading_integer("1790000000"), Some(1_790_000_000));
  assert_eq!(leading_integer(" 12abc"), Some(12));
  assert_eq!(leading_integer("-5"), Some(-5));
  assert_eq!(leading_integer("x1"), None);
}

#[test]
fn the_earliest_moment_under_an_expiring_name_answers_in_milliseconds() {
  let dir = tempfile::tempdir().unwrap();
  let account = Account {
    id: "a".into(),
    name: "a".into(),
    place: 1,
    enabled: true,
    credential: Credential::Directory {
      home: dir.path().into(),
    },
    needs_login_at: Some(1),
    user_id: None,
    unit_id: None,
    node: None,
    lendable: false,
    base_prompt: None,
    node_token_gone: None,
  };

  std::fs::write(
    dir.path().join(CREDENTIAL_FILE),
    r#"{"claudeAiOauth":{"expiresAt":1790000000000,"refreshToken":"r"},"other":{"tokenExpiry":1780000000},"small":{"expires":5}}"#,
  )
  .unwrap();

  assert_eq!(credential_expiry(&account), Some(1_780_000_000_000.0));
}

// The list tells a login as the account judgement holds it: a directory account whose CLI was never signed
// in here is `none`, never `standing`, and stands once its credentials file does (docs/cache-fallback.md,
// 「A login that is about to end」).
#[tokio::test]
async fn an_account_never_signed_in_here_is_listed_as_holding_no_login() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let windows = Keeping::new(db.clone(), vec![Arc::new(HalfHour)]);
  let judging = Judging {
    db,
    windows: &windows,
    data_root: Path::new("/nonexistent"),
  };
  let logins = tempfile::tempdir().unwrap();
  let home = logins.path().join("fresh");

  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "claude_accounts" ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at")
         VALUES ('fresh', 'fresh', 1, 1, 'directory', $1, 0)"#,
      vec![home.to_string_lossy().into_owned().into()],
    )
    .await;

  let login = || async { views(judging).await.unwrap()[0]["login"].clone() };

  assert_eq!(login().await, "none", "a CLI never signed in holds no login");

  signed_in(&home);

  assert_eq!(login().await, "standing", "a credentials file that stands is a login");
}
