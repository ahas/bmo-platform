//! What a failed Claude turn was (docs/run-records.md, 「Why a run died」). A job's
//! Claude turn always runs in the resident program, so a turn that did not answer leaves no stream: what
//! it left is the screen or the program's own error, read as the process's error output of a run that
//! exited with 1. That one text is judged here once — a full window, a login gone or refusing the run (an
//! endpoint's refused key or spent balance, and a 403 the firewall in front of the API answered, among
//! them), a short rest for too many requests at once, or the run having died —
//! and the judgement is carried by name, the name its cause is written under.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

/// What a failed run said about the login it went out on rather than about the work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountFailure {
  /// The login's window is full; another login answers and this one rests until its window resets.
  Limit,
  /// The login is gone or refuses the run: it rests, and waits for a person to sign in again.
  Login,
  /// The service itself would not answer, and no other login would either.
  Outage,
  /// The account's token for the work nodes did not answer on a node: its login here still answers,
  /// and a run on a node goes to the next login until a person makes the token again.
  #[serde(rename = "node-token")]
  NodeToken,
}

/// How a run that died ended, as a run's record names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RunEnding {
  Exit { code: i64 },
}

/// A failed Claude turn, by the error it is known as.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Failure {
  /// A limit reached, with the moment it resets where the run said one, and the name the answer gave the
  /// limit where it gave one ("Fable" of "You've reached your Fable limit"). Once the walk has read it,
  /// `account` is the login the run went out on, none where the run went out on none; `model` is the model
  /// that rests on that login where the limit was one model's rather than the login's own; and `resets_at`
  /// is the moment the rest it wrote ends, none where the login's next reading decides. `silenced` is the
  /// walk's word that the rest it wrote closed the last open login and nobody has told the owner yet,
  /// which leaves that line to whatever reads the failure.
  #[serde(rename_all = "camelCase")]
  Limit {
    resets_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    silenced: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    named: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
  },
  /// A failure of the login itself, read where the run was read.
  Account {
    failure: AccountFailure,
    detail: String,
  },
  /// The run ended with no answer, carrying what was readable where it ended.
  Died {
    ending: RunEnding,
    said: Option<String>,
    message: String,
  },
  /// The run was waiting on a person before it could take any work.
  Person,
  /// The program would not get past the folder-trust question in the worker's own folder.
  Untrusted { folder: String, config: String },
  /// The run was stopped from outside while it was working.
  Stopped,
  /// The pane or the program the turn stood in failed under it, rather than the road or the work.
  Transport { message: String },
  /// The road answered and refused the request the turn sent — one grown past its window among them —
  /// which is the work's failure and never the road's.
  Refused { message: String },
  /// The road the run was asked on is switched off.
  Unavailable {
    scope: String,
    mode: String,
    reason: String,
  },
  /// Anything else, by its error's name and message.
  Error { name: String, message: String },
  /// The error the last attempt of a walk raised, which the walk does not read as its road's: the attempt's
  /// runner keeps it and ends the job by it.
  Attempt,
  /// Every road the walk could take was tried or is shut, with the selections it tried.
  Exhausted { attempted: Vec<String> },
}

impl Failure {
  /// A plain error with the name a thrown `Error` carries.
  pub fn error(message: impl Into<String>) -> Self {
    Self::Error {
      name: "Error".to_owned(),
      message: message.into(),
    }
  }

  /// A limit as a run's answer states it: the moment it resets and the name it gives the limit, where it
  /// says either; no login and no model named yet.
  pub fn limit(resets_at: Option<i64>, named: Option<String>) -> Self {
    Self::Limit {
      resets_at,
      account: None,
      silenced: false,
      named,
      model: None,
    }
  }

  /// Which of the login's failures this is, where it is one.
  pub fn account_failure(&self) -> Option<AccountFailure> {
    match self {
      Self::Account { failure, .. } => Some(*failure),
      Self::Limit { .. } => Some(AccountFailure::Limit),
      _ => None,
    }
  }

