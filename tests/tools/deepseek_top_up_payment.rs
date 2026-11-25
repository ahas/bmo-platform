//! The DeepSeek top-up's payment (docs/tools.md, 「How it pays」 and 「Where a person is asked」), driven in the
//! machine's Firefox, on a throwaway profile planted with a session of its own, against a fake top-up platform on
//! the loopback: its sign-in page, a landing page, its top-up form with a custom amount, the card among the ways
//! to pay and a next step, and the card's fields in a frame served from another site, as a payment processor's
//! are. The platform pays, refuses, or waits for a confirmation
//! nobody gives, and a human check stops the sign-in; no step reaches DeepSeek, so no test spends anything.
//!
//! Where the fake holds the sign-in for a check of the account, a fake mail service on the loopback serves the
//! mailbox the tool reads it from: a code mailed once the check is asked to send one, or a link that approves the
//! device, beside an older code mailed before the sign-in began, which is never the one taken.
//!
//! A payment made moves the fake's balance, which the stand-in agency reads back as a balance reader that has just
//! read it; anything else leaves it where it stood. Every call ends its browser and the browser's screen with it,
//! gives the profile's session back as it was planted, and leaves no saved value in the profile.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::{Json, Path, Query, State};
use axum::http::header::{COOKIE, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use deepseek_top_up_tool::{MailService, Payer, top_up_with};
use serde_json::{Value, json};

use super::deepseek_top_up::{
  CVC, EXPIRY, HOLDER, NUMBER, PASSWORD, Stand, Throwaway, USERNAME, agency, amount,
  assert_profile_as_planted, cache_root, data_root, firefox_profiles, holds_no_secret_value, whole_card,
  whole_login,
};

/// What the fake platform does with a sign-in and a payment.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scenario {
  Pays,
  Refuses,
  /// The card's issuer asks for a confirmation, which never comes.
  Confirms,
  /// The sign-in is met by a human check.
  HumanCheck,
  /// The sign-in waits on a code it mails once asked to send one.
  MailsCode,
  /// The sign-in waits on a link it mails, which approves the device for the next sign-in.
  MailsLink,
  /// The sign-in waits on a code that no mail ever brings.
  MailsNothing,
}

/// The code the fake mails, and the older one mailed before the sign-in began.
const CODE: &str = "482913";
const OLD_CODE: &str = "111111";
/// The mail service's token and the owner key the fake mail service answers.
const MAIL_TOKEN: &str = "mail-token";
const MAIL_OWNER: &str = "mail-owner";

/// The fake platform's record: its balance, each sign-in and each payment asked for, with what it carried.
#[derive(Clone)]
struct Fake {
  scenario: Scenario,
  port: u16,
  balance: Arc<Mutex<f64>>,
  sign_ins: Arc<Mutex<Vec<Value>>>,
  payments: Arc<Mutex<Vec<Value>>>,
  /// The mail in the login's mailbox, the codes typed into the check, whether the device is approved, and every
  /// read of the mail service that carried the wrong token or owner key.
  mails: Arc<Mutex<Vec<Value>>>,
  codes: Arc<Mutex<Vec<String>>>,
  approved: Arc<Mutex<bool>>,
  mail_refused: Arc<Mutex<usize>>,
  /// What reached the platform on the way to the card's fields, in order: each press of the card among the ways
  /// to pay and of the next step, and each fetch of the card's frame. A call stopped before the card's fields
  /// names it, so a stop on any machine says which of them never came.
  steps: Arc<Mutex<Vec<String>>>,
}

const SIGN_IN: &str = r#"<!doctype html><html><body>
<input type="text" placeholder="Phone number / email address" id="who">
<input type="password" placeholder="Password" id="pass">
<div role="button" id="go" style="padding: 8px; width: 80px">Log in</div>
<div id="said"></div>
<div id="check" style="display: none">
  <div>Verify your account</div>
  <input type="text" placeholder="Enter verification code" id="code">
  <div role="button" id="send" style="padding: 8px; width: 80px">Send code</div>
  <div role="button" id="verify" style="padding: 8px; width: 120px">Verify and log in</div>
