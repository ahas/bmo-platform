//! The Claude account reader against a stand-in endpoint: an answer read into windows as the parity cases
//! record them, each account asked with its own login and its windows written under it alone, an account
//! that cannot be asked keeping what it had, an endpoint that breaks the connection a failure that leaves
//! the account unmarked, a token that cannot be read with marking the account unread so its last full
//! reading stops standing until a reading lifts the mark, a directory account's login found expired or
//! refused renewed by one run of a stand-in CLI at a time and at a pace, and a failure said once when it
//! starts and its recovery once when it ends.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use cache_siren::claude::{ClaudeAccounts, Credential, Login, Logins, MODE, Renewals, windows_of};
use cache_siren::reader::Said;
use cache_siren::{Keeping, Reader, Slot, Window};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::{Notify, mpsc};

use crate::support::scratch::Scratch;

// Every moment here is fixed, in epoch milliseconds; nothing reads the real clock.
const NOW_MS: i64 = 1_800_000_000_000;
const NOW: i64 = NOW_MS / 1000;

// The token the stand-in endpoint refuses as unauthorised.
const REFUSED: &str = "sk-refused";

// The token the stand-in endpoint refuses for asking too often, naming two minutes.
const BUSY: &str = "sk-busy";

struct Fixed(Vec<Login>);

impl Logins for Fixed {
  fn logins(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Login>, String>> + Send + '_>> {
    Box::pin(async { Ok(self.0.clone()) })
  }
}

/// An account-usage endpoint answering every token but the refused one with the same windows, and the
/// authorisation each ask carried.
async fn usage_endpoint() -> (String, Arc<Mutex<Vec<String>>>) {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let asked = Arc::new(Mutex::new(Vec::new()));
  let heard = asked.clone();
  let app = Router::new().fallback(move |request: Request| {
    let heard = heard.clone();

    async move {
      let said = request
        .headers()
        .get("authorization")
        .map(|value| value.to_str().unwrap().to_owned())
        .unwrap_or_default();

      heard.lock().unwrap().push(said.clone());

      if said == format!("Bearer {REFUSED}") {
        return StatusCode::UNAUTHORIZED.into_response();
      }

      if said == format!("Bearer {BUSY}") {
        return (StatusCode::TOO_MANY_REQUESTS, [("retry-after", "120")]).into_response();
      }

      axum::Json(json!({
        "five_hour": { "utilization": 25, "resets_at": "2026-10-07T05:00:00+00:00" },
        "seven_day": { "utilization": 50 },
      }))
      .into_response()
    }
  });

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (format!("http://127.0.0.1:{port}/usage"), asked)
}

fn token_login(account: &str, file: std::path::PathBuf) -> Login {
  Login {
    account: account.into(),
    credential: Credential::Token(file),
  }
}

fn slot(account: &str) -> Slot<'_> {
  Slot {
    mode: MODE,
    account,
  }
}

fn full(name: &str) -> Window {
  Window {
    name: name.into(),
    utilization: 1.0,
    resets_at: Some(NOW + 3600),
    duration_minutes: None,
    limit_name: None,
  }
}

#[test]
fn an_answer_is_read_into_the_windows_the_parity_cases_record() {
  let cases = [
    (
      json!({
        "five_hour": { "utilization": 37.5, "resets_at": "2026-10-07T05:00:00.123456+00:00" },
        "seven_day": { "utilization": 12, "resets_at": null },
        "seven_day_opus": null,
      }),
      json!([
        { "name": "five_hour", "utilization": 0.375, "resetsAt": 1791349200, "durationMinutes": null, "limitName": null },
        { "name": "seven_day", "utilization": 0.12, "resetsAt": null, "durationMinutes": null, "limitName": null },
      ]),
    ),
    (json!({ "five_hour": { "utilization": null }, "seven_day": "none" }), json!([])),
    (json!([]), json!([])),
  ];

  for (answer, windows) in cases {
    assert_eq!(serde_json::to_value(windows_of(&answer)).unwrap(), windows, "{answer}");
  }
}

#[test]
fn a_reading_stands_thirty_one_minute_passes() {
  let reader = ClaudeAccounts::new("http://[::1]:9/", Arc::new(Fixed(vec![])));

  assert_eq!(reader.cadence(), Duration::from_secs(60));
  assert_eq!(reader.fresh(), Duration::from_secs(1800));
}

