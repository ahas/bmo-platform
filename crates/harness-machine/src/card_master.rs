//! The arbiter's master as agency asks it (docs/own-llm.md, 「An engine the arbiter stands」): where the
//! deployment names it, a demand for one card of a service, the demand read and given back, the demands it
//! holds, and the instances of a service the master reports standing. Each call is the master's documented
//! route (the arbiter's `docs/api.md`, 「Demands」 and 「The cluster」).

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

/// The variable naming the master's address.
pub const URL_VARIABLE: &str = "AGENCY_ARBITER_URL";

/// The variable naming the settings file of the arbiter's deployment the master's token is read from.
pub const TOKEN_FILE_VARIABLE: &str = "AGENCY_ARBITER_TOKEN_FILE";

/// The entry of that file holding the master's token.
pub const TOKEN_ENTRY: &str = "CARD_MASTER_TOKEN";

/// How long one read of a demand holds its answer open at the master, the longest a read waits for.
pub const DEMAND_WAIT: Duration = Duration::from_secs(30);

/// Where the master is reached, and the token it is called with where the deployment names one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MasterAddress {
  pub url: Url,
  pub token: Option<String>,
}

impl MasterAddress {
  /// The address the variables name through `variable`: none where the address is unset or blank, and a
  /// fault naming the variable where it does not read as an HTTP URL. The token is the settings file's
  /// `CARD_MASTER_TOKEN`, read as the arbiter's deployment keeps it; a file that does not read or holds no
  /// such entry is no token.
  pub fn read(variable: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, String> {
    let named =
      |name: &str| variable(name).map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
    let Some(given) = named(URL_VARIABLE) else {
      return Ok(None);
    };
    let url =
      Url::parse(&given).map_err(|why| format!("{URL_VARIABLE}: {given} is not an address: {why}"))?;

    if !matches!(url.scheme(), "http" | "https") {
      return Err(format!("{URL_VARIABLE}: {given} is not an HTTP address"));
    }

    Ok(Some(Self {
      url,
      token: named(TOKEN_FILE_VARIABLE).and_then(|file| token_of(Path::new(&file))),
    }))
  }

  /// The address this process's environment names.
  pub fn of_process() -> Result<Option<Self>, String> {
    Self::read(|name| std::env::var(name).ok())
  }
}

/// The master's token in a settings file of `KEY=value` lines, or none.
pub fn token_of(file: &Path) -> Option<String> {
  entry_of(file, TOKEN_ENTRY)
}

/// One entry of a settings file of `KEY=value` lines, trimmed, or none where the file does not read or
/// holds no such entry or an empty one.
pub fn entry_of(file: &Path, entry: &str) -> Option<String> {
  dotenvy::from_path_iter(file)
    .ok()?
    .filter_map(Result::ok)
    .find(|(name, _)| name == entry)
    .map(|(_, value)| value.trim().to_owned())
    .filter(|value| !value.is_empty())
}

/// One card a demand was granted, as the master names it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantedCard {
  pub node_name: String,
  #[serde(default)]
  pub address: Option<String>,
  #[serde(default)]
  pub key_file: Option<String>,
}

/// Why a demand's card waits: a kebab-case code and an English sentence.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Reason {
  pub code: String,
  pub text: String,
}

/// A demand as the master answers it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Demand {
  pub id: i64,
  pub service_name: String,
  #[serde(default)]
  pub cards: Vec<GrantedCard>,
  /// `waiting`, `granted`, `partly` or `lent`.
  pub state: String,
  #[serde(default)]
  pub taken: Option<Value>,
  /// Whether a demand of a better rank took its card, rather than a claim or a lower tier.
  #[serde(default)]
  pub outranked: bool,
  #[serde(default)]
  pub reason: Option<Reason>,
  #[serde(default)]
  pub within: Option<i64>,
  #[serde(default)]
  pub work_key: Option<String>,
  /// The reference its asker showed it under.
  #[serde(rename = "ref", default)]
  pub reference: Option<String>,
}

impl Demand {
  /// The card the demand holds now, where it is granted one with no claim or lower tier taking it.
  pub fn card(&self) -> Option<&GrantedCard> {
    (self.state == "granted" && self.taken.as_ref().is_none_or(Value::is_null))
      .then(|| self.cards.first())
      .flatten()
  }

  /// Whether the demand's card is lent to a demand asked within it.
  pub fn lent(&self) -> bool {
    self.state == "lent"
  }

  /// What the demand waits on, as the master says it.
  pub fn waiting_on(&self) -> Option<String> {
    self.reason.as_ref().map(|reason| reason.text.clone()).or_else(|| {
      self.taken.as_ref().filter(|taken| !taken.is_null()).map(|taken| match taken {
        Value::String(name) => format!("The card was taken by {name}."),
        other => format!("The card was taken by {other}."),
      })
    })
  }
}

