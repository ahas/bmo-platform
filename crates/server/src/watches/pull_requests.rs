//! Pull request reviews (docs/pull-request-reviews.md): GitHub's delivery read and checked against the webhook
//! secret, the fallback's one search an hour, a review item filed for the worker that holds the repository, and
//! a merge letting the items that waited on the pull request go on. No model runs here.

use std::collections::BTreeMap;

use hmac::{Hmac, Mac};
use policy::fill_marks;
use policy::pull_requests::PULL_REQUESTS;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};
use sha2::Sha256;
use store::all;

use crate::job_end::{address, token};

/// The variable naming the webhook secret, and the one naming the file it stands in.
pub const SECRET: &str = "BMO_GITHUB_WEBHOOK_SECRET";
pub const SECRET_FILE: &str = "BMO_GITHUB_WEBHOOK_SECRET_FILE";
/// The variable naming the GitHub login whose repositories the fallback searches.
pub const OWNER: &str = "BMO_GITHUB_ORG";

/// The fallback's search: every open pull request of the repositories a login owns.
pub fn search_query(owner: &str) -> String {
  format!("is:pr is:open user:{}", owner.trim())
}

/// The fallback's GraphQL request: one page of the search, each pull request with who opened it and where its
/// head branch stands.
pub const SEARCH_GRAPHQL: &str = "query($q: String!, $n: Int!) { search(query: $q, type: ISSUE, first: $n) { \
  nodes { ... on PullRequest { url number title authorAssociation repository { nameWithOwner } \
  headRepository { nameWithOwner } } } } }";

// The host a GitHub remote names.
const GITHUB: &str = "github.com";

/// A pull request as agency acts on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequest {
  pub url: String,
  /// `owner/name`.
  pub repository: String,
  pub number: i64,
  pub title: String,
  pub head: Option<String>,
  /// The author's association with the repository as GitHub names it (`OWNER`, `MEMBER`, `CONTRIBUTOR`, ...),
  /// empty where it was not said.
  pub author: String,
  /// The `owner/name` the head branch stands in, or nothing where GitHub names none (a fork since deleted).
  pub head_repository: Option<String>,
}

impl PullRequest {
  /// Whether the pull request is the owner's own (docs/pull-request-reviews.md, 「Who opened it」): its author a
  /// trusted association and its head branch in the repository it is opened on.
  pub fn owners_own(&self) -> bool {
    PULL_REQUESTS.trusted_authors.contains(&self.author)
      && self
        .head_repository
        .as_deref()
        .is_some_and(|head| head.eq_ignore_ascii_case(&self.repository))
  }
}

/// What a delivery or the fallback asks of agency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivered {
  Open(PullRequest),
  Pushed(PullRequest),
  Merged(PullRequest),
  Closed(PullRequest),
}

/// Whether a delivery's `X-Hub-Signature-256` is the body's signature under the secret.
pub fn signed(secret: &str, body: &[u8], signature: &str) -> bool {
  let Some(given) = signature.strip_prefix("sha256=").and_then(|hex| hex::decode(hex).ok()) else {
    return false;
  };
  let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
    return false;
  };

  mac.update(body);
  mac.verify_slice(&given).is_ok()
}

/// What a `pull_request` delivery asks, or nothing for an event or an action agency does not act on.
pub fn read_delivery(event: &str, body: &Value) -> Option<Delivered> {
  if event != "pull_request" {
    return None;
  }

  let pull = &body["pull_request"];
  let request = PullRequest {
    url: pull["html_url"].as_str()?.to_owned(),
    repository: body["repository"]["full_name"].as_str()?.to_owned(),
    number: pull["number"].as_i64().or_else(|| body["number"].as_i64())?,
    title: pull["title"].as_str().unwrap_or_default().to_owned(),
    head: pull["head"]["sha"].as_str().map(str::to_owned),
    author: pull["author_association"].as_str().unwrap_or_default().to_owned(),
    head_repository: pull["head"]["repo"]["full_name"].as_str().map(str::to_owned),
  };

  match body["action"].as_str()? {
    "opened" | "reopened" | "ready_for_review" => Some(Delivered::Open(request)),
    "synchronize" => Some(Delivered::Pushed(request)),
    "closed" if pull["merged"].as_bool() == Some(true) => Some(Delivered::Merged(request)),
    "closed" => Some(Delivered::Closed(request)),
    _ => None,
  }
}