// An account the endpoint refuses for asking too often is not asked again until the moment the refusal's
// Retry-After names, and is asked again after it (docs/usage-windows.md, 「The Claude account reader」).
#[tokio::test]
async fn an_account_refused_for_asking_too_often_is_left_unasked_until_the_moment_its_refusal_names() {
  let scratch = Scratch::new().await;
  let dir = tempfile::tempdir().unwrap();
  let token = dir.path().join("token");

  std::fs::write(&token, format!("{BUSY}\n")).unwrap();

  let (url, asked) = usage_endpoint().await;
  let reader = Arc::new(ClaudeAccounts::new(url, Arc::new(Fixed(vec![token_login("busy", token)]))));
  let keeping = Keeping::new(scratch.db().clone(), vec![reader.clone()]);

  reader.pass(&keeping, NOW_MS).await;
  reader.pass(&keeping, NOW_MS + 60_000).await;

  assert_eq!(asked.lock().unwrap().len(), 1, "the refusal's two minutes have not passed");

  reader.pass(&keeping, NOW_MS + 121_000).await;

  assert_eq!(asked.lock().unwrap().len(), 2, "asked again once the moment it named has passed");
}

#[tokio::test]
async fn each_account_is_read_with_its_own_login_and_one_that_cannot_be_asked_keeps_what_it_had() {
  let scratch = Scratch::new().await;
  let dir = tempfile::tempdir().unwrap();
  let token = dir.path().join("token");

  std::fs::write(&token, "  sk-token-one \n").unwrap();

  let (url, asked) = usage_endpoint().await;
  let reader = Arc::new(ClaudeAccounts::new(
    url,
    Arc::new(Fixed(vec![token_login("one", token), token_login("two", dir.path().join("gone"))])),
  ));
  let keeping = Keeping::new(scratch.db().clone(), vec![reader.clone()]);
  let mut kept = full("five_hour");

  kept.utilization = 0.9;
  keeping.replace(slot("two"), &[kept.clone()], NOW - 60).await.unwrap();
  reader.pass(&keeping, NOW_MS).await;

  assert_eq!(
    *asked.lock().unwrap(),
    ["Bearer sk-token-one"],
    "a token goes in its own account's ask alone"
  );

  let one = keeping.read(slot("one")).await.unwrap();
  let read: Vec<Value> =
    one.windows.iter().map(|kept| serde_json::to_value(&kept.window).unwrap()).collect();

  assert_eq!(
    read,
    [
      json!({ "name": "five_hour", "utilization": 0.25, "resetsAt": 1791349200, "durationMinutes": null, "limitName": null }),
      json!({ "name": "seven_day", "utilization": 0.5, "resetsAt": null, "durationMinutes": null, "limitName": null }),
    ]
  );
  assert!(one.windows.iter().all(|window| window.seen_at == NOW));
  assert_eq!(keeping.read(slot("two")).await.unwrap().windows[0].window, kept);
}

#[tokio::test]
async fn a_full_reading_the_reader_cannot_renew_for_its_token_stops_standing_until_a_reading_lifts_the_mark()
 {
  let scratch = Scratch::new().await;
  let dir = tempfile::tempdir().unwrap();
  let expired = dir.path().join(".credentials.json");
  let refused = dir.path().join("refused");

  std::fs::write(
    &expired,
    json!({ "claudeAiOauth": { "accessToken": "sk-expired", "expiresAt": NOW_MS - 1 } }).to_string(),
  )
  .unwrap();
  std::fs::write(&refused, REFUSED).unwrap();

  let (url, asked) = usage_endpoint().await;
  let logins = vec![
    Login {
      account: "main".into(),
      credential: Credential::Directory(expired.clone()),
    },
    token_login("spare", refused.clone()),
  ];
  let reader = Arc::new(ClaudeAccounts::new(url, Arc::new(Fixed(logins))));
  let keeping = Keeping::new(scratch.db().clone(), vec![reader.clone()]);

  for account in ["main", "spare"] {
    keeping.replace(slot(account), &[full("five_hour")], NOW - 60).await.unwrap();
  }

  let outcomes = reader.read_each(&keeping, NOW_MS).await.unwrap();

  assert_eq!(
    outcomes,
    [
      ("main".to_owned(), Err("token expired".to_owned())),
      ("spare".to_owned(), Err("HTTP 401".to_owned()))
    ]
  );
  assert_eq!(*asked.lock().unwrap(), [format!("Bearer {REFUSED}")], "an expired token is never sent");

  for account in ["main", "spare"] {
    let read = keeping.read(slot(account)).await.unwrap();

    assert_eq!(read.unread_at, Some(NOW), "{account}");
    assert_eq!(read.standing(NOW).count(), 0, "{account}'s full reading closes nothing once marked");
  }

  std::fs::write(
    &expired,
    json!({ "claudeAiOauth": { "accessToken": "sk-renewed", "expiresAt": NOW_MS + 3_600_000 } })
      .to_string(),
  )
  .unwrap();
  reader.pass(&keeping, NOW_MS + 1000).await;

  let main = keeping.read(slot("main")).await.unwrap();

  assert_eq!(main.unread_at, None, "a reading taken lifts the mark");
  assert_eq!(main.standing(NOW + 1).count(), 2);
  assert_eq!(keeping.unread_at("spare").await.unwrap(), Some(NOW), "a refused token stays marked");
}