/// What a demand asks.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandAsk {
  pub service_name: String,
  pub count: u32,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub within: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub work_key: Option<String>,
  #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
  pub reference: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub rank: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub pool: Option<CardPool>,
}

/// A pool a demand stands in: a key that means nothing to the master, and the most cards the demands of that
/// key use of one service at once.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct CardPool {
  pub key: String,
  pub cards: u32,
}

/// What a caller says of the demands of its work: their rank and their pool, each its own setting.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CardTerms {
  pub rank: Option<u32>,
  pub pool: Option<CardPool>,
}

/// The header a gateway request names its demand's rank in.
pub const RANK_HEADER: &str = "x-bmo-card-rank";
/// The header a gateway request names its demand's pool in, as `<cards> <key>`.
pub const POOL_HEADER: &str = "x-bmo-card-pool";

impl CardTerms {
  /// The terms a gateway request's two headers name; one that does not read names nothing.
  pub fn of_headers(rank: Option<&str>, pool: Option<&str>) -> Self {
    Self {
      rank: rank.and_then(|rank| rank.trim().parse().ok()),
      pool: pool.and_then(|pool| {
        let (cards, key) = pool.trim().split_once(' ')?;
        let cards = cards.parse().ok().filter(|cards| *cards > 0)?;
        let key = key.trim();
        (!key.is_empty()).then(|| CardPool {
          key: key.to_owned(),
          cards,
        })
      }),
    }
  }

  /// The header lines a gateway request names them in.
  pub fn headers(&self) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(rank) = self.rank {
      lines.push(format!("{RANK_HEADER}: {rank}"));
    }
    if let Some(pool) = &self.pool {
      lines.push(format!("{POOL_HEADER}: {} {}", pool.cards, pool.key));
    }
    lines
  }
}

/// One node a service stands on now, as the master reports it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
  pub node_name: String,
  #[serde(default)]
  pub address: Option<String>,
  #[serde(default)]
  pub key_file: Option<String>,
}

/// A service as the master's picture shows it: the instances standing now, and whether it is placed on a
/// node no claim and no lower tier holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Standing {
  pub instances: Vec<Instance>,
  pub open: bool,
}

/// Why a call to the master came back without its answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MasterFault {
  /// The master was not reached, or its answer did not arrive.
  Unreachable(String),
  /// The master answered with a status that is not a success.
  Failed { status: u16, error: String },
  /// The master answered in a shape the call does not read.
  Malformed(String),
}

impl std::fmt::Display for MasterFault {
  fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Unreachable(detail) => write!(out, "the arbiter's master could not be reached: {detail}"),
      Self::Failed { status, error } => write!(out, "the arbiter's master answered {status}: {error}"),
      Self::Malformed(detail) => write!(out, "the arbiter's master answered out of shape: {detail}"),
    }
  }
}

impl std::error::Error for MasterFault {}

/// The master, called over HTTP.
#[derive(Clone, Debug)]
pub struct Master {
  address: MasterAddress,
  http: reqwest::Client,
}

impl Master {
  pub fn new(address: MasterAddress) -> Self {
    Self {
      address,
      http: reqwest::Client::new(),
    }
  }

  pub fn address(&self) -> &MasterAddress {
    &self.address
  }

  fn at(&self, path: &str) -> String {
    format!("{}{path}", self.address.url.as_str().trim_end_matches('/'))
  }