</div>
<script>
document.getElementById("go").addEventListener("click", async () => {
  const asked = await fetch("/api/sign_in", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ who: document.getElementById("who").value, pass: document.getElementById("pass").value }),
  });
  const answered = await asked.json();
  if (answered.result === "in") location.href = "/usage";
  else if (answered.result === "human") document.getElementById("said").innerText = "Let's confirm you are human";
  else if (answered.result === "check") document.getElementById("check").style.display = "block";
  else if (answered.result === "link")
    document.getElementById("said").innerText = "We sent a confirmation link to your email. Open it, then log in again.";
  else document.getElementById("said").innerText = "Your password is incorrect or this account doesn't exist.";
});
const post = (path, body) =>
  fetch(path, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
document.getElementById("send").addEventListener("click", () => post("/api/send_code", {}));
document.getElementById("verify").addEventListener("click", async () => {
  const answered = await (await post("/api/verify", { code: document.getElementById("code").value })).json();
  if (answered.result === "in") location.href = "/usage";
  else document.getElementById("said").innerText = "Incorrect verification code. Please try again.";
});
</script></body></html>"#;

const USAGE: &str = "<!doctype html><html><body><h1>Usage</h1></body></html>";

/// The top-up form, its ways to pay as the platform shows them: PayPal's guest checkout under the words of a card,
/// the platform's own card processor by the card networks' marks alone, and Google Pay; `{card}` is the card
/// frame's address on the other site.
const TOP_UP: &str = r#"<!doctype html><html><body>
<div>Amount</div>
<label id="ten"><input type="radio" name="amount" value="10"> $10</label>
<label id="custom"><input type="radio" name="amount"> Custom</label>
<input type="text" id="amount" placeholder="Enter amount" style="display: none">
<div>Payment method</div>
<label id="paypal"><input type="radio" name="way"> Debit or Credit Card</label>
<label id="card"><input type="radio" name="way"><svg role="img" aria-label="Visa" width="40" height="20"><rect width="40" height="20"/></svg><svg role="img" aria-label="Mastercard" width="40" height="20"><rect width="40" height="20"/></svg></label>
<label id="gpay"><input type="radio" name="way"><svg role="img" aria-label="Google Pay" width="40" height="20"><rect width="40" height="20"/></svg></label>
<div role="button" id="next" style="display: none; padding: 8px; width: 80px">Next step</div>
<div id="checkout" style="display: none">
  <iframe id="frame" width="420" height="260"></iframe>
  <div role="button" id="pay" style="padding: 8px; width: 80px">Pay</div>
</div>
<div id="said"></div>
<script>
const shown = (id) => { document.getElementById(id).style.display = "block"; };
document.getElementById("custom").addEventListener("click", () => shown("amount"));
const step = (what) => fetch(`/api/step/${what}`, { method: "POST" });
document.getElementById("card").addEventListener("click", () => {
  step("card");
  shown("next");
});
document.getElementById("paypal").addEventListener("click", () => step("paypal"));
document.getElementById("gpay").addEventListener("click", () => step("gpay"));
document.getElementById("next").addEventListener("click", () => {
  step("next");
  document.getElementById("frame").src = "{card}";
  shown("checkout");
});
document.getElementById("pay").addEventListener("click", () => {
  document.getElementById("frame").contentWindow.postMessage({ amount: document.getElementById("amount").value }, "*");
});
addEventListener("message", (event) => {
  document.getElementById("said").innerText =
    event.data.result === "paid" ? "Payment successful" : "Payment failed. Please try again.";
});
</script></body></html>"#;

const CARD_FRAME: &str = r#"<!doctype html><html><body>
<input autocomplete="cc-number" placeholder="Card number" id="number">
<input autocomplete="cc-exp" placeholder="MM / YY" id="expiry">
<input autocomplete="cc-csc" placeholder="CVV" id="cvc">
<input autocomplete="cc-name" placeholder="Cardholder Name (optional)" id="holder">
<div id="ask"></div>
<script>
const value = (id) => document.getElementById(id).value;
addEventListener("message", async (event) => {
  const asked = await fetch("/api/pay", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      amount: event.data.amount, number: value("number"), expiry: value("expiry"), cvc: value("cvc"), holder: value("holder"),
    }),
  });
  const answered = await asked.json();
  if (answered.result === "confirm") {
    document.getElementById("ask").innerText = "Approve this payment in your card app";
    return;
  }
  parent.postMessage({ result: answered.result }, "*");
});
</script></body></html>"#;