/// An account-usage endpoint that takes each connection and drops it unanswered.
async fn breaking_endpoint() -> String {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();

  tokio::spawn(async move {
    while let Ok((stream, _)) = listener.accept().await {
      drop(stream);
    }
  });

  format!("http://127.0.0.1:{port}/usage")
}

#[tokio::test]
async fn an_endpoint_that_breaks_the_connection_is_a_failure_and_the_account_keeps_what_it_had() {
  let scratch = Scratch::new().await;
  let dir = tempfile::tempdir().unwrap();
  let token = dir.path().join("token");

  std::fs::write(&token, "sk-token-one").unwrap();

  let reader = Arc::new(ClaudeAccounts::new(
    breaking_endpoint().await,
    Arc::new(Fixed(vec![token_login("one", token)])),
  ));
  let keeping = Keeping::new(scratch.db().clone(), vec![reader.clone()]);
  let kept = full("five_hour");

  keeping.replace(slot("one"), std::slice::from_ref(&kept), NOW - 60).await.unwrap();

  let outcomes = reader.read_each(&keeping, NOW_MS).await.unwrap();

  assert_eq!(outcomes, [("one".to_owned(), Err("the request failed".to_owned()))]);

  let read = keeping.read(slot("one")).await.unwrap();

  assert_eq!(read.windows[0].window, kept);
  assert_eq!(read.windows[0].seen_at, NOW - 60);
  assert_eq!(read.unread_at, None, "an endpoint that broke the connection says nothing of the token");
}

#[test]
fn a_failure_is_said_once_with_its_reason_and_its_recovery_once() {
  let said = Said::default();
  let what = "claude usage for one";
  let refused = Err("HTTP 401".to_owned());

  assert_eq!(said.outcome(what, &Ok(())), None, "a reading taken says nothing");
  assert_eq!(
    said.outcome(what, &refused).as_deref(),
    Some("claude usage for one could not be read: HTTP 401")
  );
  assert_eq!(said.outcome(what, &refused), None, "the same failure is not said again");
  assert_eq!(
    said.outcome(what, &Err("the request failed".to_owned())).as_deref(),
    Some("claude usage for one could not be read: the request failed"),
    "another reason is said as it starts"
  );
  assert_eq!(said.outcome(what, &Ok(())).as_deref(), Some("claude usage for one read again"));
  assert_eq!(said.outcome(what, &Ok(())), None);
}

/// A stand-in for the run of the CLI on a login: each account it was asked for, in order; a run that waits
/// to be let go, then writes the credential it renews to, where it names one, and says it ended.
struct StandInCli {
  asked: Mutex<Vec<String>>,
  go: Arc<Notify>,
  renews: Option<std::path::PathBuf>,
  ended: mpsc::UnboundedSender<()>,
}

impl Renewals for StandInCli {
  fn renew(&self, account: &str) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'static>> {
    self.asked.lock().unwrap().push(account.to_owned());

    let go = self.go.clone();
    let renews = self.renews.clone();
    let ended = self.ended.clone();

    Box::pin(async move {
      go.notified().await;

      let outcome = match renews {
        Some(file) => {
          std::fs::write(&file, credential("sk-renewed", NOW_MS + 3_600_000)).unwrap();

          Ok(())
        }
        None => Err("the run's pane went away under it".to_owned()),
      };

      ended.send(()).unwrap();
      outcome
    })
  }
}

