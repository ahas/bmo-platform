//! The list of runs (docs/api.md, 「runs」): the newest first, paged by a run's id before or after it with
//! `more` saying whether a page lies beyond, narrowed by each filter alone and by every one named at once, and a
//! query naming no filter of its, or a value that does not read, refused.

use agency_server::run_ledger::{self, Opening, Page, Search};
use serde_json::Value;

use crate::support::scratch::Scratch;
use crate::support::standing;

// Six runs: two agents, two rooms, two rounds, the even ones closed. Answers their ids, oldest first.
async fn seeded(scratch: &Scratch) -> Vec<i64> {
  let mut ids = vec![];

  for at in 0..6i64 {
    let id = run_ledger::open(
      scratch.db(),
      &Opening {
        agent: if at < 3 { "agent-a".into() } else { "agent-b".into() },
        chat_id: Some(if at % 3 == 0 { -5 } else { -6 }),
        work_unit: format!("unit-{at}"),
        depth: 0,
        cap_ms: None,
        started_at: 1_000 + at,
        started_by: None,
        search_id: None,
        round: Some(format!("round:{}", at % 2)),
        message_id: None,
        title: None,
        carried: false,
      },
    )
    .await
    .unwrap();

    ids.push(id);
  }

  for id in ids.iter().step_by(2) {
    scratch
      .run(
        r#"UPDATE "runs" SET "finished_at" = 2000, "outcome" = 'answered' WHERE "id" = $1"#,
        vec![(*id).into()],
      )
      .await;
  }

  ids
}

async fn listed(scratch: &Scratch, search: Search) -> Page {
  run_ledger::search(
    scratch.db(),
    &Search {
      limit: if search.limit == 0 { 50 } else { search.limit },
      ..search
    },
  )
  .await
  .unwrap()
}

fn ids_of(page: &Page) -> Vec<i64> {
  page.runs.iter().map(|run| run.id).collect()
}

// The ids named, newest first.
fn newest(ids: &[i64], picks: &[usize]) -> Vec<i64> {
  let mut picked: Vec<i64> = picks.iter().map(|at| ids[*at]).collect();

  picked.sort_unstable_by(|one, other| other.cmp(one));
  picked
}

#[tokio::test]
async fn the_newest_runs_come_first_and_limit_bounds_the_page_with_more_saying_older_ones_lie_beyond() {
  let scratch = Scratch::new().await;
  let ids = seeded(&scratch).await;
  let all = listed(&scratch, Search::default()).await;

  assert_eq!(ids_of(&all), newest(&ids, &[0, 1, 2, 3, 4, 5]));
  assert!(!all.more);

  let two = listed(
    &scratch,
    Search {
      limit: 2,
      ..Search::default()
    },
  )
  .await;

  assert_eq!(ids_of(&two), newest(&ids, &[4, 5]));
  assert!(two.more);
}

#[tokio::test]
async fn before_pages_back_to_older_runs() {
  let scratch = Scratch::new().await;
  let ids = seeded(&scratch).await;
  let page = listed(
    &scratch,
    Search {
      before: Some(ids[4]),
      limit: 2,
      ..Search::default()
    },
  )
  .await;

  assert_eq!(ids_of(&page), newest(&ids, &[2, 3]));
  assert!(page.more, "two older runs lie beyond");

  let last = listed(
    &scratch,
    Search {
      before: Some(ids[2]),
      limit: 2,
      ..Search::default()
    },
  )
  .await;

  assert_eq!(ids_of(&last), newest(&ids, &[0, 1]));
  assert!(!last.more);
}

#[tokio::test]
async fn after_answers_the_runs_nearest_after_it_newest_first() {
  let scratch = Scratch::new().await;
  let ids = seeded(&scratch).await;
  let page = listed(
    &scratch,
    Search {
      after: Some(ids[1]),
      limit: 2,
      ..Search::default()
    },
  )
  .await;

  assert_eq!(ids_of(&page), newest(&ids, &[2, 3]));
  assert!(page.more, "newer runs lie beyond");

  let both = listed(
    &scratch,
    Search {
      after: Some(ids[1]),
      before: Some(ids[3]),
      ..Search::default()
    },
  )
  .await;

  assert_eq!(ids_of(&both), vec![ids[2]]);
}

#[tokio::test]
async fn live_narrows_to_the_open_runs_or_the_closed_ones() {
  let scratch = Scratch::new().await;
  let ids = seeded(&scratch).await;
  let open = listed(
    &scratch,
    Search {
      live: Some(true),
      ..Search::default()
    },
  )
  .await;
  let closed = listed(
    &scratch,
    Search {
      live: Some(false),
      ..Search::default()
    },
  )
  .await;

  assert_eq!(ids_of(&open), newest(&ids, &[1, 3, 5]));
  assert_eq!(ids_of(&closed), newest(&ids, &[0, 2, 4]));
}