async fn sign_in_page() -> Html<&'static str> {
  Html(SIGN_IN)
}

async fn usage_page() -> Html<&'static str> {
  Html(USAGE)
}

async fn card_frame(State(fake): State<Fake>) -> Html<&'static str> {
  fake.steps.lock().unwrap().push("card frame fetched".to_owned());
  Html(CARD_FRAME)
}

async fn step(State(fake): State<Fake>, Path(what): Path<String>) -> StatusCode {
  fake.steps.lock().unwrap().push(format!("{what} pressed"));
  StatusCode::NO_CONTENT
}

async fn top_up_page(State(fake): State<Fake>, headers: HeaderMap) -> Response {
  let signed = headers
    .get_all(COOKIE)
    .iter()
    .any(|cookie| cookie.to_str().is_ok_and(|cookie| cookie.contains("signed=1")));

  if !signed {
    return (StatusCode::FOUND, [(LOCATION, "/sign_in")]).into_response();
  }

  Html(TOP_UP.replace("{card}", &format!("http://localhost:{}/card", fake.port))).into_response()
}

async fn sign_in(State(fake): State<Fake>, Json(asked): Json<Value>) -> Response {
  let right = asked["who"] == USERNAME && asked["pass"] == PASSWORD;

  fake.sign_ins.lock().unwrap().push(asked);

  if fake.scenario == Scenario::HumanCheck {
    return Json(json!({ "result": "human" })).into_response();
  }

  if !right {
    return Json(json!({ "result": "refused" })).into_response();
  }

  match fake.scenario {
    Scenario::MailsCode | Scenario::MailsNothing => Json(json!({ "result": "check" })).into_response(),
    Scenario::MailsLink if !*fake.approved.lock().unwrap() => {
      let link = format!("http://127.0.0.1:{}/approve?ticket=device-ticket", fake.port);

      fake.mail(&format!(
        r#"<html><head><style>p {{ margin: 0 }}</style></head><body>
        <a href="https://cdn.example.test/logo.png">DeepSeek</a>
        <p>A new device is signing in to your account.</p>
        <p><a href="{link}">Approve this sign-in</a></p></body></html>"#
      ));
      Json(json!({ "result": "link" })).into_response()
    }
    _ => signed_in(),
  }
}

fn signed_in() -> Response {
  ([(SET_COOKIE, "signed=1; Path=/; Max-Age=3600")], Json(json!({ "result": "in" }))).into_response()
}

impl Fake {
  /// A mail from DeepSeek's sender into the login's mailbox, reaching it now.
  fn mail(&self, html: &str) {
    self.mail_at(html, chrono::Utc::now().timestamp_millis());
  }

  fn mail_at(&self, html: &str, at: i64) {
    let mut mails = self.mails.lock().unwrap();
    let id = format!("INBOX/{}", mails.len() + 1);

    mails.push(json!({
      "id": id, "accountName": "tester", "at": at, "subject": "Your verification code",
      "from": { "name": "DeepSeek", "address": "noreply@mail.example.test" }, "html": html, "text": null,
    }));
  }
}

/// A code mail as DeepSeek words one: HTML alone, a style, the year and a lifetime beside the code.
fn code_mail(code: &str) -> String {
  format!(
    r#"<html><head><style>.code {{ font-size: 24px }}</style></head><body><p>Hello, {USERNAME}</p>
    <p>Please verify your account with the code below:</p><p class="code">{code}</p>
    <p>This code will expire in 10 minutes.</p><p>© 2026 DeepSeek. All Rights Reserved.</p></body></html>"#
  )
}

