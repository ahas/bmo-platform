//! The orderer of an item (docs/agent-control.md, 「An item's change is told to its orderer」): the supervisor
//! of the room the item was asked in — its own room, else the room of the item it was raised from, followed up
//! the chain — or the owner's private room where none is found. Every notice of a change in the queue finds
//! the room it is told in here.

use std::collections::{BTreeMap, BTreeSet};

use policy::job_notices::JOB_NOTICES;
use serde_json::Value;

use crate::job_end::{address, token};

/// The label naming the item whose job raised an item, by its number.
pub const RAISED_BY: &str = "raised-by";

/// Whom a change of an item is told to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Orderer {
  /// The room the orderer answers; none for the owner's private room.
  pub room: Option<i64>,
  /// The requester of the item the room was found on, where it names one.
  pub requester: Option<String>,
}

/// The room an item was asked in: its `roomId`, else the room its `scope` names; none for an item that names
/// neither.
pub fn room_of(item: &Value) -> Option<i64> {
  item["roomId"]
    .as_i64()
    .or_else(|| item["scope"].as_str().and_then(|scope| scope.trim().parse().ok()))
    .or_else(|| item["scope"].as_i64())
    .filter(|room| *room != 0)
}

/// The item an item was raised from: its `raisedBy` field where the job service answers one, else the item
/// its `raised-by` label names.
pub fn parent_of(item: &Value) -> Option<i64> {
  let label = &item["labels"][RAISED_BY];

  item["raisedBy"]
    .as_i64()
    .filter(|parent| *parent > 0)
    .or_else(|| label.as_i64())
    .or_else(|| label.as_str().and_then(|number| number.trim().trim_start_matches('#').parse().ok()))
    .filter(|parent| *parent > 0)
}

fn requester_of(item: &Value) -> Option<String> {
  item["requesterName"]
    .as_str()
    .map(str::trim)
    .filter(|name| !name.is_empty())
    .map(str::to_owned)
}

/// The job service as the queue's notices read it: its address and token.
#[derive(Clone, Debug)]
pub struct JobService {
  pub url: String,
  pub token: String,
}

impl JobService {
  /// The job service the deployment names; none where it names none, or the ground is a test's.
  pub fn of(vars: &BTreeMap<String, String>) -> Option<Self> {
    let (Some(url), Some(token)) =
      (address(vars, "BMO_JOB_URL"), token(vars, "BMO_JOB_TOKEN", "BMO_JOB_TOKEN_FILE"))
    else {
      return None;
    };

    Some(Self { url, token })
  }

  /// The job service under the supervisors' own token, the one it lets ask a person (the job service's
  /// docs/api.md, 「The addressee」), else under the deployment's where it names none; none where it names no
  /// service.
  pub fn of_supervisor(vars: &BTreeMap<String, String>) -> Option<Self> {
    let (Some(url), Some(token)) = (
      address(vars, "BMO_JOB_URL"),
      token(vars, "BMO_JOB_CHAT_TOKEN", "BMO_JOB_CHAT_TOKEN_FILE")
        .or_else(|| token(vars, "BMO_JOB_TOKEN", "BMO_JOB_TOKEN_FILE")),
    ) else {
      return None;
    };

    Some(Self { url, token })
  }

  /// One read of the service at `path` with `query`, its answer's body.
  pub async fn read(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, String> {
    let answer = reqwest::Client::new()
      .get(format!("{}{path}", self.url))
      .query(query)
      .bearer_auth(&self.token)
      .send()
      .await
      .map_err(|why| why.to_string())?;

    if !answer.status().is_success() {
      return Err(format!("the job service answered {} to {path}", answer.status().as_u16()));
    }

    answer.json().await.map_err(|why| why.to_string())
  }

  /// Every page of a list of the service's at `path` with `query`, past `after` while it says more stand.
  pub async fn every_page(&self, path: &str, query: &[(&str, &str)]) -> Result<Vec<Value>, String> {
    let mut held = Vec::new();
    let mut after: Option<String> = None;

    loop {
      let mut asked = query.to_vec();

      asked.push(("limit", "500"));

      if let Some(after) = &after {
        asked.push(("after", after));
      }

      let page = self.read(path, &asked).await?;
      let members = page["items"].as_array().cloned().unwrap_or_default();

      held.extend(members.iter().cloned());

      match members.last().and_then(|last| last["id"].as_i64()) {
        Some(last) if page["more"] == true => after = Some(last.to_string()),
        _ => return Ok(held),
      }
    }
  }

  // One member at `path`; none where the service answers that it holds no such member.
  async fn member(&self, path: &str) -> Result<Option<Value>, String> {
    let answer = reqwest::Client::new()
      .get(format!("{}{path}", self.url))
      .bearer_auth(&self.token)
      .send()
      .await
      .map_err(|why| why.to_string())?;

    if answer.status() == reqwest::StatusCode::NOT_FOUND {
      return Ok(None);
    }

    if !answer.status().is_success() {
      return Err(format!("the job service answered {} to {path}", answer.status().as_u16()));
    }

    answer.json().await.map(Some).map_err(|why| why.to_string())
  }

  /// One item by its number; none where the service answers that it holds no such item.
  pub async fn item(&self, id: i64) -> Result<Option<Value>, String> {
    self.member(&format!("/items/{id}")).await
  }

  /// One question by its number; none where the service answers that it holds no such question.
  pub async fn question(&self, id: i64) -> Result<Option<Value>, String> {
    self.member(&format!("/questions/{id}")).await
  }

  /// The orderer of an item as the job service answers it: its own room, else its parent's up the chain,
  /// each parent read by its number; the owner's private room where the chain reaches no room, an item the
  /// service no longer holds, an item already passed, or the policy's depth.
  pub async fn orderer_of(&self, item: &Value) -> Result<Orderer, String> {
    if let Some(room) = room_of(item) {
      return Ok(Orderer {
        room: Some(room),
        requester: requester_of(item),
      });
    }

    let mut passed: BTreeSet<i64> = item["id"].as_i64().into_iter().collect();
    let mut next = parent_of(item);
    let mut read = 0;

    while let Some(id) = next.filter(|id| passed.insert(*id)) {
      if read == JOB_NOTICES.orderer_depth {
        break;
      }

      read += 1;

      let Some(parent) = self.item(id).await? else { break };

      if let Some(room) = room_of(&parent) {
        return Ok(Orderer {
          room: Some(room),
          requester: requester_of(&parent),
        });
      }

      next = parent_of(&parent);
    }

    Ok(Orderer {
      room: None,
      requester: requester_of(item),
    })
  }

  /// The orderer of the item by its number; the owner's private room where the service holds no such item.
  pub async fn orderer_of_id(&self, id: i64) -> Result<(Option<Value>, Orderer), String> {
    match self.item(id).await? {
      Some(item) => {
        let orderer = self.orderer_of(&item).await?;

        Ok((Some(item), orderer))
      }
      None => Ok((None, Orderer::default())),
    }
  }
}