fn stand_in_cli(renews: Option<std::path::PathBuf>) -> (Arc<StandInCli>, mpsc::UnboundedReceiver<()>) {
  let (ended, heard) = mpsc::unbounded_channel();
  let cli = StandInCli {
    asked: Mutex::new(Vec::new()),
    go: Arc::new(Notify::new()),
    renews,
    ended,
  };

  (Arc::new(cli), heard)
}

fn credential(token: &str, expires_at: i64) -> String {
  json!({ "claudeAiOauth": { "accessToken": token, "expiresAt": expires_at } }).to_string()
}

fn directory_login(account: &str, file: std::path::PathBuf) -> Login {
  Login {
    account: account.into(),
    credential: Credential::Directory(file),
  }
}

// A directory account's login found expired, or refused, asks one run of the CLI on it, and a token account
// none; no other run is asked while it runs, and once it has renewed the login the next pass reads the
// account and lifts its mark (docs/usage-windows.md, 「The Claude account reader」).
#[tokio::test]
async fn a_login_found_expired_asks_one_run_of_the_cli_and_the_pass_after_it_reads_the_account() {
  let scratch = Scratch::new().await;
  let dir = tempfile::tempdir().unwrap();
  let expired = dir.path().join("main.json");
  let refused = dir.path().join("held.json");
  let token = dir.path().join("token");

  std::fs::write(&expired, credential("sk-expired", NOW_MS - 1)).unwrap();
  std::fs::write(&refused, credential(REFUSED, NOW_MS + 3_600_000)).unwrap();
  std::fs::write(&token, REFUSED).unwrap();

  let (url, asked) = usage_endpoint().await;
  let (cli, mut ended) = stand_in_cli(Some(expired.clone()));
  let logins = vec![
    directory_login("main", expired.clone()),
    directory_login("held", refused),
    token_login("spare", token),
  ];
  let reader = Arc::new(ClaudeAccounts::new(url, Arc::new(Fixed(logins))).renewing(cli.clone()));
  let keeping = Keeping::new(scratch.db().clone(), vec![reader.clone()]);

  reader.pass(&keeping, NOW_MS).await;

  assert_eq!(
    *cli.asked.lock().unwrap(),
    ["main", "held"],
    "a directory login expired or refused asks a run, a token account none"
  );

  reader.pass(&keeping, NOW_MS + 60_000).await;
  reader.pass(&keeping, NOW_MS + 600_000).await;

  assert_eq!(cli.asked.lock().unwrap().len(), 2, "no run is asked while one runs, however long");
  assert_eq!(asked.lock().unwrap().iter().filter(|said| said.contains("sk-expired")).count(), 0);

  cli.go.notify_one();
  ended.recv().await.unwrap();
  reader.pass(&keeping, NOW_MS + 660_000).await;

  let main = keeping.read(slot("main")).await.unwrap();

  assert_eq!(main.unread_at, None, "the reading after the run lifts the mark");
  assert_eq!(main.standing(NOW + 660).count(), 2);
  assert!(asked.lock().unwrap().contains(&"Bearer sk-renewed".to_owned()));
  assert_eq!(cli.asked.lock().unwrap().len(), 2, "a login read again asks no run");
}

// A run that ended without renewing the login is followed by another only once the pace after it was asked
// has passed, never in the pass after it ends.
#[tokio::test]
async fn a_login_still_expired_after_its_run_asks_another_only_after_the_pace() {
  let scratch = Scratch::new().await;
  let dir = tempfile::tempdir().unwrap();
  let expired = dir.path().join("main.json");

  std::fs::write(&expired, credential("sk-expired", NOW_MS - 1)).unwrap();

  let (url, _asked) = usage_endpoint().await;
  let (cli, mut ended) = stand_in_cli(None);
  let reader = Arc::new(
    ClaudeAccounts::new(url, Arc::new(Fixed(vec![directory_login("main", expired)]))).renewing(cli.clone()),
  );
  let keeping = Keeping::new(scratch.db().clone(), vec![reader.clone()]);

  reader.pass(&keeping, NOW_MS).await;
  cli.go.notify_one();
  ended.recv().await.unwrap();
  reader.pass(&keeping, NOW_MS + 60_000).await;

  assert_eq!(cli.asked.lock().unwrap().len(), 1, "the pass after a failed run waits out the pace");

  reader.pass(&keeping, NOW_MS + 300_000).await;

  assert_eq!(cli.asked.lock().unwrap().len(), 2, "asked again once the pace has passed");
}