async fn send_code(State(fake): State<Fake>) -> Json<Value> {
  if fake.scenario == Scenario::MailsCode {
    fake.mail(&code_mail(CODE));
  }

  Json(json!({ "result": "sent" }))
}

async fn verify(State(fake): State<Fake>, Json(asked): Json<Value>) -> Response {
  let code = asked["code"].as_str().unwrap_or_default().to_owned();
  let right = code == CODE;

  fake.codes.lock().unwrap().push(code);

  if right { signed_in() } else { Json(json!({ "result": "wrong" })).into_response() }
}

async fn approve(State(fake): State<Fake>) -> Html<&'static str> {
  *fake.approved.lock().unwrap() = true;

  Html("<!doctype html><html><body>Sign-in approved.</body></html>")
}

/// Whether a read of the fake mail service carries its token and an owner key.
fn mail_read_allowed(fake: &Fake, headers: &HeaderMap) -> bool {
  let token = headers.get("authorization").and_then(|value| value.to_str().ok());
  let owner = headers.get("owner-key").and_then(|value| value.to_str().ok());
  let allowed = token == Some(&format!("Bearer {MAIL_TOKEN}")) && owner == Some(MAIL_OWNER);

  if !allowed {
    *fake.mail_refused.lock().unwrap() += 1;
  }

  allowed
}

async fn mail_accounts(State(fake): State<Fake>, headers: HeaderMap) -> Response {
  if !mail_read_allowed(&fake, &headers) {
    return StatusCode::UNAUTHORIZED.into_response();
  }

  let accounts = [("other", "other@example.test"), ("tester", USERNAME)]
    .map(|(name, address)| json!({ "name": name, "kind": "imap-smtp", "address": address }));

  Json(json!({ "items": accounts, "more": false })).into_response()
}

async fn mail_messages(
  State(fake): State<Fake>,
  headers: HeaderMap,
  Path(account): Path<String>,
  Query(query): Query<std::collections::HashMap<String, String>>,
) -> Response {
  if !mail_read_allowed(&fake, &headers) {
    return StatusCode::UNAUTHORIZED.into_response();
  }

  let after = query.get("after").and_then(|after| after.parse::<i64>().ok()).unwrap_or(0);
  let from = query.get("q").is_some_and(|asked| asked.starts_with("from:"));
  let items: Vec<Value> = fake
    .mails
    .lock()
    .unwrap()
    .iter()
    .rev()
    .filter(|mail| account == "tester" && from && mail["at"].as_i64().unwrap_or(0) > after)
    .map(|mail| json!({ "id": mail["id"], "at": mail["at"], "from": mail["from"], "html": null, "text": null }))
    .collect();

  Json(json!({ "items": items, "more": false })).into_response()
}

async fn mail_message(
  State(fake): State<Fake>,
  headers: HeaderMap,
  Path((account, id)): Path<(String, String)>,
) -> Response {
  if !mail_read_allowed(&fake, &headers) {
    return StatusCode::UNAUTHORIZED.into_response();
  }

  let found = fake
    .mails
    .lock()
    .unwrap()
    .iter()
    .find(|mail| account == "tester" && mail["id"] == id)
    .cloned();

  match found {
    Some(mail) => Json(mail).into_response(),
    None => StatusCode::NOT_FOUND.into_response(),
  }
}

async fn pay(State(fake): State<Fake>, Json(asked): Json<Value>) -> Json<Value> {
  let paid = asked["amount"].as_str().and_then(|amount| amount.parse::<f64>().ok()).unwrap_or(0.0);

  fake.payments.lock().unwrap().push(asked);

  let result = match fake.scenario {
    Scenario::Refuses => "refused",
    Scenario::Pays | Scenario::MailsCode | Scenario::MailsLink => {
      *fake.balance.lock().unwrap() += paid;
      "paid"
    }
    Scenario::Confirms | Scenario::HumanCheck | Scenario::MailsNothing => "confirm",
  };

  Json(json!({ "result": result }))
}

