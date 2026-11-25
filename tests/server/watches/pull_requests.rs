//! Pull request reviews (docs/pull-request-reviews.md): a delivery checked against the webhook secret and read,
//! a worker's remote read as its repository, the fallback's search read, a review item filed once for the
//! worker that holds the repository, and a merge opening the item that waited on it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use agency_server::watches::pull_requests::{
  Delivered, PullRequest, SEARCH_GRAPHQL, act, read_delivery, repository_of, review_item, search_query,
  searched, signed,
};
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{Method, StatusCode, Uri};
use axum::response::IntoResponse;
use policy::pull_requests::PULL_REQUESTS;
use serde_json::{Value, json};
use tokio::net::TcpListener;

#[test]
fn a_delivery_is_github_s_where_its_signature_is_the_body_s_under_the_secret() {
  // GitHub's own example of a signed delivery.
  let secret = "It's a Secret to Everybody";
  let signature = "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";

  assert!(signed(secret, b"Hello, World!", signature));
  assert!(!signed(secret, b"Hello, World?", signature));
  assert!(!signed("another secret", b"Hello, World!", signature));
  assert!(!signed(secret, b"Hello, World!", "sha1=757107"));
}

fn delivery(action: &str, merged: bool) -> Value {
  json!({
    "action": action,
    "number": 7,
    "repository": { "full_name": "org/base-ui" },
    "pull_request": {
      "html_url": "https://github.com/org/base-ui/pull/7",
      "number": 7,
      "title": "Document the theme tokens",
      "merged": merged,
      "author_association": "MEMBER",
      "head": { "sha": "abc123", "repo": { "full_name": "org/base-ui" } },
      "base": { "repo": { "full_name": "org/base-ui" } },
    },
  })
}

fn request() -> PullRequest {
  PullRequest {
    url: "https://github.com/org/base-ui/pull/7".into(),
    repository: "org/base-ui".into(),
    number: 7,
    title: "Document the theme tokens".into(),
    head: Some("abc123".into()),
    author: "MEMBER".into(),
    head_repository: Some("org/base-ui".into()),
  }
}

#[test]
fn a_pull_request_event_reads_as_what_it_asks_and_every_other_event_as_nothing() {
  assert_eq!(
    read_delivery("pull_request", &delivery("opened", false)),
    Some(Delivered::Open(request()))
  );
  assert_eq!(
    read_delivery("pull_request", &delivery("synchronize", false)),
    Some(Delivered::Pushed(request()))
  );
  assert_eq!(
    read_delivery("pull_request", &delivery("closed", true)),
    Some(Delivered::Merged(request()))
  );
  assert_eq!(
    read_delivery("pull_request", &delivery("closed", false)),
    Some(Delivered::Closed(request()))
  );
  assert_eq!(read_delivery("pull_request", &delivery("labeled", false)), None);
  assert_eq!(read_delivery("push", &delivery("opened", false)), None);
}

#[test]
fn a_worker_s_remote_reads_as_the_github_repository_it_names() {
  // The SSH user is written apart, so no remote reads as an address in a tracked file.
  let user = "git";

  for remote in [
    format!("{user}@github.com:org/base-ui.git"),
    "https://github.com/org/base-ui".to_owned(),
    "https://github.com/org/base-ui.git\n".to_owned(),
    format!("ssh://{user}@github.com/org/base-ui.git"),
  ] {
    assert_eq!(repository_of(&remote).as_deref(), Some("org/base-ui"), "{remote}");
  }

  assert_eq!(repository_of(&format!("{user}@gitlab.com:org/base-ui.git")), None);
  assert_eq!(repository_of("https://github.com/org"), None);
}

