//! A person's own local AI (docs/supply.md, 「A person's local AI」; docs/own-llm.md, 「A person's local AI」): a
//! registration the person owns whose one address stands on their PC and is asked through its link at a port of
//! this machine's loopback; whether their chain lets them hold one; the ports the links listen on; and the test of
//! one, a single request sent from the PC (docs/person-nodes.md, 「A local AI on a PC」).

use std::path::Path;
use std::time::Duration;

use bmo_pocket_client::Runner;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use policy::organisation::ORGANISATION;
use policy::supply::SUPPLY;
use sea_orm::ConnectionTrait;
use serde_json::{Value, json};
use store::{all, one};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::Refusal;
use crate::chain::{self, Subject};
use crate::control::supply_test::Outcome;
use crate::places::Ground;

/// The registration's name of a person's local AI: their account's mark and the name they gave.
pub fn registration_name(account: i64, name: &str) -> String {
  format!("u{account}-{name}")
}

/// The name a person gave a local AI, read off its registration's name, none where it is not theirs.
pub fn given_name(account: i64, registration: &str) -> Option<&str> {
  registration.strip_prefix(&format!("u{account}-"))
}

/// The file of a person's secret under the data root's `env/`, as their local AI's address names it.
pub fn key_file(account: i64, secret: &str) -> String {
  format!("users/{account}/{secret}")
}

/// The secret's name a key file of the person's names, none for any other file.
pub fn secret_of(account: i64, key_file: &str) -> Option<String> {
  key_file.strip_prefix(&format!("users/{account}/")).map(str::to_owned)
}

/// One local AI of a person's, as its registration and its one address hold it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalAi {
  /// The name the person gave it.
  pub name: String,
  /// Its registration's name.
  pub registration: String,
  pub url: String,
  pub model: String,
  pub key_file: Option<String>,
  pub context_tokens: i64,
  pub enabled: bool,
  pub added_at: i64,
  pub reach_port: i64,
}

impl LocalAi {
  /// As the person's routes answer it.
  pub fn view(&self, account: i64) -> Value {
    json!({
      "name": self.name,
      "model": format!("{}::{}", crate::control::registry::PROVIDER, self.registration),
      "url": self.url,
      "modelName": self.model,
      "keyName": self.key_file.as_deref().and_then(|file| secret_of(account, file)),
      "contextTokens": self.context_tokens,
      "enabled": self.enabled,
      "addedAt": self.added_at,
    })
  }
}

/// Every local AI of a person's: each registration they own whose address is reached through their PC.
pub async fn of_person<C: ConnectionTrait>(db: &C, account: i64) -> Result<Vec<LocalAi>, Refusal> {
  let rows = all(
    db,
    r#"SELECT m."name", m."model", m."context_tokens", m."enabled", m."added_at",
         a."url", a."key_file", a."reach_port"
       FROM "registered_models" m JOIN "registered_model_addresses" a ON a."registration" = m."name"
       WHERE m."account" = $1 AND a."reach_port" IS NOT NULL
       ORDER BY m."added_at", m."name""#,
    vec![account.into()],
  )
  .await?;
  let mut held = Vec::with_capacity(rows.len());

  for row in rows {
    let registration: String = row.try_get("", "name")?;
    let Some(name) = given_name(account, &registration).map(str::to_owned) else {
      continue;
    };

    held.push(LocalAi {
      name,
      url: row.try_get("", "url")?,
      model: row.try_get("", "model")?,
      key_file: row.try_get("", "key_file")?,
      context_tokens: row.try_get("", "context_tokens")?,
      enabled: row.try_get::<i64>("", "enabled")? == 1,
      added_at: row.try_get("", "added_at")?,
      reach_port: row.try_get("", "reach_port")?,
      registration,
    });
  }

  Ok(held)
}

/// One local AI of a person's by the name they gave it.
pub async fn named<C: ConnectionTrait>(
  db: &C,
  account: i64,
  name: &str,
) -> Result<Option<LocalAi>, Refusal> {
  Ok(of_person(db, account).await?.into_iter().find(|one| one.name == name))
}

/// Whether a registration is a person's local AI: an address of it is reached through its owner's PC.
pub async fn is_local<C: ConnectionTrait>(db: &C, registration: &str) -> Result<bool, Refusal> {
  Ok(
    one(
      db,
      r#"SELECT 1 AS "reached" FROM "registered_model_addresses" WHERE "registration" = $1 AND "reach_port" IS NOT NULL"#,
      vec![registration.into()],
    )
    .await?
    .is_some(),
  )
}