  /// The words the failure is known by.
  pub fn message(&self) -> String {
    match self {
      Self::Limit {
        resets_at,
        account: Some(account),
        model,
        ..
      } => {
        let until = resets_at.map_or_else(
          || "until its next usage reading".to_owned(),
          |at| format!("until {}", crate::control::claude_accounts::moment_words(at)),
        );

        match model {
          Some(model) => format!("claude usage limit reached: {model} rests on account {account} {until}"),
          None => format!("claude usage limit reached: account {account} rests {until}"),
        }
      }
      Self::Limit { .. } => "claude usage limit reached".to_owned(),
      Self::Account { detail, .. } => detail.clone(),
      Self::Died { message, .. } => message.clone(),
      Self::Person => "the run is waiting on a person before it can take any work".to_owned(),
      Self::Untrusted { folder, config } => format!(
        "Claude Code did not get past the folder-trust question: the folder {folder} was written as trusted in the settings {config} and the program stood up again, and the question still stood"
      ),
      Self::Stopped => "the run was stopped from outside while it was working".to_owned(),
      Self::Transport { message } | Self::Refused { message } => message.clone(),
      Self::Unavailable {
        scope,
        mode,
        reason,
      } if reason.is_empty() => format!("{scope}: selected {mode} route did not answer"),
      Self::Unavailable {
        scope,
        mode,
        reason,
      } => format!("{scope}: selected {mode} route did not answer: {reason}"),
      Self::Error { message, .. } => message.clone(),
      Self::Attempt => "the attempt's own error".to_owned(),
      Self::Exhausted { .. } => {
        "Every work provider that could take the run is blocked, so the run stopped.".to_owned()
      }
    }
  }

  /// The name the failure's error carries: its own, where the failure has a name of its own, and the plain
  /// error's otherwise.
  pub fn name(&self) -> &str {
    match self {
      Self::Person => "JobWaitingOnPerson",
      Self::Untrusted { .. } => "JobFolderUntrusted",
      Self::Stopped => "JobStopped",
      Self::Transport { .. } => "PaneTransportError",
      Self::Refused { .. } => "RequestRefusedError",
      Self::Unavailable { .. } => "SelectedRouteUnavailableError",
      Self::Error { name, .. } => name,
      _ => "Error",
    }
  }

  /// The failure as a cause is written of it: its name, then its words.
  pub fn cause(&self) -> String {
    format!("{}: {}", self.name(), self.message())
  }
}

// What the CLI says when the login is gone. Its own wording for it is the shape of the message, read
// beside the run's ending and the credential's own expiry.
static LOGIN_GONE: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(
    r"(?i)(?:please )?(?:run )?/?login\b|not (?:logged in|authenticated)|invalid api key|oauth token (?:has )?expired|oauth (?:access )?token (?:has been )?revoked|invalid bearer token",
  )
  .expect("the login wording is a pattern")
});

static LIMIT_WORDINGS: LazyLock<[Regex; 3]> = LazyLock::new(|| {
  [
    Regex::new(r"(?i)claude (?:ai )?usage limit reached").expect("a limit wording"),
    Regex::new(r"(?i)usage limit reached\|\d+").expect("a limit wording"),
    Regex::new(r"(?i)you'?ve (?:hit|reached) your [^\n]{0,40}limit").expect("a limit wording"),
  ]
});

// The name the CLI's answer gives the limit it reached, where it gives one: "Fable" of "You've reached your
// Fable limit", "session" of "You've hit your session limit".
static LIMIT_NAMED: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?i)you'?ve (?:hit|reached) your ([^\n]{0,40}?)\s*limit")
    .expect("the limit's name is a pattern")
});

static LIMIT_RESET: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"(?i)limit reached\|(\d{9,11})").expect("the reset wording is a pattern"));

// The status a refused request names, as the CLI writes it: `API Error: 401 <the endpoint's message>` for
// a key or a balance the endpoint refused, `API Error: 403 <its message>` for a request the account may not
// make, the firewall in front of the API refusing it among them, and `API Error: Request rejected (429) ·
// <its message>` for too many requests at once.
static API_STATUS: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"API Error[^\n]{0,80}?\b(\d{3})\b").expect("an API status"));