/// The `owner/name` a GitHub remote names, or nothing for a remote that is no GitHub repository.
pub fn repository_of(remote: &str) -> Option<String> {
  let remote = remote.trim();
  // The scheme where it names one, then a user before the host where it names one, then the host.
  let rest = ["ssh://", "https://", "http://"]
    .iter()
    .find_map(|scheme| remote.strip_prefix(scheme))
    .unwrap_or(remote);
  let rest = rest.split_once('@').map_or(rest, |(_, after)| after);
  let path = rest.strip_prefix(GITHUB).and_then(|after| after.strip_prefix([':', '/']))?;
  let path = path.trim_end_matches('/').trim_end_matches(".git");
  let mut parts = path.split('/');
  let (owner, name) = (parts.next()?, parts.next()?);

  (parts.next().is_none() && !owner.is_empty() && !name.is_empty()).then(|| format!("{owner}/{name}"))
}

/// The review item a pull request is filed as for the worker that holds its repository: in the unattended lane
/// where it is the owner's own, and otherwise in the approval lane with the brief that never merges.
pub fn review_item(request: &PullRequest, worker: &str) -> Value {
  let marks = [
    ("repository", request.repository.as_str()),
    ("number", &request.number.to_string()),
    ("title", request.title.as_str()),
    ("url", request.url.as_str()),
  ];

  let (lane, brief) = if request.owners_own() {
    (&PULL_REQUESTS.lane, &PULL_REQUESTS.brief)
  } else {
    (&PULL_REQUESTS.approval_lane, &PULL_REQUESTS.approval_brief)
  };
  let mut labels = json!({ "pr": request.url, "lane": lane });

  if let Some(head) = &request.head {
    labels["head"] = json!(head);
  }

  json!({
    "title": fill_marks(&PULL_REQUESTS.title, &marks),
    "brief": fill_marks(brief, &marks),
    "queueName": PULL_REQUESTS.queue,
    "assigneeName": worker,
    "requesterName": PULL_REQUESTS.requester,
    "labels": labels,
  })
}