/// The fake platform on a loopback port, holding 3.5 USD; its server task ends with the test's runtime.
async fn platform(scenario: Scenario) -> Fake {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let fake = Fake {
    scenario,
    port: listener.local_addr().unwrap().port(),
    balance: Arc::new(Mutex::new(3.5)),
    sign_ins: Arc::default(),
    payments: Arc::default(),
    mails: Arc::default(),
    codes: Arc::default(),
    approved: Arc::default(),
    mail_refused: Arc::default(),
    steps: Arc::default(),
  };

  // A code mailed ten minutes before any sign-in of the test began.
  fake.mail_at(&code_mail(OLD_CODE), chrono::Utc::now().timestamp_millis() - 600_000);

  let app = Router::new()
    .route("/sign_in", get(sign_in_page))
    .route("/usage", get(usage_page))
    .route("/top_up", get(top_up_page))
    .route("/card", get(card_frame))
    .route("/api/sign_in", post(sign_in))
    .route("/api/pay", post(pay))
    .route("/api/step/{what}", post(step))
    .route("/api/send_code", post(send_code))
    .route("/api/verify", post(verify))
    .route("/approve", get(approve))
    .with_state(fake.clone());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  fake
}

/// The fake mail service on a loopback port of its own, reading the fake platform's mailbox; its server task ends
/// with the test's runtime.
async fn mail_service(fake: &Fake) -> MailService {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let app = Router::new()
    .route("/accounts", get(mail_accounts))
    .route("/accounts/{account}/messages", get(mail_messages))
    .route("/accounts/{account}/messages/{id}", get(mail_message))
    .with_state(fake.clone());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  MailService {
    address: format!("http://127.0.0.1:{port}"),
    token: MAIL_TOKEN.to_owned(),
    owner: MAIL_OWNER.to_owned(),
  }
}

/// The Firefox the fake platform is paid in, in the owner's Firefox's stead: its executable, the scope its
/// launcher starts in, and the folder a throwaway profiles folder is made in, which the executable can read.
struct Firefox {
  executable: String,
  scope: String,
  /// The folder the throwaway profiles folder is made in.
  near: std::path::PathBuf,
  /// The Firefox's own profiles root, whose profiles it mirrors into its cache root; none for Playwright's.
  mirrored: Option<std::path::PathBuf>,
}

/// The machine's Firefox: the owner's, as the tool's policy names it, where it stands, with a throwaway profiles
/// folder made beside his profiles root, where a snap's confined Firefox reads it and, standing outside that root,
/// keeps its caches inside the throwaway folder rather than mirroring it into the cache root; else the newest
/// Firefox Playwright keeps, as on a machine whose browser tests run on Playwright's, with its profiles in the
/// temporary folder. None where the machine holds neither, as a placed test machine holding no browser build does.
fn machine_firefox(root: &std::path::Path) -> Option<Firefox> {
  let place = Payer::of(root).platform.place;

  if std::path::Path::new(&place.firefox).is_file()
    && place.profiles.is_dir()
    && let Some(beside) = place.profiles.parent()
  {
    return Some(Firefox {
      executable: place.firefox.clone(),
      scope: place.scope.clone(),
      near: beside.to_owned(),
      mirrored: Some(place.profiles),
    });
  }

  let kept = std::env::var_os("PLAYWRIGHT_BROWSERS_PATH").map(std::path::PathBuf::from).or_else(|| {
    std::env::var_os("HOME").map(|home| std::path::Path::new(&home).join(".cache/ms-playwright"))
  })?;
  let executable = std::fs::read_dir(&kept)
    .into_iter()
    .flatten()
    .filter_map(Result::ok)
    .filter_map(|entry| {
      let revision = entry.file_name().to_str()?.strip_prefix("firefox-")?.parse::<u32>().ok()?;
      let binary = entry.path().join("firefox/firefox");

      binary.is_file().then_some((revision, binary))
    })
    .max()
    .map(|(_, binary)| binary.display().to_string())?;

  Some(Firefox {
    executable,
    scope: String::new(),
    near: std::env::temp_dir(),
    mirrored: None,
  })
}