// The endpoint's own words for a refusal, where its body reached the run without a status in front: a key
// it will not take, and a balance spent.
static KEY_REFUSED: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#""type"\s*:\s*"authentication_error""#).expect("a key refusal"));
static BALANCE_SPENT: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"(?i)"message"\s*:\s*"insufficient balance""#).expect("a balance refusal"));

// The connection to the provider, or to the model gateway in front of it, refused: nothing answered the
// request at all, as while the gateway restarts.
static CONNECTION_REFUSED: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"(?i)connection refused|ECONNREFUSED").expect("a refused connection"));

/// Whether a turn that ended on an API error was refused for the request it sent rather than for its road:
/// the error's kind naming the request (`invalid_request`, `max_output_tokens`), or a 400, 413 or 422 status.
/// That failure is the work's own and moves no road (docs/provider-order.md, 「A road that fails」).
pub fn request_refused(error: &str, kind: Option<&str>) -> bool {
  matches!(kind, Some("invalid_request" | "max_output_tokens"))
    || API_STATUS
      .captures(error)
      .and_then(|found| found[1].parse::<i64>().ok())
      .is_some_and(|status| matches!(status, 400 | 413 | 422))
}

/// Whether a run's words say its login's window is full.
pub fn mentions_limit(text: &str) -> bool {
  LIMIT_WORDINGS.iter().any(|pattern| pattern.is_match(text))
}

/// The moment a full window resets, where the run's words carry it.
pub fn limit_reset_from_text(text: &str) -> Option<i64> {
  LIMIT_RESET
    .captures(text)
    .and_then(|found| found.get(1))
    .and_then(|epoch| epoch.as_str().parse().ok())
}

/// The name a run's words give the limit they say was reached, where they give one.
pub fn limit_named(text: &str) -> Option<String> {
  LIMIT_NAMED
    .captures(text)
    .and_then(|found| found.get(1))
    .map(|named| named.as_str().trim().to_owned())
    .filter(|named| !named.is_empty())
}

/// Whether a limit's name is the model's own: the CLI names a model's limit by its family word, which is
/// one of the words of the model's id ("Fable" of `claude-fable-5-2`). A word that holds no letter, a
/// version number, names no model.
pub fn names_model(named: &str, model: &str) -> bool {
  let parts: Vec<String> = model.split(['-', '_', '.']).map(str::to_lowercase).collect();

  named
    .split_whitespace()
    .map(str::to_lowercase)
    .filter(|word| word.chars().any(char::is_alphabetic))
    .any(|word| parts.contains(&word))
}

/// How much of a failed run's output is kept, everywhere it is kept.
pub const FAILURE_TEXT_LIMIT: usize = 2000;

static CSI_ESCAPE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"\x1b\[[0-9;:?]*[ -/]*[@-~]").expect("an escape pattern"));
static OS_ESCAPE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)").expect("an escape pattern"));
static LONE_ESCAPE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"\x1b[@-Z\\-_]").expect("an escape pattern"));

// A line carrying no letter and no digit is a drawing, a rule or a row of dots, never a cause.
fn says_something(line: &str) -> bool {
  line.chars().any(char::is_alphanumeric)
}

fn stripped(raw: &str) -> String {
  let plain = CSI_ESCAPE.replace_all(raw, "");
  let plain = OS_ESCAPE.replace_all(&plain, "");
  let plain = LONE_ESCAPE.replace_all(&plain, "");

  plain.replace("\r\n", "\n").replace('\r', "\n")
}

// The last `limit` characters as JavaScript counts them, in UTF-16 units; the cut lands mid-line, so
// that half line is dropped unless dropping it would leave nothing.
fn tail_of(text: &str, limit: usize) -> String {
  let units: Vec<u16> = text.encode_utf16().collect();

  if units.len() <= limit {
    return text.to_owned();
  }

  let tail = String::from_utf16_lossy(&units[units.len() - limit..]);
  let line_break = tail.find('\n');

  match line_break {
    Some(at) if at != tail.len() - 1 => format!("…\n{}", &tail[at + 1..]),
    _ => format!("…{tail}"),
  }
}