#[test]
fn the_fallback_reads_every_open_pull_request_of_the_search_and_the_item_names_its_review() {
  let page = json!({ "data": { "search": { "nodes": [
    {
      "url": "https://github.com/org/base-ui/pull/7",
      "number": 7,
      "title": "Document the theme tokens",
      "authorAssociation": "MEMBER",
      "repository": { "nameWithOwner": "org/base-ui" },
      "headRepository": { "nameWithOwner": "org/base-ui" },
    },
    {
      "url": "https://github.com/org/base-ui/pull/8",
      "number": 8,
      "title": "Fix a typo",
      "authorAssociation": "MEMBER",
      "repository": { "nameWithOwner": "org/base-ui" },
      "headRepository": null,
    },
    // An issue the search names is no pull request and reads as nothing.
    {},
  ]}}});

  assert_eq!(
    searched(&page),
    vec![
      PullRequest {
        head: None,
        ..request()
      },
      PullRequest {
        url: "https://github.com/org/base-ui/pull/8".into(),
        number: 8,
        title: "Fix a typo".into(),
        head: None,
        head_repository: None,
        ..request()
      },
    ]
  );

  // The search asks of each pull request who opened it and where its head stands.
  for field in ["authorAssociation", "headRepository", "nameWithOwner"] {
    assert!(SEARCH_GRAPHQL.contains(field), "{field}");
  }

  // The login owns the repositories: a user's, so the search names it with `user:`, never `org:`.
  assert_eq!(search_query(" someone "), "is:pr is:open user:someone");

  let item = review_item(&request(), "base-ui");

  assert_eq!(item["assigneeName"], "base-ui");
  assert_eq!(item["queueName"], PULL_REQUESTS.queue.as_str());
  assert_eq!(
    item["labels"],
    json!({ "pr": "https://github.com/org/base-ui/pull/7", "lane": PULL_REQUESTS.lane, "head": "abc123" })
  );
  assert!(item["title"].as_str().unwrap().contains("#7 of org/base-ui"));
  assert!(item["brief"].as_str().unwrap().contains("base-library rule"));
}

#[test]
fn only_the_owner_s_own_pull_request_is_filed_in_the_unattended_lane() {
  let fork = PullRequest {
    head_repository: Some("stranger/base-ui".into()),
    ..request()
  };
  let gone = PullRequest {
    head_repository: None,
    ..request()
  };
  let by = |author: &str| PullRequest {
    author: author.into(),
    ..request()
  };

  for own in [request(), by("OWNER")] {
    assert!(own.owners_own(), "{own:?}");
    assert_eq!(review_item(&own, "base-ui")["labels"]["lane"], PULL_REQUESTS.lane.as_str());
  }

  // A fork, even the owner's, a fork since deleted, a collaborator, an outsider, and an author GitHub did not name.
  for other in [
    fork,
    gone,
    PullRequest {
      author: "OWNER".into(),
      head_repository: Some("org-fork/base-ui".into()),
      ..request()
    },
    by("COLLABORATOR"),
    by("CONTRIBUTOR"),
    by("FIRST_TIME_CONTRIBUTOR"),
    by("NONE"),
    by(""),
  ] {
    assert!(!other.owners_own(), "{other:?}");

    let item = review_item(&other, "base-ui");

    assert_eq!(item["labels"]["lane"], PULL_REQUESTS.approval_lane.as_str(), "{other:?}");
    assert_ne!(item["labels"]["lane"], PULL_REQUESTS.lane.as_str());
    assert_eq!(item["brief"], review_item(&by("NONE"), "base-ui")["brief"]);
    assert!(item["brief"].as_str().unwrap().contains("never merge it"), "{other:?}");
  }

  // A delivery reads who opened the pull request and where its head stands.
  let mut forked = delivery("opened", false);

  forked["pull_request"]["author_association"] = json!("CONTRIBUTOR");
  forked["pull_request"]["head"]["repo"] = json!({ "full_name": "stranger/base-ui" });

  let Some(Delivered::Open(read)) = read_delivery("pull_request", &forked) else {
    panic!("an opening reads as one");
  };

  assert_eq!(
    (read.author.as_str(), read.head_repository.as_deref()),
    ("CONTRIBUTOR", Some("stranger/base-ui"))
  );
  assert!(!read.owners_own());

  forked["pull_request"]["head"]["repo"] = Value::Null;

  let Some(Delivered::Open(read)) = read_delivery("pull_request", &forked) else {
    panic!("an opening reads as one");
  };

  assert_eq!(read.head_repository, None);
}

