//! The warming of an agent's kept session (docs/keepalive-schedule.md, 「An agent's kept session」): what a
//! pass makes of a remembered session, the one warm session an agent, and the session's own row through a
//! ping.

use agency_server::apart_sessions::{self, Answered, Kept, SessionKey};
use agency_server::keepalive::agents::{How, Judged, Warm, forget, judged, remember, remembered};
use agency_server::role_agents::{RoleAgent, composed_as, seated_at, worked_in};
use harness_providers::PlaceCache;

use crate::support::grounds::seeded;
use crate::support::scratch::Scratch;

const MINUTE: i64 = 60_000;
// A provider that keeps a prefix an hour and pings one idle forty minutes.
const CACHE: PlaceCache = PlaceCache {
  ttl_minutes: 60,
  idle_minutes: 40,
};

fn agent(enabled: bool, keeps_cache: bool) -> RoleAgent {
  RoleAgent {
    name: "coder-01".to_owned(),
    role: "coder".to_owned(),
    provider: Some("claude-code".to_owned()),
    model: Some("claude-opus-5".to_owned()),
    effort: None,
    thinking: None,
    answer_tokens: None,
    advisor: None,
    enabled,
    keeps_cache,
    sessions: policy::roles::Sessions::Keep,
    fold_ratio: None,
    prompt: None,
    bundles: None,
    skills: None,
    added_at: 0,
    owner_account: None,
    follows_asker: None,
  }
}

fn kept(answered_at: Option<i64>, held: bool, dropped: bool) -> Kept {
  Kept {
    id: 1,
    role: "coder".to_owned(),
    path: "/checkouts/one".to_owned(),
    provider: "claude-code".to_owned(),
    model: "claude-opus-5".to_owned(),
    effort: "high".to_owned(),
    account: None,
    session: "session-a".to_owned(),
    opened_at: 0,
    answered_at,
    runs: 1,
    context_tokens: None,
    context_share: None,
    held,
    dropped,
  }
}

#[test]
fn a_remembered_session_is_due_once_idle_its_stretch_and_let_go_once_its_cache_has_gone() {
  let warm = agent(true, true);
  let answered = 1_000 * MINUTE;
  let at = |minutes: i64| {
    judged(
      Some(&warm),
      Some(&kept(Some(answered), false, false)),
      Some(CACHE),
      answered + minutes * MINUTE,
    )
  };

  assert_eq!(at(0), Judged::Wait);
  assert_eq!(at(39), Judged::Wait, "inside its idle stretch");
  assert_eq!(at(40), Judged::Due, "idle its stretch, its prefix still there");
  // The lifetime less the pool's margin is what a prefix is counted to.
  let usable = 60 - policy::pool_warm::POOL_WARM.margin_minutes;

  assert_eq!(at(usable - 1), Judged::Due);
  assert!(
    matches!(at(usable), Judged::Forget(_)),
    "a prefix already gone is not rebuilt by a ping"
  );
}

#[test]
fn a_remembered_session_is_let_go_with_its_agent_its_switches_and_its_session() {
  let now = 1_040 * MINUTE;
  let answered = Some(1_000 * MINUTE);
  let fresh = kept(answered, false, false);

  assert!(
    matches!(judged(None, Some(&fresh), Some(CACHE), now), Judged::Forget(_)),
    "its agent is gone"
  );
  assert!(matches!(
    judged(Some(&agent(false, true)), Some(&fresh), Some(CACHE), now),
    Judged::Forget(_)
  ));
  assert!(matches!(
    judged(Some(&agent(true, false)), Some(&fresh), Some(CACHE), now),
    Judged::Forget(_)
  ));
  assert!(matches!(judged(Some(&agent(true, true)), None, Some(CACHE), now), Judged::Forget(_)));
  assert!(matches!(
    judged(Some(&agent(true, true)), Some(&kept(answered, false, true)), Some(CACHE), now),
    Judged::Forget(_)
  ));
  // A provider that publishes no lifetime has no prefix to keep.
  assert!(matches!(judged(Some(&agent(true, true)), Some(&fresh), None, now), Judged::Forget(_)));
  // A session a run or a fold holds is passed over, and kept.
  assert_eq!(
    judged(Some(&agent(true, true)), Some(&kept(answered, true, false)), Some(CACHE), now),
    Judged::Wait
  );
  assert_eq!(judged(Some(&agent(true, true)), Some(&fresh), Some(CACHE), now), Judged::Due);
}

