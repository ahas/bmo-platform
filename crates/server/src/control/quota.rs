//! The cache siren's quota source (docs/api.md, 「Accounts」): `GET /api/claude` on a loopback port of its
//! own. The siren, a monitor outside agency, asks a fixed address on this machine with no token of its own,
//! so this is a listener apart from the control port: it answers one path — which login the deployment's
//! chat runs on, and each login's own windows as agency keeps them — and can do nothing else.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::response::Response;
use std::sync::Arc;

use serde_json::{Map, Value, json};
use tokio::net::TcpListener;

use super::claude_accounts::{Judging, accounts};
use super::fallback::{active_agent_usage, headline_windows};
use super::harness::named;
use super::{Refused, answer};
use crate::places::Ground;

/// The one path the source answers.
pub const QUOTA_PATH: &str = "/api/claude";

// A share of a window as the siren reads it: a percentage to one decimal, or nothing where the slot holds
// no such window.
fn percent(window: &Value) -> Value {
  window["utilization"]
    .as_f64()
    .map_or(Value::Null, |share| json!((share * 1000.0).round() / 10.0))
}

// One slot as the siren reads it.
fn slot(provider: &str, account: &Value, tier: &str, session: &Value, week: &Value, active: bool) -> Value {
  json!({
    "five_hour_utilization": percent(session),
    "five_hour_resets_at": session["resetsAt"],
    "seven_day_utilization": percent(week),
    "seven_day_resets_at": week["resetsAt"],
    "tier": tier,
    "account": account,
    "provider": provider,
    "active": active,
  })
}

/// What the source answers: the active agent's slot first and marked, so a siren that reads only the first
/// slot still shows the login the runs go out on, then the provider's other logins in the order it holds
/// them.
pub async fn quota_source(judging: Judging<'_>) -> Result<Value, Refused> {
  let db = judging.db;
  let active = active_agent_usage(judging).await?;
  let provider = active["provider"].as_str().unwrap_or_default().to_owned();
  let tier = active["accountName"].as_str().unwrap_or_default();
  let mut slots =
    vec![slot(&provider, &active["account"], tier, &active["session"], &active["week"], true)];

  if named(&provider)?.holds_logins() {
    for account in accounts(db).await? {
      if !account.enabled || active["account"].as_str() == Some(account.id.as_str()) {
        continue;
      }

      let (session, week) =
        headline_windows(judging.windows, &provider, Some(&account.id), account.needs_login_at).await?;

      slots.push(slot(&provider, &json!(account.id), &account.name, &session, &week, false));
    }
  }

  let numbered: Map<String, Value> = slots
    .into_iter()
    .enumerate()
    .map(|(place, one)| ((place + 1).to_string(), one))
    .collect();

  Ok(json!({
    "usage": {
      "active": {
        "provider": provider,
        "account": active["account"],
        "name": active["accountName"],
        "accounts": active["accounts"],
      },
      "real_5h_pct": percent(&active["session"]),
      "real_week_pct": percent(&active["week"]),
      "quota_slots": numbered,
    }
  }))
}

async fn asked(State(ground): State<Arc<Ground>>, request: Request) -> Response {
  if request.method() != Method::GET || request.uri().path() != QUOTA_PATH {
    return answer(StatusCode::NOT_FOUND, json!({ "error": "not found" }));
  }

  match quota_source(Judging::of(&ground)).await {
    Ok(source) => answer(StatusCode::OK, source),
    Err(refused) => axum::response::IntoResponse::into_response(refused),
  }
}

/// The source's router: one path, everything else not found.
pub fn router(ground: Arc<Ground>) -> Router {
  Router::new().fallback(asked).with_state(ground)
}

/// The source on its port of the loopback, for as long as the process runs. A port another process still
/// holds is asked for again every half second, as the control port is: the process before keeps it until it
/// stops.
pub async fn stand(port: u16, ground: Arc<Ground>) -> io::Result<()> {
  let listener = loop {
    match TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await {
      Ok(listener) => break listener,
      Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
      }
      Err(error) => return Err(error),
    }
  };

  println!("agency: quota source on http://{}{QUOTA_PATH} (localhost only)", listener.local_addr()?);
  axum::serve(listener, router(ground)).await
}