type Calls = Arc<Mutex<Vec<(String, String, String, Value)>>>;

// A job service holding the items the test names under one pull request's label, every call kept.
async fn jobs(
  State((calls, standing)): State<(Calls, Value)>,
  method: Method,
  uri: Uri,
  body: Bytes,
) -> impl IntoResponse {
  calls.lock().unwrap().push((
    method.to_string(),
    uri.path().to_owned(),
    uri.query().unwrap_or_default().to_owned(),
    serde_json::from_slice(&body).unwrap_or(Value::Null),
  ));

  match (method.as_str(), uri.path()) {
    ("GET", "/items") => (StatusCode::OK, axum::Json(json!({ "items": standing, "more": false }))),
    ("POST", "/items") => (StatusCode::CREATED, axum::Json(json!({ "id": 99 }))),
    _ => (StatusCode::OK, axum::Json(json!({}))),
  }
}

async fn stand(standing: Value) -> (Calls, BTreeMap<String, String>) {
  let calls: Calls = Arc::default();
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let app = axum::Router::new().fallback(jobs).with_state((calls.clone(), standing));

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  let vars = [
    ("BMO_JOB_URL".to_owned(), address),
    ("BMO_JOB_TOKEN".to_owned(), "job-token".to_owned()),
  ]
  .into();

  (calls, vars)
}