/// What a failed run printed, shortened for a record: escape sequences gone, decoration lines gone,
/// blank runs closed up, and the last characters kept with a `…` where the head was dropped.
pub fn failure_text(raw: &str) -> String {
  let plain = stripped(raw);
  let said = plain
    .split('\n')
    .map(str::trim_end)
    .filter(|line| says_something(line))
    .collect::<Vec<_>>()
    .join("\n");
  let said = said.trim();

  tail_of(if said.is_empty() { plain.trim() } else { said }, FAILURE_TEXT_LIMIT)
}

/// Whether a run that exited says, in what its harness reads of it, that its login is gone.
pub fn login_gone(stderr: &str, code: i64) -> bool {
  LOGIN_GONE.is_match(&harness_failure(stderr, code))
}

// The harness's own reading of a process that exited: its last line said, cut short.
fn harness_failure(stderr: &str, code: i64) -> String {
  let tail = stderr.split('\n').map(str::trim).rfind(|line| !line.is_empty());

  match tail {
    None => format!("exit {code}"),
    Some(line) => format!("exit {code}: {}", line.chars().take(300).collect::<String>()),
  }
}

/// What a turn that did not answer comes to, read off the text it left as a run's error output on exit
/// 1: the endpoint refusing its key, its balance, its request or too many requests at once; a login gone, by what the
/// harness reads of it; a full window; or the run having died.
pub fn of_unanswered(evidence: &str, key: &str) -> Failure {
  let code = 1;

  if let Some(refused) = status_refusal(evidence, key) {
    return refused;
  }

  if LOGIN_GONE.is_match(&harness_failure(evidence, code)) {
    return Failure::Account {
      failure: AccountFailure::Login,
      detail: format!("{key}: {}", failure_text(evidence)),
    };
  }

  let error_text = format!("{evidence}\n");

  if mentions_limit(&error_text) {
    return Failure::limit(limit_reset_from_text(&error_text), limit_named(&error_text));
  }

  let said = failure_text(evidence);

  Failure::Died {
    ending: RunEnding::Exit { code },
    message: format!("claude exited with code {code}: {said}"),
    said: Some(said),
  }
}

// What the endpoint's refusal comes to for the account the run went out on: a key or a balance it will
// not take, or a request it forbids the account, waits for a person, since nobody else can settle any of
// them, and too many requests at once rests it for a short while. A status the endpoint names decides;
// otherwise its own words for a refused key or a spent balance do.
fn status_refusal(evidence: &str, key: &str) -> Option<Failure> {
  let status = API_STATUS.captures(evidence).and_then(|found| found[1].parse::<i64>().ok());

  let gone = match status {
    Some(401..=403) => true,
    None => KEY_REFUSED.is_match(evidence) || BALANCE_SPENT.is_match(evidence),
    Some(_) => false,
  };

  if gone {
    return Some(Failure::Account {
      failure: AccountFailure::Login,
      detail: format!("{key}: {}", failure_text(evidence)),
    });
  }

  (status == Some(429)).then(|| Failure::limit(None, None))
}

/// What a turn whose program wrote its own error in the answer's place comes to: the endpoint refusing its
/// key, its balance or its request, or too many of its requests at once; a full window, with the reset it names; the
/// service answering with a server error or refusing the connection, which no other login would get past
/// either; or, read as a turn
/// that did not answer, a login gone or the run having died.
pub fn of_errored(error: &str, key: &str) -> Failure {
  if let Some(refused) = status_refusal(error, key) {
    return refused;
  }

  if mentions_limit(error) {
    return Failure::limit(limit_reset_from_text(error), limit_named(error));
  }

  if API_STATUS
    .captures(error)
    .and_then(|found| found[1].parse::<i64>().ok())
    .is_some_and(|status| status >= 500)
  {
    return Failure::Account {
      failure: AccountFailure::Outage,
      detail: failure_text(error),
    };
  }

  if CONNECTION_REFUSED.is_match(error) {
    return Failure::Account {
      failure: AccountFailure::Outage,
      detail: failure_text(error),
    };
  }

  of_unanswered(error, key)
}