#[test]
fn an_agent_has_one_warm_session_its_last() {
  let warm = |agent: &str, session: &str| Warm {
    agent: agent.to_owned(),
    session: session.to_owned(),
    role: "coder".to_owned(),
    provider: "claude-code".to_owned(),
    model: "claude-opus-5".to_owned(),
    how: How::Worker {
      row: agency_server::places::AgentRow {
        name: "repo-a".to_owned(),
        kind: "worker".to_owned(),
        label: None,
        herdr_label: None,
        node: None,
        cwd: None,
        dev_command: None,
        support_of: None,
        resting: false,
      },
    },
  };
  let of = |agent: &str| remembered().into_iter().find(|held| held.agent == agent).map(|held| held.session);

  remember(warm("warm-test-01", "session-a"));
  remember(warm("warm-test-02", "session-b"));
  remember(warm("warm-test-01", "session-c"));

  assert_eq!(
    of("warm-test-01").as_deref(),
    Some("session-c"),
    "a later session takes the earlier one's place"
  );
  assert_eq!(of("warm-test-02").as_deref(), Some("session-b"));
  assert!(forget("warm-test-01"));
  assert!(!forget("warm-test-01"));
  assert_eq!(of("warm-test-01"), None);
  assert!(forget("warm-test-02"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ping_holds_its_session_and_leaves_it_answered_at_its_own_moment() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let db = &ground.db;
  let key = |path: &str| SessionKey {
    agent: None,
    role: "coder".to_owned(),
    path: path.to_owned(),
    provider: "claude-code".to_owned(),
    model: "claude-opus-5".to_owned(),
    effort: "high".to_owned(),
    account: None,
    command: "key".to_owned(),
  };
  let answered = |at: i64| Answered {
    at,
    opened_at: at,
    context_tokens: None,
    context_share: None,
  };

  apart_sessions::keep(db, &key("/checkouts/one"), "session-a", answered(1_000))
    .await
    .unwrap();

  let row = apart_sessions::kept(db, "session-a").await.unwrap().unwrap();

  assert_eq!((row.answered_at, row.runs, row.held), (Some(1_000), 1, false));
  assert!(apart_sessions::newest(db, &row).await.unwrap());
  assert_eq!(apart_sessions::kept(db, "nobody").await.unwrap(), None);

  // Taken for its ping, no run and no second ping takes it; given back warmed, its last answer is the ping's
  // and its count of runs is as it was.
  assert!(apart_sessions::take_for_ping(db, "session-a").await.unwrap());
  assert!(!apart_sessions::take_for_ping(db, "session-a").await.unwrap());
  assert_eq!(apart_sessions::take(db, &key("/checkouts/one"), 2_000).await.unwrap(), None);
  apart_sessions::warmed(db, "session-a", 5_000).await.unwrap();

  let warmed = apart_sessions::kept(db, "session-a").await.unwrap().unwrap();

  assert_eq!((warmed.answered_at, warmed.runs, warmed.held), (Some(5_000), 1, false));

  // A later run that kept another session at the same key leaves this one behind.
  apart_sessions::keep(db, &key("/checkouts/one"), "session-b", answered(9_000))
    .await
    .unwrap();
  assert!(!apart_sessions::newest(db, &warmed).await.unwrap());
  assert!(
    apart_sessions::newest(db, &apart_sessions::kept(db, "session-b").await.unwrap().unwrap())
      .await
      .unwrap()
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ping_composes_as_its_agent_and_passes_over_a_folder_a_run_works_in() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let db = &ground.db;

  // No run stands anywhere: a composition reads no agent, and one made as an agent reads that agent, of its
  // own role alone.
  assert_eq!(seated_at(db, Some("coder"), Some("/checkouts/one")).await.unwrap(), None);
  assert_eq!(
    composed_as("coder-02".to_owned(), seated_at(db, Some("coder"), Some("/checkouts/one")))
      .await
      .unwrap()
      .map(|agent| agent.name),
    Some("coder-02".to_owned())
  );
  assert_eq!(
    composed_as("coder-02".to_owned(), seated_at(db, None, Some("/checkouts/one")))
      .await
      .unwrap()
      .map(|agent| agent.name),
    Some("coder-02".to_owned())
  );
  assert_eq!(
    composed_as("coder-02".to_owned(), seated_at(db, Some("worker"), None)).await.unwrap(),
    None
  );

  // A folder a run works in is one a slot request stands active at.
  assert!(!worked_in(db, "/checkouts/one").await.unwrap());
  scratch
    .run(
      r#"INSERT INTO "role_slot_requests" ("token", "request_key", "owner", "source", "role", "path", "provider",
           "model", "effort", "capacity", "state", "enqueued_at", "class")
         VALUES ('t-1', 'k-1', 'o-1', 'job', 'worker', '/checkouts/one/', 'deepseek', 'deepseek-flash', '', 3,
           'active', 0, 'foreground')"#,
      vec![],
    )
    .await;
  assert!(worked_in(db, "/checkouts/one").await.unwrap());
  assert!(!worked_in(db, "/checkouts/two").await.unwrap());
  assert!(!worked_in(db, "").await.unwrap());
}