/// The host and port an address names, as the PC connects to it: its own port, else plain http's.
pub fn reached_at(url: &str) -> Option<String> {
  let parsed = url::Url::parse(url).ok()?;
  let host = parsed.host_str()?;
  let port = parsed.port_or_known_default()?;

  Some(if host.contains(':') && !host.starts_with('[') {
    format!("[{host}]:{port}")
  } else {
    format!("{host}:{port}")
  })
}

/// The local AIs that are on of an account's, as its PC's link carries them (docs/person-nodes.md, 「A local AI on a
/// PC」).
pub async fn reaches<C: ConnectionTrait>(
  db: &C,
  account: i64,
) -> Result<Vec<node_link::spec::Reach>, Refusal> {
  Ok(
    of_person(db, account)
      .await?
      .into_iter()
      .filter(|one| one.enabled)
      .filter_map(|one| {
        Some(node_link::spec::Reach {
          port: u16::try_from(one.reach_port).ok()?,
          to: reached_at(&one.url)?,
        })
      })
      .collect(),
  )
}

/// The lowest port of the policy's range no address is asked at yet, none where every one is taken.
pub async fn free_port<C: ConnectionTrait>(db: &C) -> Result<Option<i64>, Refusal> {
  let taken: std::collections::BTreeSet<i64> = all(
    db,
    r#"SELECT "reach_port" FROM "registered_model_addresses" WHERE "reach_port" IS NOT NULL"#,
    vec![],
  )
  .await?
  .iter()
  .map(|row| row.try_get::<i64>("", "reach_port"))
  .collect::<Result<_, _>>()?;
  let range = i64::from(SUPPLY.local_ai.first_port)..=i64::from(SUPPLY.local_ai.last_port);

  Ok(range.into_iter().find(|port| !taken.contains(port)))
}

/// Whether the chain lets the person hold a local AI of their own; the administrator's account is held to none of
/// it. An account service that does not answer is the error.
pub async fn allowed(ground: &Ground, account: i64) -> Result<bool, String> {
  if crate::account_isolation::owners_own(&ground.db, &account.to_string())
    .await
    .map_err(|refusal| refusal.detail)?
  {
    return Ok(true);
  }

  let service = crate::apps::service(ground);
  let answer =
    chain::resolve(&service, &ORGANISATION.personal_local_ai.key, Subject::Person(account)).await?;

  Ok(chain::flag(answer.as_ref(), true))
}

/// The runner socket of the person's own PC, where they hold one the link runs their personal tools on; none
/// otherwise.
pub async fn pc_of(ground: &Ground, account: i64) -> Result<Option<std::path::PathBuf>, Refusal> {
  if crate::account_isolation::owners_own(&ground.db, &account.to_string()).await? {
    return Ok(None);
  }

  Ok(match crate::lines::token::runner_of(ground, &account.to_string()).await? {
    Some(Runner::Socket(socket)) => Some(socket),
    _ => None,
  })
}

/// What a local AI's test sends and where: the address, the model and the key file, under the data root's `env/`.
pub struct Tried<'a> {
  pub url: &'a str,
  pub model: &'a str,
  pub key_file: Option<&'a str>,
}

/// One request of a single answer token sent to a local AI from the person's PC (docs/supply.md, 「Testing a
/// source」): the chain asked first, then the PC's runner socket asked to reach the address, and the request spoken
/// over the stream it opens.
pub async fn test(ground: &Ground, account: i64, tried: &Tried<'_>) -> Result<Outcome, Refusal> {
  match allowed(ground, account).await {
    Ok(true) => {}
    Ok(false) => return Ok(Outcome::Withheld),
    Err(why) => return Ok(Outcome::Failed(why)),
  }

  let Some(socket) = pc_of(ground, account).await? else {
    return Ok(Outcome::NoPc);
  };
  let key = match tried.key_file {
    None => None,
    Some(file) => match std::fs::read_to_string(ground.roots.data.join("env").join(file)) {
      Ok(key) if !key.trim().is_empty() => Some(key.trim().to_owned()),
      _ => return Ok(Outcome::Failed(format!("the secret {file} could not be read"))),
    },
  };
  let wait = Duration::from_secs(SUPPLY.local_ai.test_seconds);

  Ok(
    tokio::time::timeout(wait, asked(&socket, &account.to_string(), tried, key.as_deref()))
      .await
      .unwrap_or_else(|_| Outcome::Failed(format!("no answer came within {} seconds", wait.as_secs()))),
  )
}

