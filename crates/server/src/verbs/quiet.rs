//! `bmo-agency quiet`, the deployment's count of the work agency's restart would cut (docs/sessions.md, 「A
//! restart hands the session over」; docs/api.md, 「The command」): the running server's `GET /quiet` asked on
//! the port and under the token the environment names, else the data root's `env/agency.env`, and its answer
//! read as the testbed reads a quiet's code — 0 quiet, 1 work under way, 2 could not tell.

use std::path::Path;

use serde_json::Value;

/// The code a quiet ends with where the server holds no work its restart would cut.
pub const QUIET: u8 = 0;
/// The code where it holds some.
pub const UNDER_WAY: u8 = 1;
/// The code where it could not tell.
pub const UNTOLD: u8 = 2;

/// What the server answered the quiet's ask, or why it gave no answer.
#[derive(Debug)]
pub enum Asked {
  /// The status and the body it answered.
  Answered(u16, Option<Value>),
  /// No server listens on the port: nothing runs that a restart would cut.
  Unreached,
  /// The ask could not be made, or broke on the way.
  Failed(String),
}

/// The quiet's code and the lines it says, out of what the server answered.
pub fn judged(asked: Asked) -> (u8, Vec<String>) {
  match asked {
    Asked::Unreached => (QUIET, vec!["no agency server listens, so none carries a turn".to_owned()]),
    Asked::Failed(why) => (UNTOLD, vec![format!("agency's quiet could not be asked: {why}")]),
    // A server whose build answers no quiet route holds no count to wait on.
    Asked::Answered(404, _) => (QUIET, vec!["the agency server serving answers no quiet route".to_owned()]),
    Asked::Answered(200, Some(body)) => match body.get("work").and_then(Value::as_array) {
      Some(work) if work.is_empty() => (QUIET, vec![]),
      Some(work) => (UNDER_WAY, work.iter().map(said).collect()),
      None => (UNTOLD, vec!["agency's quiet answered no work list".to_owned()]),
    },
    Asked::Answered(status, body) => {
      let why = body
        .as_ref()
        .and_then(|body| body.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("no reason given")
        .to_owned();

      (UNTOLD, vec![format!("agency's quiet answered {status}: {why}")])
    }
  }
}

// One piece of work under way, said on a line.
fn said(work: &Value) -> String {
  let text = |field: &str| work.get(field).and_then(Value::as_str).unwrap_or("?").to_owned();

  format!(
    "agency carries turn {} of {} in {} since {}, which no program has taken yet",
    text("run"),
    text("agent"),
    text("session"),
    work.get("since").and_then(Value::as_i64).unwrap_or_default()
  )
}

/// The port and the token the quiet asks under: the environment's, else those the data root's
/// `env/agency.env` names.
pub fn reached(data_root: &Path) -> (u16, Option<String>) {
  let filed = |name: &str| super::deployment_setting(data_root, name);
  let port = super::control_port_in(data_root);
  let token = super::control_token().or_else(|| {
    filed(super::CONTROL_TOKEN).or_else(|| {
      filed(&crate::places::file_variable(super::CONTROL_TOKEN))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
    })
  });

  (port, token)
}

/// The running server's quiet, asked and judged.
pub async fn run(data_root: &Path) -> (u8, Vec<String>) {
  let (port, token) = reached(data_root);
  let Some(token) = token else {
    return judged(Asked::Failed(format!("{} is not set here or in env/agency.env", super::CONTROL_TOKEN)));
  };
  let asked = match reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}/quiet"))
    .bearer_auth(token)
    .send()
    .await
  {
    Ok(answer) => {
      let status = answer.status().as_u16();

      Asked::Answered(status, answer.json().await.ok())
    }
    Err(why) if why.is_connect() => Asked::Unreached,
    Err(why) => Asked::Failed(why.to_string()),
  };

  judged(asked)
}