/// A payer on the fake platform, paying in `firefox` from the profiles folder `profiles`, reading the fake mail
/// service, its waits short: `confirm` for a confirmation nobody gives, and two seconds for a mail.
async fn payer(
  fake: &Fake,
  root: &std::path::Path,
  profiles: &std::path::Path,
  confirm: Duration,
  firefox: &Firefox,
) -> Payer {
  let mut payer = Payer::of(root);

  payer.platform.mail.service = Some(mail_service(fake).await);
  payer.platform.mail.senders = vec!["example.test".to_owned(), "127.0.0.1".to_owned()];
  payer.platform.mail.wait = Duration::from_secs(2);
  payer.platform.mail.look = Duration::from_millis(100);

  payer.platform.place.firefox = firefox.executable.clone();
  payer.platform.place.scope = firefox.scope.clone();
  payer.platform.place.profiles = profiles.to_owned();

  payer.platform.address = format!("http://127.0.0.1:{}", fake.port);
  payer.platform.waits.step = Duration::from_secs(20);
  payer.platform.waits.confirm = confirm;
  payer.platform.waits.look = Duration::from_millis(100);
  payer.balance_wait = Duration::from_secs(5);
  payer.balance_look = Duration::from_millis(100);
  payer
}

/// Every process whose command line names the folder, read from `/proc`.
fn processes_naming(folder: &std::path::Path) -> Vec<String> {
  let named = folder.display().to_string();

  std::fs::read_dir("/proc")
    .unwrap()
    .filter_map(Result::ok)
    .filter_map(|entry| std::fs::read(entry.path().join("cmdline")).ok())
    .map(|line| String::from_utf8_lossy(&line).replace('\0', " "))
    .filter(|line| line.contains(&named))
    .collect()
}

/// Every file under `folder` holding one of the saved values whole, each named with the value's field.
fn files_holding_a_secret(folder: &std::path::Path) -> Vec<String> {
  let values = [
    ("CARD_NUMBER", NUMBER),
    ("CARD_HOLDER_NAME", HOLDER),
    ("USERNAME", USERNAME),
    ("PASSWORD", PASSWORD),
  ];
  let mut found = Vec::new();
  let mut folders = vec![folder.to_owned()];

  while let Some(folder) = folders.pop() {
    for entry in std::fs::read_dir(&folder).into_iter().flatten().filter_map(Result::ok) {
      let path = entry.path();

      if path.is_dir() {
        folders.push(path);
      } else if let Ok(bytes) = std::fs::read(&path) {
        for (field, value) in values {
          if bytes.windows(value.len()).any(|window| window == value.as_bytes()) {
            found.push(format!("{field} in {}", path.display()));
          }
        }
      }
    }
  }

  found
}

/// The machine's Firefox and a throwaway profiles folder for it; none, said on the error stream, where the machine
/// holds no Firefox, so the test that asked is passed over there, as every browser test of the platform is on
/// such a machine.
fn firefox_on_machine(root: &std::path::Path) -> Option<(Firefox, Throwaway)> {
  let Some(firefox) = machine_firefox(root) else {
    eprintln!(
      "passed over: no Firefox stands on this machine, neither the policy's nor a Playwright Firefox"
    );
    return None;
  };
  let profiles = firefox_profiles(&firefox.near, firefox.mirrored.as_deref());

  Some((firefox, profiles))
}

/// The folder Firefox would mirror the throwaway profiles folder `profiles` into, in the cache root of `firefox`'s
/// own profiles root; none for a Firefox that mirrors nothing.
fn mirror_of(firefox: &Firefox, profiles: &std::path::Path) -> Option<std::path::PathBuf> {
  Some(firefox.mirrored.as_deref().and_then(cache_root)?.join(profiles.file_name()?))
}