#[tokio::test]
async fn agent_name_narrows_to_one_agent_s_runs() {
  let scratch = Scratch::new().await;
  let ids = seeded(&scratch).await;
  let page = listed(
    &scratch,
    Search {
      agent: Some("agent-b".into()),
      ..Search::default()
    },
  )
  .await;

  assert_eq!(ids_of(&page), newest(&ids, &[3, 4, 5]));
}

#[tokio::test]
async fn room_id_narrows_to_one_room_s_runs() {
  let scratch = Scratch::new().await;
  let ids = seeded(&scratch).await;
  let page = listed(
    &scratch,
    Search {
      room: Some(-5),
      ..Search::default()
    },
  )
  .await;

  assert_eq!(ids_of(&page), newest(&ids, &[0, 3]));
}

#[tokio::test]
async fn round_narrows_to_one_round_s_runs() {
  let scratch = Scratch::new().await;
  let ids = seeded(&scratch).await;
  let page = listed(
    &scratch,
    Search {
      round: Some("round:1".into()),
      ..Search::default()
    },
  )
  .await;

  assert_eq!(ids_of(&page), newest(&ids, &[1, 3, 5]));
}

#[tokio::test]
async fn names_match_a_run_s_unit_or_its_round() {
  let scratch = Scratch::new().await;
  let ids = seeded(&scratch).await;
  let page = listed(
    &scratch,
    Search {
      names: vec!["unit-2".into(), "round:1".into()],
      ..Search::default()
    },
  )
  .await;

  assert_eq!(ids_of(&page), newest(&ids, &[1, 2, 3, 5]));
}

#[tokio::test]
async fn every_filter_named_holds_at_once() {
  let scratch = Scratch::new().await;
  let ids = seeded(&scratch).await;
  let page = listed(
    &scratch,
    Search {
      live: Some(true),
      agent: Some("agent-b".into()),
      room: Some(-6),
      round: Some("round:1".into()),
      ..Search::default()
    },
  )
  .await;

  assert_eq!(ids_of(&page), vec![ids[5]]);
}

async fn get(port: u16, token: &str, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

fn listed_ids(page: &Value) -> Vec<i64> {
  page["items"]
    .as_array()
    .unwrap()
    .iter()
    .map(|run| run["id"].as_i64().unwrap())
    .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_route_reads_every_filter_off_its_query_and_refuses_one_it_does_not_take() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let ids = seeded(&scratch).await;
  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;

  let (status, page) = get(port, token, "/runs?limit=2").await;

  assert_eq!(status, 200, "{page}");
  assert_eq!(listed_ids(&page), newest(&ids, &[4, 5]));
  assert_eq!(page["more"], true);

  let path = format!("/runs?before={}&limit=2", ids[4]);

  assert_eq!(listed_ids(&get(port, token, &path).await.1), newest(&ids, &[2, 3]));

  let path = format!("/runs?after={}&limit=2", ids[1]);

  assert_eq!(listed_ids(&get(port, token, &path).await.1), newest(&ids, &[2, 3]));
  assert_eq!(listed_ids(&get(port, token, "/runs?live=true").await.1), newest(&ids, &[1, 3, 5]));
  assert_eq!(listed_ids(&get(port, token, "/runs?live=false").await.1), newest(&ids, &[0, 2, 4]));
  assert_eq!(
    listed_ids(&get(port, token, "/runs?agentName=agent-b").await.1),
    newest(&ids, &[3, 4, 5])
  );
  assert_eq!(listed_ids(&get(port, token, "/runs?roomId=-5").await.1), newest(&ids, &[0, 3]));
  assert_eq!(listed_ids(&get(port, token, "/runs?round=round:1").await.1), newest(&ids, &[1, 3, 5]));
  assert_eq!(
    listed_ids(&get(port, token, "/runs?names=unit-2,round:1").await.1),
    newest(&ids, &[1, 2, 3, 5])
  );

  for (path, field) in [
    ("/runs?model=m", "model"),
    ("/runs?workerName=w", "workerName"),
    ("/runs?live=maybe", "live"),
    ("/runs?roomId=five", "roomId"),
  ] {
    let (status, refused) = get(port, token, path).await;

    assert_eq!(status, 400, "{path}: {refused}");
    assert!(refused["error"].as_str().unwrap_or_default().contains(field), "{path}: {refused}");
  }
}