#[tokio::test]
async fn an_open_pull_request_is_filed_once_for_the_worker_that_holds_its_repository() {
  let held = BTreeMap::from([("org/base-ui".to_owned(), "base-ui".to_owned())]);
  let (calls, vars) = stand(json!([])).await;

  act(&vars, &held, &Delivered::Open(request())).await.unwrap();

  let made = calls.lock().unwrap().clone();

  assert!(made[0].2.contains("label=pr%3Dhttps"), "{made:?}");
  assert_eq!((made[1].0.as_str(), made[1].1.as_str()), ("POST", "/items"));
  assert_eq!(made[1].3["assigneeName"], "base-ui");

  // Filed already: nothing more.
  let (again, vars) = stand(
    json!([{ "id": 5, "state": "draft", "labels": { "pr": request().url, "lane": PULL_REQUESTS.lane } }]),
  )
  .await;

  act(&vars, &held, &Delivered::Open(request())).await.unwrap();
  assert!(again.lock().unwrap().iter().all(|(method, ..)| method == "GET"));

  // A repository no worker holds is let be.
  let (none, vars) = stand(json!([])).await;

  act(&vars, &BTreeMap::new(), &Delivered::Open(request())).await.unwrap();
  assert!(none.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_delivered_pull_request_from_outside_is_filed_to_wait_for_the_owner_s_approval() {
  let held = BTreeMap::from([("org/base-ui".to_owned(), "base-ui".to_owned())]);
  let forked = PullRequest {
    author: "CONTRIBUTOR".into(),
    head_repository: Some("stranger/base-ui".into()),
    ..request()
  };
  let (calls, vars) = stand(json!([])).await;

  act(&vars, &held, &Delivered::Open(forked.clone())).await.unwrap();

  let filed = made(&calls);

  assert_eq!(filed.len(), 1, "{filed:?}");
  assert_eq!(filed[0].2["labels"]["lane"], PULL_REQUESTS.approval_lane.as_str());
  assert!(filed[0].2["brief"].as_str().unwrap().contains("never merge it"));

  // A review in the approval lane counts as filed: the same opening files nothing more.
  let waiting = json!([{ "id": 5, "state": "draft",
    "labels": { "pr": request().url, "lane": PULL_REQUESTS.approval_lane, "head": "abc123" } }]);
  let (again, vars) = stand(waiting.clone()).await;

  act(&vars, &held, &Delivered::Open(forked.clone())).await.unwrap();
  assert!(made(&again).is_empty(), "{:?}", made(&again));

  // A push on it is told on the waiting item, and nothing is filed.
  let (pushed, vars) = stand(waiting).await;
  let push = PullRequest {
    head: Some("def456".into()),
    ..forked.clone()
  };

  act(&vars, &held, &Delivered::Pushed(push.clone())).await.unwrap();

  let told = made(&pushed);

  assert_eq!(told.len(), 1, "{told:?}");
  assert_eq!(told[0].1, "/items/5/notes");

  // A push after it was sent back files again, still to wait for the owner.
  let (refiled, vars) = stand(json!([{ "id": 5, "state": "done",
    "labels": { "pr": request().url, "lane": PULL_REQUESTS.approval_lane, "head": "abc123" } }]))
  .await;

  act(&vars, &held, &Delivered::Pushed(push)).await.unwrap();

  let filed = made(&refiled);

  assert_eq!(filed.len(), 1, "{filed:?}");
  assert_eq!(filed[0].2["labels"]["lane"], PULL_REQUESTS.approval_lane.as_str());
}

#[tokio::test]
async fn the_fallback_files_by_the_same_rule_of_who_opened_the_pull_request() {
  let held = BTreeMap::from([("org/base-ui".to_owned(), "base-ui".to_owned())]);
  let page = json!({ "data": { "search": { "nodes": [
    {
      "url": "https://github.com/org/base-ui/pull/7",
      "number": 7,
      "title": "Document the theme tokens",
      "authorAssociation": "OWNER",
      "repository": { "nameWithOwner": "org/base-ui" },
      "headRepository": { "nameWithOwner": "org/base-ui" },
    },
    {
      "url": "https://github.com/org/base-ui/pull/9",
      "number": 9,
      "title": "Run a script",
      "authorAssociation": "OWNER",
      "repository": { "nameWithOwner": "org/base-ui" },
      "headRepository": { "nameWithOwner": "stranger/base-ui" },
    },
    {
      "url": "https://github.com/org/base-ui/pull/10",
      "number": 10,
      "title": "Add a dependency",
      "authorAssociation": "COLLABORATOR",
      "repository": { "nameWithOwner": "org/base-ui" },
      "headRepository": { "nameWithOwner": "org/base-ui" },
    },
  ]}}});
  let mut lanes = Vec::new();

  for found in searched(&page) {
    let (calls, vars) = stand(json!([])).await;

    act(&vars, &held, &Delivered::Open(found)).await.unwrap();

    let filed = made(&calls);

    assert_eq!(filed.len(), 1, "{filed:?}");
    lanes.push(filed[0].2["labels"]["lane"].as_str().unwrap().to_owned());
  }

  assert_eq!(
    lanes,
    [PULL_REQUESTS.lane.as_str(), &PULL_REQUESTS.approval_lane, &PULL_REQUESTS.approval_lane]
  );
}

#[tokio::test]
async fn a_merge_opens_the_held_item_that_waited_on_it_and_tells_the_review_item() {
  let standing = json!([
    { "id": 5, "state": "open", "labels": { "pr": request().url, "lane": PULL_REQUESTS.lane } },
    { "id": 6, "state": "held", "labels": { "pr": request().url } },
    { "id": 8, "state": "open", "labels": { "pr": request().url } },
  ]);
  let (calls, vars) = stand(standing).await;

  act(&vars, &BTreeMap::new(), &Delivered::Merged(request())).await.unwrap();

  let made: Vec<(String, String, Value)> = calls
    .lock()
    .unwrap()
    .iter()
    .skip(1)
    .map(|(method, path, _, body)| (method.clone(), path.clone(), body.clone()))
    .collect();

  assert_eq!(made[0].1, "/items/5/notes");
  assert!(made[0].2["text"].as_str().unwrap().contains("merged without this review"));
  assert_eq!(made[1].1, "/items/6/notes");
  assert_eq!((made[2].0.as_str(), made[2].1.as_str()), ("PATCH", "/items/6"));
  assert_eq!(made[2].2, json!({ "state": "open" }));
  assert_eq!(made.len(), 3, "an item that was not held is left as it stands: {made:?}");
}

// A review item of the test's pull request, by its id, state and the head it was filed for.
fn review(id: i64, state: &str, head: Option<&str>) -> Value {
  let mut labels = json!({ "pr": request().url, "lane": PULL_REQUESTS.lane });

  if let Some(head) = head {
    labels["head"] = json!(head);
  }

  json!({ "id": id, "state": state, "labels": labels })
}

// The calls a delivery made past its read of the items: method, path and body.
fn made(calls: &Calls) -> Vec<(String, String, Value)> {
  calls
    .lock()
    .unwrap()
    .iter()
    .filter(|(method, ..)| method != "GET")
    .map(|(method, path, _, body)| (method.clone(), path.clone(), body.clone()))
    .collect()
}

#[tokio::test]
async fn a_push_after_the_review_was_sent_back_files_a_new_review_for_the_new_head() {
  let held = BTreeMap::from([("org/base-ui".to_owned(), "base-ui".to_owned())]);
  let pushed = PullRequest {
    head: Some("def456".into()),
    ..request()
  };
  let (calls, vars) = stand(json!([review(5, "done", None)])).await;

  act(&vars, &held, &Delivered::Pushed(pushed.clone())).await.unwrap();

  let filed = made(&calls);

  assert_eq!(filed.len(), 1, "{filed:?}");
  assert_eq!((filed[0].0.as_str(), filed[0].1.as_str()), ("POST", "/items"));
  assert_eq!(filed[0].2["labels"]["head"], "def456");
  assert_eq!(filed[0].2["assigneeName"], "base-ui");

  // A second delivery of the same push finds that head filed.
  let (again, vars) = stand(json!([review(5, "done", None), review(9, "done", Some("def456"))])).await;

  act(&vars, &held, &Delivered::Pushed(pushed.clone())).await.unwrap();
  assert!(made(&again).is_empty(), "{:?}", made(&again));
}

#[tokio::test]
async fn a_push_on_a_review_still_standing_is_told_on_it_and_one_on_a_dropped_review_files_nothing() {
  let held = BTreeMap::from([("org/base-ui".to_owned(), "base-ui".to_owned())]);
  let pushed = PullRequest {
    head: Some("def456".into()),
    ..request()
  };

  for state in ["draft", "open", "running"] {
    let (calls, vars) = stand(json!([review(4, "done", None), review(5, state, Some("abc123"))])).await;

    act(&vars, &held, &Delivered::Pushed(pushed.clone())).await.unwrap();

    let told = made(&calls);

    assert_eq!(told.len(), 1, "{state}: {told:?}");
    assert_eq!(told[0].1, "/items/5/notes", "{state}");
    assert!(told[0].2["text"].as_str().unwrap().contains("def456"), "{state}");
  }

  let (dropped, vars) = stand(json!([review(5, "done", None), review(6, "dropped", Some("abc123"))])).await;

  act(&vars, &held, &Delivered::Pushed(pushed)).await.unwrap();
  assert!(made(&dropped).is_empty(), "{:?}", made(&dropped));
}

#[tokio::test]
async fn an_opening_files_again_only_for_a_head_no_review_was_filed_for() {
  let held = BTreeMap::from([("org/base-ui".to_owned(), "base-ui".to_owned())]);

  // Reopened on the head its sent-back review was filed for, or on any head where that review names none.
  for filed in [review(5, "done", Some("abc123")), review(5, "done", None)] {
    let (calls, vars) = stand(json!([filed])).await;

    act(&vars, &held, &Delivered::Open(request())).await.unwrap();
    assert!(made(&calls).is_empty(), "{:?}", made(&calls));
  }

  // The fallback names no head: any review filed counts.
  let (fallback, vars) = stand(json!([review(5, "done", Some("old"))])).await;
  let searched = PullRequest {
    head: None,
    ..request()
  };

  act(&vars, &held, &Delivered::Open(searched)).await.unwrap();
  assert!(made(&fallback).is_empty(), "{:?}", made(&fallback));

  // Reopened on a head no review was filed for.
  let (calls, vars) = stand(json!([review(5, "done", Some("old"))])).await;

  act(&vars, &held, &Delivered::Open(request())).await.unwrap();

  let filed = made(&calls);

  assert_eq!(filed.len(), 1, "{filed:?}");
  assert_eq!(filed[0].2["labels"]["head"], "abc123");
}