/// The workers that hold a GitHub repository, by `owner/name` in lower case: each worker's checkout's
/// `origin` remote.
pub async fn holders(db: &DatabaseConnection) -> BTreeMap<String, String> {
  let rows = all(db, r#"SELECT "name", "path" AS "cwd" FROM "repositories" ORDER BY "name""#, vec![])
    .await
    .unwrap_or_default();
  let mut held = BTreeMap::new();

  for row in rows {
    let (Ok(name), Ok(cwd)) = (row.try_get::<String>("", "name"), row.try_get::<String>("", "cwd")) else {
      continue;
    };
    let remote = tokio::process::Command::new("git")
      .args(["-C", &cwd, "remote", "get-url", "origin"])
      .output()
      .await
      .ok()
      .filter(|out| out.status.success())
      .map(|out| String::from_utf8_lossy(&out.stdout).into_owned());

    if let Some(repository) = remote.as_deref().and_then(repository_of) {
      held.entry(repository.to_lowercase()).or_insert(name);
    }
  }

  held
}

// The job service as the watch reaches it: its address and the deployment's token.
struct Jobs {
  url: String,
  token: String,
  client: reqwest::Client,
}

impl Jobs {
  fn of(vars: &BTreeMap<String, String>) -> Option<Self> {
    Some(Self {
      url: address(vars, "BMO_JOB_URL")?,
      token: token(vars, "BMO_JOB_TOKEN", "BMO_JOB_TOKEN_FILE")?,
      client: reqwest::Client::new(),
    })
  }

  async fn send(
    &self,
    method: reqwest::Method,
    path: &str,
    query: &[(&str, &str)],
    body: Option<&Value>,
  ) -> Result<Value, String> {
    let mut asked = self
      .client
      .request(method, format!("{}{path}", self.url))
      .query(query)
      .bearer_auth(&self.token);

    if let Some(body) = body {
      asked = asked.json(body);
    }

    let answer = asked.send().await.map_err(|why| why.to_string())?;
    let status = answer.status();
    let read = answer.json::<Value>().await.unwrap_or(Value::Null);

    if status.is_success() {
      Ok(read)
    } else {
      Err(format!("the job service answered {} to {path}: {}", status.as_u16(), read["error"]))
    }
  }

  // Every item labelled with a pull request's address, standing or ended.
  async fn labelled(&self, url: &str) -> Result<Vec<Value>, String> {
    let label = format!("pr={url}");
    let page = self
      .send(reqwest::Method::GET, "/items", &[("label", &label), ("limit", "100")], None)
      .await?;

    Ok(page["items"].as_array().cloned().unwrap_or_default())
  }

  // The items labelled with a pull request's address that still stand.
  async fn standing(&self, url: &str) -> Result<Vec<Value>, String> {
    Ok(self.labelled(url).await?.into_iter().filter(stands).collect())
  }

  async fn file(&self, request: &PullRequest, worker: &str) -> Result<(), String> {
    self
      .send(reqwest::Method::POST, "/items", &[], Some(&review_item(request, worker)))
      .await?;
    let waits = if request.owners_own() { "" } else { ", waiting for the owner's approval" };

    println!("pull requests: {} filed for {worker}{waits}", request.url);

    Ok(())
  }

  async fn note(&self, item: &Value, text: &str) -> Result<(), String> {
    let id = item["id"].as_i64().unwrap_or_default();

    self
      .send(
        reqwest::Method::POST,
        &format!("/items/{id}/notes"),
        &[],
        Some(&json!({ "text": text, "actorName": PULL_REQUESTS.requester })),
      )
      .await
      .map(|_| ())
  }
}

// A review item, in either lane.
fn is_review(item: &Value) -> bool {
  item["labels"]["lane"]
    .as_str()
    .is_some_and(|lane| lane == PULL_REQUESTS.lane || lane == PULL_REQUESTS.approval_lane)
}

// Whether an item still stands: neither done, dropped nor merged.
fn stands(item: &Value) -> bool {
  !matches!(item["state"].as_str(), Some("done" | "dropped" | "merged"))
}

/// Whether a review item was filed for a head (docs/pull-request-reviews.md, 「The review item」): one labelled
/// with that head, or one that names no head; where the head is not known, any.
pub fn filed_for(reviews: &[&Value], head: Option<&str>) -> bool {
  reviews.iter().any(|item| match (head, item["labels"]["head"].as_str()) {
    (Some(head), Some(filed)) => head == filed,
    _ => true,
  })
}

/// Whether a push files a new review item: none of the pull request's review items stands, the latest of them
/// is done — the worker sent it back — and none was labelled with the pushed head.
pub fn push_files(reviews: &[&Value], head: Option<&str>) -> bool {
  let latest = reviews.iter().max_by_key(|item| item["id"].as_i64().unwrap_or_default());

  !reviews.iter().any(|item| stands(item))
    && latest.is_some_and(|item| item["state"].as_str() == Some("done"))
    && !reviews.iter().any(|item| head.is_some() && item["labels"]["head"].as_str() == head)
}

/// One delivery acted on: an open pull request a worker holds filed once for that worker and head, a push told on
/// its standing item or filed afresh where its item was sent back, a merge opening the items that waited on it, a
/// close told.
pub async fn act(
  vars: &BTreeMap<String, String>,
  held: &BTreeMap<String, String>,
  delivered: &Delivered,
) -> Result<(), String> {
  let Some(jobs) = Jobs::of(vars) else { return Ok(()) };

  match delivered {
    Delivered::Open(request) => {
      // A repository no worker holds is let be, and a pull request already filed for its head is filed once.
      if let Some(worker) = held.get(&request.repository.to_lowercase()) {
        let items = jobs.labelled(&request.url).await?;
        let reviews: Vec<&Value> = items.iter().filter(|item| is_review(item)).collect();

        if !reviews.iter().any(|item| stands(item)) && !filed_for(&reviews, request.head.as_deref()) {
          jobs.file(request, worker).await?;
        }
      }
    }
    Delivered::Pushed(request) => {
      let head = request.head.as_deref().unwrap_or_default();
      let items = jobs.labelled(&request.url).await?;
      let reviews: Vec<&Value> = items.iter().filter(|item| is_review(item)).collect();

      for item in reviews.iter().filter(|item| stands(item)) {
        jobs.note(item, &fill_marks(&PULL_REQUESTS.pushed, &[("head", head)])).await?;
      }

      // A review the worker sent back is filed again for the head pushed after it.
      if let Some(worker) = held.get(&request.repository.to_lowercase())
        && push_files(&reviews, request.head.as_deref())
      {
        jobs.file(request, worker).await?;
      }
    }
    Delivered::Merged(request) => {
      let marks = [("url", request.url.as_str())];

      for item in jobs.standing(&request.url).await? {
        if is_review(&item) {
          jobs.note(&item, &fill_marks(&PULL_REQUESTS.merged_elsewhere, &marks)).await?;
        } else if item["state"].as_str() == Some("held") {
          let id = item["id"].as_i64().unwrap_or_default();

          jobs.note(&item, &fill_marks(&PULL_REQUESTS.merged, &marks)).await?;
          jobs
            .send(reqwest::Method::PATCH, &format!("/items/{id}"), &[], Some(&json!({ "state": "open" })))
            .await?;
        }
      }
    }
    Delivered::Closed(request) => {
      for item in jobs.standing(&request.url).await? {
        jobs
          .note(&item, &fill_marks(&PULL_REQUESTS.closed, &[("url", request.url.as_str())]))
          .await?;
      }
    }
  }

  Ok(())
}

/// The open pull requests one page of GitHub's search answers (`SEARCH_GRAPHQL`), as the fallback reads them.
pub fn searched(page: &Value) -> Vec<PullRequest> {
  page["data"]["search"]["nodes"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|node| {
      Some(PullRequest {
        url: node["url"].as_str()?.to_owned(),
        repository: node["repository"]["nameWithOwner"].as_str()?.to_owned(),
        number: node["number"].as_i64()?,
        title: node["title"].as_str().unwrap_or_default().to_owned(),
        head: None,
        author: node["authorAssociation"].as_str().unwrap_or_default().to_owned(),
        head_repository: node["headRepository"]["nameWithOwner"].as_str().map(str::to_owned),
      })
    })
    .collect()
}

/// The fallback: one search for every open pull request of the repositories the owner's login owns, and each a
/// delivery missed filed; nothing where the deployment names no login.
pub async fn fallback(db: &DatabaseConnection, vars: &BTreeMap<String, String>) -> Result<(), String> {
  let Some(owner) = vars.get(OWNER).map(|owner| owner.trim()).filter(|owner| !owner.is_empty()) else {
    return Ok(());
  };
  let graphql = format!("query={SEARCH_GRAPHQL}");
  let query = format!("q={}", search_query(owner));
  let per_page = format!("n={}", PULL_REQUESTS.page);
  let out = tokio::process::Command::new("gh")
    .args(["api", "graphql", "-f", &graphql, "-f", &query, "-F", &per_page])
    .output()
    .await
    .map_err(|why| format!("gh: {why}"))?;

  if !out.status.success() {
    return Err(format!("gh: {}", String::from_utf8_lossy(&out.stderr).trim()));
  }

  let page: Value = serde_json::from_slice(&out.stdout).map_err(|why| format!("gh's answer: {why}"))?;
  let held = holders(db).await;

  for request in searched(&page) {
    act(vars, &held, &Delivered::Open(request)).await?;
  }

  Ok(())
}

/// The webhook secret the deployment names, or nothing.
pub fn secret(vars: &BTreeMap<String, String>) -> Option<String> {
  token(vars, SECRET, SECRET_FILE)
}
