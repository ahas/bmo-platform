#![allow(
  dead_code,
  unused_imports,
  unused_variables,
  clippy::needless_lifetimes,
  clippy::let_and_return,
  clippy::unnecessary_to_owned
)]
//! The Claude logins as a job's runner, the job unit and the console ask after them (docs/api.md,
//! 「Accounts」): the rest a done job's session usage calls for, whether a user holds a login that is on with
//! each login's own two headline windows, and where the logins keep their session records.

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use harness_providers::claude_code::ClaudeCode;
use serde::Deserialize;
use serde_json::{Value, json};
use store::one;

use super::claude_accounts::{Judging, accounts, login_homes};
use super::fallback::{account_balance, account_headlines};
use super::{Call, Refused, Route, answer};
use crate::door::record::SessionUsage;
use crate::now_ms;
use crate::rest::{RestKind, rest_until_after, tell_silence, write_rest};

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

/// A done job's session usage, as its answer reported it on the login.
#[derive(Deserialize)]
struct Used {
  usage: SessionUsage,
}

/// The rest a done job's session usage calls for, begun on the login it was read of.
pub struct UsageRest;

impl Route for UsageRest {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/accounts/{accountId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let account = call.param("accountId").unwrap_or_default().to_owned();
    let body = call.body().await?;
    let usage = body
      .get("usage")
      .cloned()
      .or_else(|| body.get("window").cloned())
      .unwrap_or(body.clone());
    let used: Used = serde_json::from_value(json!({ "usage": usage }))
      .map_err(|error| bad(format!("a session's usage: {error}")))?;
    let now = now_ms() / 1000;

    if account.is_empty() {
      return Err(bad("an account is required"));
    }

    let mut rest = None;

    if let Some(until) = rest_until_after(&used.usage, now) {
      let ground = call.ground();

      if write_rest(Judging::of(ground), until, RestKind::Limit, &account, now).await? {
        tell_silence(ground, RestKind::Limit, Some(&account), until as i64).await?;
      }
      rest = Some(until as i64);
    }

    Ok(answer(
      StatusCode::OK,
      json!({
        "id": account,
        "provider": "claude-code",
        "userName": Value::Null,
        "held": true,
        "home": Value::Null,
        "window": body.get("window").cloned().unwrap_or(Value::Null),
        "restUntil": rest,
      }),
    ))
  }
}

crate::moved!(UsageRest);

/// Whether a user holds a login that is on. A person's call reads their own accounts alone; the platform's
/// reads the platform's accounts and of each person's supply the roads it serves, save where it asks after
/// one user's logins, as the job unit does, which it is answered over every account (docs/api.md, 「Who a
/// caller acts for」).
pub struct AccountHeld;

impl Route for AccountHeld {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/accounts";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let acting = crate::access::Acting::of(&call)?;
    let machine = call.machine();
    let user = call.query("userName").or_else(|| call.query("user"));
    let mut items = Vec::new();
    let wanted = match user.as_deref() {
      Some(user) => match user.trim().parse::<i64>() {
        Ok(user) => Some(user),
        Err(_) => return Ok(answer(StatusCode::OK, json!({ "items": [], "more": false }))),
      },
      None => None,
    };

    let judging = Judging::of(call.ground());
    let administrator = super::claude_accounts::administrator(call.store()).await?;
    let shown = |account: &super::claude_accounts::Account| match acting {
      crate::access::Acting::Person(user) => account.owner_account == Some(user),
      crate::access::Acting::Platform => {
        user.is_some() || super::claude_accounts::platform_owned(account, administrator)
      }
    };

    for account in accounts(call.store()).await?.into_iter().filter(|one| shown(one)) {
      let (session, week) = account_headlines(judging.windows, &account).await?;
      let balance = account_balance(judging.windows, &account).await?;
      let held = match wanted {
        Some(user) => account.owner_account == Some(user) && account.enabled,
        None => account.enabled,
      };
      let (endpoint, _, models) = account.credential.endpoint_parts();

      items.push(json!({
        "id": account.id,
        "provider": "claude-code",
        "userName": account.owner_account,
        "held": held,
        "home": account.records_dir(machine).to_string_lossy(),
        "window": Value::Null,
        "session": session,
        "week": week,
        "balance": balance,
        "restUntil": Value::Null,
        "credentialKind": account.credential.kind().word(),
        "endpointUrl": endpoint,
        "keyFile": account.credential.key_file().map(|file| file.to_string_lossy()),
        "models": models,
        "basePrompt": account.base_prompt,
        "userId": account.owner_account,
        "node": account.node,
        "lendable": account.lendable,
      }));
    }

    let mut page = json!({ "items": items, "more": false });

    if acting == crate::access::Acting::Platform {
      page["personalSupply"] = json!(super::personal_supply::people(call.store()).await?);
    }

    Ok(answer(StatusCode::OK, page))
  }
}

crate::moved!(AccountHeld);

/// The directory each login keeps its session records under: this machine's own first, each once.
pub struct RecordHomes;

impl Route for RecordHomes {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/accounts/claude/homes";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let machine = call.machine();
    let logins = login_homes(call.store(), machine).await?;
    let homes: Vec<String> = ClaudeCode::config_dirs(machine, &logins)
      .iter()
      .map(|home| home.to_string_lossy().into_owned())
      .collect();

    Ok(answer(StatusCode::OK, json!({ "homes": homes })))
  }
}