// The stream to the address opened through the PC's runner socket, and the request spoken over it.
async fn asked(socket: &Path, account: &str, tried: &Tried<'_>, key: Option<&str>) -> Outcome {
  let Some(to) = reached_at(tried.url) else {
    return Outcome::Failed(format!("{} names no host", tried.url));
  };
  let mut stream = match tokio::net::UnixStream::connect(socket).await {
    Ok(stream) => stream,
    Err(why) => return Outcome::Unreachable(format!("the PC's link: {why}")),
  };
  let mut ask = json!({ "owner": account, "reach": to }).to_string();

  ask.push('\n');

  if let Err(why) = stream.write_all(ask.as_bytes()).await {
    return Outcome::Unreachable(format!("the PC's link: {why}"));
  }

  // The link's answer is one line, read a byte at a time so nothing of the stream behind it is taken.
  let mut line = Vec::new();

  loop {
    let mut byte = [0u8; 1];

    match stream.read(&mut byte).await {
      Ok(1) if byte[0] == b'\n' => break,
      Ok(1) => line.push(byte[0]),
      _ => return Outcome::Unreachable("the PC's link closed before it answered".to_owned()),
    }
  }

  match serde_json::from_slice::<Value>(&line) {
    Ok(said) if said["t"] == "reached" && said.get("why").is_none_or(Value::is_null) => {}
    Ok(said) => {
      let why = said["why"].as_str().unwrap_or("the PC did not reach it").to_owned();

      return Outcome::Unreachable(why);
    }
    Err(_) => return Outcome::Unreachable("the PC's link answered what does not read".to_owned()),
  }

  spoken(stream, tried, key).await
}

// One chat completions request of a single answer token over a stream already joined to the address.
async fn spoken(stream: tokio::net::UnixStream, tried: &Tried<'_>, key: Option<&str>) -> Outcome {
  let Ok(parsed) = url::Url::parse(tried.url) else {
    return Outcome::Failed(format!("{} does not read as an address", tried.url));
  };
  let (mut sender, connection) =
    match hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream)).await {
      Ok(opened) => opened,
      Err(why) => return Outcome::Failed(format!("the address: {why}")),
    };

  tokio::spawn(connection);

  let body = json!({
    "model": tried.model,
    "messages": [{ "role": "user", "content": SUPPLY.test.asked }],
    "max_tokens": 1,
  });
  let host = reached_at(tried.url).unwrap_or_default();
  let mut request = hyper::Request::post(parsed.path())
    .header(hyper::header::HOST, host)
    .header(hyper::header::CONTENT_TYPE, "application/json")
    .header(hyper::header::ACCEPT, "application/json");

  if let Some(key) = key {
    request = request.header(hyper::header::AUTHORIZATION, format!("Bearer {key}"));
  }

  let Ok(request) = request.body(Full::new(Bytes::from(body.to_string()))) else {
    return Outcome::Failed("the request could not be made".to_owned());
  };
  let answered = match sender.send_request(request).await {
    Ok(answered) => answered,
    Err(why) => return Outcome::Failed(format!("the address: {why}")),
  };
  let status = answered.status().as_u16();
  let text = match answered.into_body().collect().await {
    Ok(collected) => String::from_utf8_lossy(&collected.to_bytes()).into_owned(),
    Err(why) => return Outcome::Failed(format!("the answer: {why}")),
  };

  local_outcome(status, &text)
}

/// What a local AI's answer to the test's one request comes to: any success answers; a 401 or a 403 is the server
/// refusing the key; anything else reached no answer, with what it said.
pub fn local_outcome(status: u16, text: &str) -> Outcome {
  if (200..300).contains(&status) {
    return Outcome::Answers;
  }

  let read: Value = serde_json::from_str(text).unwrap_or(Value::Null);
  let said = read["error"]["message"]
    .as_str()
    .or_else(|| read["error"].as_str())
    .or_else(|| read["message"].as_str())
    .map(str::to_owned)
    .unwrap_or_else(|| text.trim().chars().take(300).collect());
  let said = if said.is_empty() {
    format!("it answered {status}")
  } else {
    format!("it answered {status}: {said}")
  };

  if matches!(status, 401 | 403) { Outcome::Refused(said) } else { Outcome::Failed(said) }
}