/// One call paid on the fake platform in the machine's Firefox and its answer, checked as `pay_in` says.
async fn pay_on(fake: &Fake, root: &std::path::Path, confirm: Duration) -> Option<String> {
  let (firefox, profiles) = firefox_on_machine(root)?;

  Some(pay_in(fake, root, profiles.path(), confirm, &firefox).await)
}

/// One call paid on the fake platform in `firefox` from the profiles folder `profiles`, and its answer, checked
/// for what no answer holds, for the processes it left, for the session and settings it gave back, and for any saved
/// value the profile kept.
async fn pay_in(
  fake: &Fake,
  root: &std::path::Path,
  profiles: &std::path::Path,
  confirm: Duration,
  firefox: &Firefox,
) -> String {
  let stand = Stand::reading(fake.balance.clone());
  let payer = payer(fake, root, profiles, confirm, firefox).await;

  let said = top_up_with(&agency(&stand).await, root, &payer, &amount(10.0))
    .await
    .unwrap_or_else(|refused| {
      panic!(
        "{refused}\nWhat reached the platform on the way to the card: {:?}",
        fake.steps.lock().unwrap()
      )
    })
    .plain();

  holds_no_secret_value(&said);
  assert!(
    !said.contains(CODE) && !said.contains("device-ticket"),
    "no mailed code or link stands in: {said}"
  );
  assert_eq!(
    *fake.mail_refused.lock().unwrap(),
    0,
    "every read of the mail service carried its token and owner"
  );
  assert!(processes_naming(profiles).is_empty(), "the browser and its screen ended with the call");
  assert_profile_as_planted(profiles);
  assert!(
    mirror_of(firefox, profiles).is_none_or(|mirror| !mirror.exists()),
    "the call's Firefox mirrored no folder into its cache root"
  );
  assert_eq!(
    files_holding_a_secret(profiles),
    Vec::<String>::new(),
    "the profile kept no saved value"
  );

  said
}

#[tokio::test]
async fn a_payment_the_platform_makes_answers_the_amount_and_the_balance_after_and_the_sign_in_stands() {
  let fake = platform(Scenario::Pays).await;
  let root = data_root(Some(&whole_card()), Some(&whole_login()));

  let Some((firefox, profiles)) = firefox_on_machine(root.path()) else {
    return;
  };
  let said = pay_in(&fake, root.path(), profiles.path(), Duration::from_secs(20), &firefox).await;

  assert!(
    said.starts_with("Paid: 10 USD. Balance: 13.5 USD, as the balance reader last read it at "),
    "{said}"
  );
  assert!(!said.contains("before it"), "the balance was read since the payment: {said}");
  assert_eq!(
    *fake.payments.lock().unwrap(),
    [json!({ "amount": "10", "number": NUMBER, "expiry": EXPIRY, "cvc": CVC, "holder": HOLDER })],
    "the card's fields were typed as saved, and pay was pressed once"
  );
  assert_eq!(
    *fake.sign_ins.lock().unwrap(),
    [json!({ "who": USERNAME, "pass": PASSWORD })],
    "the login was typed as saved"
  );

  let again = pay_in(&fake, root.path(), profiles.path(), Duration::from_secs(20), &firefox).await;

  assert!(again.starts_with("Paid: 10 USD. Balance: 23.5 USD"), "{again}");
  assert_eq!(fake.sign_ins.lock().unwrap().len(), 1, "the profile kept the sign-in for the next call");
}

#[tokio::test]
async fn a_refused_payment_moves_no_balance_and_is_not_tried_again() {
  let fake = platform(Scenario::Refuses).await;
  let root = data_root(Some(&whole_card()), Some(&whole_login()));

  let Some(said) = pay_on(&fake, root.path(), Duration::from_secs(20)).await else {
    return;
  };

  assert!(
    said.starts_with("Nothing was paid: DeepSeek or the card's issuer refused the payment"),
    "{said}"
  );
  assert!(said.contains("Paid: 0 USD. Balance: 3.5 USD"), "{said}");
  assert_eq!(fake.payments.lock().unwrap().len(), 1, "pay was pressed once");
  assert_eq!(*fake.balance.lock().unwrap(), 3.5);
}