  // A call waits for the master's answer: its end is that answer, or the connection refused or broken.
  fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match &self.address.token {
      Some(token) => request.bearer_auth(token),
      None => request,
    }
  }

  async fn read<T: for<'de> Deserialize<'de>>(answer: reqwest::Response) -> Result<T, MasterFault> {
    let status = answer.status();

    if !status.is_success() {
      let error = answer.text().await.unwrap_or_default();

      return Err(MasterFault::Failed {
        status: status.as_u16(),
        error: error.chars().take(400).collect(),
      });
    }

    answer.json::<T>().await.map_err(|why| MasterFault::Malformed(why.to_string()))
  }

  /// A demand asked: answered as it stands, granted or waiting.
  pub async fn ask(&self, ask: &DemandAsk) -> Result<Demand, MasterFault> {
    let answer = self
      .authorized(self.http.post(self.at("/demands")).json(ask))
      .send()
      .await
      .map_err(|why| MasterFault::Unreachable(why.to_string()))?;

    Self::read(answer).await
  }

  /// A demand read, its answer held open up to `wait` for a change; none where the master holds no such
  /// demand.
  pub async fn demand(&self, id: i64, wait: Duration) -> Result<Option<Demand>, MasterFault> {
    self.read_demand(&format!("/demands/{id}?waitMs={}", wait.as_millis())).await
  }

  /// A demand a caller stands on read, its answer held open up to `wait` for a change whatever its state; none
  /// where the master holds no such demand.
  pub async fn watch(&self, id: i64, wait: Duration) -> Result<Option<Demand>, MasterFault> {
    self.read_demand(&format!("/demands/{id}?waitMs={}&watch=true", wait.as_millis())).await
  }

  async fn read_demand(&self, path: &str) -> Result<Option<Demand>, MasterFault> {
    let request = self.http.get(self.at(path));
    let answer = self
      .authorized(request)
      .send()
      .await
      .map_err(|why| MasterFault::Unreachable(why.to_string()))?;

    if answer.status() == reqwest::StatusCode::NOT_FOUND {
      return Ok(None);
    }

    Self::read(answer).await.map(Some)
  }

  /// A demand given back; one the master no longer holds is given back already.
  pub async fn give_back(&self, id: i64) -> Result<(), MasterFault> {
    let answer = self
      .authorized(self.http.delete(self.at(&format!("/demands/{id}"))))
      .send()
      .await
      .map_err(|why| MasterFault::Unreachable(why.to_string()))?;
    let status = answer.status();

    if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
      return Ok(());
    }

    Err(MasterFault::Failed {
      status: status.as_u16(),
      error: answer.text().await.unwrap_or_default().chars().take(400).collect(),
    })
  }

  /// Every demand the master holds, in the order they were asked.
  pub async fn demands(&self) -> Result<Vec<Demand>, MasterFault> {
    #[derive(Deserialize)]
    struct Page {
      items: Vec<Demand>,
    }

    let answer = self
      .authorized(self.http.get(self.at("/demands")))
      .send()
      .await
      .map_err(|why| MasterFault::Unreachable(why.to_string()))?;

    Self::read::<Page>(answer).await.map(|page| page.items)
  }

  /// The master's whole picture.
  pub async fn picture(&self) -> Result<Value, MasterFault> {
    let answer = self
      .authorized(self.http.get(self.at("/cluster")))
      .send()
      .await
      .map_err(|why| MasterFault::Unreachable(why.to_string()))?;

    Self::read(answer).await
  }

  /// A service as the master's picture shows it, or none where the master places no such service.
  pub async fn standing(&self, service: &str) -> Result<Option<Standing>, MasterFault> {
    Ok(standing_in(&self.picture().await?, service))
  }
}

/// A service as a picture of the master's shows it: its standing instances that name an address, and
/// whether a node of its placement is held by no claim and no turn or ticket of a lower tier; none where
/// the picture places no such service.
pub fn standing_in(picture: &Value, service: &str) -> Option<Standing> {
  let placed = picture
    .get("services")?
    .as_array()?
    .iter()
    .find(|one| one.get("serviceName").and_then(Value::as_str) == Some(service))?;
  let instances: Vec<Instance> = placed
    .get("instances")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|one| serde_json::from_value::<Instance>(one.clone()).ok())
    .filter(|one| one.address.is_some())
    .collect();
  let tiers: Vec<(String, f64)> = picture
    .get("tiers")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|one| Some((one.get("name")?.as_str()?.to_owned(), one.get("tier")?.as_f64()?)))
    .collect();
  let tier_of = |name: &str| tiers.iter().find(|(named, _)| named == name).map(|(_, tier)| *tier);
  let own = placed.get("tier").and_then(Value::as_f64).or_else(|| tier_of(service));
  let lower = |name: Option<&str>| match (name.and_then(tier_of), own) {
    (Some(theirs), Some(own)) => theirs < own,
    _ => false,
  };
  let nodes = placed.get("nodeNames").and_then(Value::as_array).into_iter().flatten();
  let open = nodes.filter_map(Value::as_str).any(|node| {
    let Some(held) = picture
      .get("nodes")
      .and_then(Value::as_array)
      .and_then(|nodes| nodes.iter().find(|one| one.get("name").and_then(Value::as_str) == Some(node)))
    else {
      return false;
    };

    if held.pointer("/link/state").and_then(Value::as_str) == Some("lost") {
      return false;
    }

    held.get("cards").and_then(Value::as_array).into_iter().flatten().all(|card| {
      let claimed = card.get("claim").is_some_and(|claim| !claim.is_null());
      let turn = card.pointer("/turn/serviceName").and_then(Value::as_str);
      let queued = card
        .get("queue")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|ticket| lower(ticket.get("serviceName").and_then(Value::as_str)));

      !claimed && !lower(turn) && !queued
    })
  });

  Some(Standing { instances, open })
}