#[tokio::test]
async fn a_confirmation_asked_of_a_person_stops_the_tool_there_and_moves_no_balance() {
  let fake = platform(Scenario::Confirms).await;
  let root = data_root(Some(&whole_card()), Some(&whole_login()));

  let Some(said) = pay_on(&fake, root.path(), Duration::from_secs(2)).await else {
    return;
  };

  assert!(
    said.starts_with("Nothing was paid that the platform confirmed within 2 seconds"),
    "{said}"
  );
  assert!(said.contains("a person approves it in the card's app"), "{said}");
  assert!(said.contains("Paid: 0 USD. Balance: 3.5 USD"), "{said}");
  assert_eq!(fake.payments.lock().unwrap().len(), 1, "pay was pressed once, and nothing after it");
  assert_eq!(*fake.balance.lock().unwrap(), 3.5);
}

#[tokio::test]
async fn a_human_check_at_the_sign_in_stops_before_any_card_is_typed() {
  let fake = platform(Scenario::HumanCheck).await;
  let root = data_root(Some(&whole_card()), Some(&whole_login()));

  let Some(said) = pay_on(&fake, root.path(), Duration::from_secs(20)).await else {
    return;
  };

  assert!(
    said.starts_with("Nothing was paid: DeepSeek asks a person to finish signing in"),
    "{said}"
  );
  assert!(
    said.contains(&format!(
      "The owner signs in to DeepSeek at http://127.0.0.1:{} in his own Firefox",
      fake.port
    )),
    "the answer says where the owner signs in: {said}"
  );
  assert!(said.contains("Paid: 0 USD. Balance: 3.5 USD"), "{said}");
  assert!(fake.payments.lock().unwrap().is_empty(), "no card was typed or paid");
}

#[tokio::test]
async fn a_sign_in_check_is_approved_with_the_code_mailed_after_the_sign_in_began_and_the_payment_goes_on()
{
  let fake = platform(Scenario::MailsCode).await;
  let root = data_root(Some(&whole_card()), Some(&whole_login()));

  let Some(said) = pay_on(&fake, root.path(), Duration::from_secs(20)).await else {
    return;
  };

  assert!(said.starts_with("Paid: 10 USD. Balance: 13.5 USD"), "{said}");
  assert_eq!(
    *fake.codes.lock().unwrap(),
    [CODE],
    "the code mailed after the sign-in began was typed, once, and the older one never"
  );
  assert_eq!(fake.payments.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_sign_in_check_is_approved_by_the_mailed_link_opened_in_the_same_profile_and_the_payment_goes_on()
{
  let fake = platform(Scenario::MailsLink).await;
  let root = data_root(Some(&whole_card()), Some(&whole_login()));

  let Some(said) = pay_on(&fake, root.path(), Duration::from_secs(20)).await else {
    return;
  };

  assert!(said.starts_with("Paid: 10 USD. Balance: 13.5 USD"), "{said}");
  assert!(*fake.approved.lock().unwrap(), "the approval link was opened");
  assert_eq!(
    fake.sign_ins.lock().unwrap().len(),
    2,
    "the sign-in was made again once the link approved it"
  );
  assert!(fake.codes.lock().unwrap().is_empty(), "no code was typed");
}

#[tokio::test]
async fn a_sign_in_check_no_mail_approves_in_time_stops_with_the_persons_step_and_pays_nothing() {
  let fake = platform(Scenario::MailsNothing).await;
  let root = data_root(Some(&whole_card()), Some(&whole_login()));

  let Some(said) = pay_on(&fake, root.path(), Duration::from_secs(20)).await else {
    return;
  };

  assert!(
    said.starts_with("Nothing was paid: DeepSeek asks a person to finish signing in"),
    "{said}"
  );
  assert!(said.contains("Paid: 0 USD. Balance: 3.5 USD"), "{said}");
  assert!(fake.codes.lock().unwrap().is_empty(), "the older code was never typed");
  assert!(fake.payments.lock().unwrap().is_empty(), "no card was typed or paid");
}
