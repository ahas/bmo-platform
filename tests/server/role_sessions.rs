//! The sessions roles keep (docs/sessions.md, 「A role's session」): a session is kept by the role, the path,
//! the provider and the model together with the command's key, resumed by the next run of the same key while
//! its cache is warm and its window not full, held by one run at a time, and opened afresh otherwise.

use agency_server::apart_sessions::{
  Answered, Kept, SessionKey, fold_due, folded, give_back, keep, take, take_for_fold, valid,
};
use agency_server::now_ms;

use crate::support::scratch::Scratch;

fn key(role: &str, path: &str, model: &str) -> SessionKey {
  SessionKey {
    agent: None,
    role: role.into(),
    path: path.into(),
    provider: "claude-code".into(),
    model: model.into(),
    effort: "high".into(),
    account: Some("default".into()),
    command: "the command".into(),
  }
}

fn answered_at(at: i64) -> Answered {
  Answered {
    at,
    opened_at: at,
    ..Answered::default()
  }
}

#[tokio::test]
async fn a_kept_session_is_resumed_by_the_next_run_of_its_role_path_provider_and_model() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let now = now_ms();
  let held = key("worker", "/checkouts/one", "claude-opus-5");

  keep(db, &held, "session-one", answered_at(now)).await.unwrap();

  assert_eq!(take(db, &held, now).await.unwrap().as_deref(), Some("session-one"));
  assert_eq!(take(db, &held, now).await.unwrap(), None, "one run holds a kept session at a time");

  give_back(db, "session-one").await.unwrap();

  for other in [
    key("builder", "/checkouts/one", "claude-opus-5"),
    key("worker", "/checkouts/two", "claude-opus-5"),
    key("worker", "/checkouts/one", "claude-sonnet-5"),
  ] {
    assert_eq!(
      take(db, &other, now).await.unwrap(),
      None,
      "another role, path or model keeps its own: {other:?}"
    );
  }

  let mut other_command = held.clone();

  other_command.command = "another command".into();
  assert_eq!(take(db, &other_command, now).await.unwrap(), None, "another command keeps its own");
  assert_eq!(take(db, &held, now).await.unwrap().as_deref(), Some("session-one"));
}

// A session is its agent's: another agent of the same role on the same road at the same folder opens its own, and
// the agent's next run there takes up its own (docs/sessions.md, 「A role's session」).
#[tokio::test]
async fn a_kept_session_is_its_agent_s_and_no_other_agent_resumes_it() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let now = now_ms();
  let of = |agent: &str| SessionKey {
    agent: Some(agent.into()),
    ..key("coder", "/checkouts/one", "deepseek-flash")
  };

  keep(db, &of("coder-05"), "session-five", answered_at(now)).await.unwrap();

  assert_eq!(take(db, &of("coder-04"), now).await.unwrap(), None, "another agent keeps its own");
  assert_eq!(take(db, &key("coder", "/checkouts/one", "deepseek-flash"), now).await.unwrap(), None);
  assert_eq!(take(db, &of("coder-05"), now).await.unwrap().as_deref(), Some("session-five"));
}

#[tokio::test]
async fn a_session_past_its_cache_lifetime_or_its_window_is_not_resumed_and_is_dropped() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let now = now_ms();
  let cold = key("worker", "/checkouts/cold", "claude-opus-5");
  let full = key("worker", "/checkouts/full", "claude-opus-5");

  keep(db, &cold, "session-cold", answered_at(now - 2 * 3_600_000)).await.unwrap();
  keep(
    db,
    &full,
    "session-full",
    Answered {
      context_share: Some(1.0),
      ..answered_at(now)
    },
  )
  .await
  .unwrap();

  assert_eq!(take(db, &cold, now).await.unwrap(), None, "a cache past its lifetime is not resumed");
  assert_eq!(
    take(db, &full, now).await.unwrap(),
    None,
    "a session that filled its window is not resumed"
  );
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "role_sessions" WHERE "dropped" = 1"#, vec![])
      .await,
    2,
    "each is dropped as it is taken"
  );
}

// A provider that declares no cache lifetime — a registered model on this machine — leaves its kept session
// nothing to outlive, so the next run of its key resumes it however long the wait before it, a job's wait for
// its road included, while a provider that declares a lifetime stops being resumed past it
// (docs/sessions.md, 「The warm/cold criterion」).
#[tokio::test]
async fn a_registered_road_s_kept_session_is_resumed_however_long_the_wait() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let now = now_ms();
  let local = SessionKey {
    provider: "registered".into(),
    model: "a-registration".into(),
    account: None,
    ..key("worker", "/checkouts/local", "a-registration")
  };
  let claude = key("worker", "/checkouts/one", "claude-opus-5");

  keep(db, &local, "session-local", answered_at(now - 24 * 3_600_000)).await.unwrap();
  keep(db, &claude, "session-claude", answered_at(now - 2 * 3_600_000)).await.unwrap();

  let local_row =
    store::one(db, r#"SELECT * FROM "role_sessions" WHERE "session" = 'session-local'"#, vec![])
      .await
      .unwrap()
      .unwrap();
  let claude_row =
    store::one(db, r#"SELECT * FROM "role_sessions" WHERE "session" = 'session-claude'"#, vec![])
      .await
      .unwrap()
      .unwrap();

  assert!(
    valid(&Kept::of(&local_row).unwrap(), now),
    "a local model's session has nothing to outlive"
  );
  assert!(
    !valid(&Kept::of(&claude_row).unwrap(), now),
    "a declared lifetime still ends the session"
  );
  assert_eq!(take(db, &local, now).await.unwrap(), Some("session-local".to_owned()));
}

#[tokio::test]
async fn a_resumed_session_counts_its_runs_and_stays_one_row_of_the_roles_history() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let now = now_ms();
  let held = key("worker", "/checkouts/one", "claude-opus-5");

  keep(db, &held, "session-one", answered_at(now - 1_000)).await.unwrap();
  take(db, &held, now).await.unwrap();
  keep(db, &held, "session-one", answered_at(now)).await.unwrap();

  let row = store::one(db, r#"SELECT * FROM "role_sessions" WHERE "session" = 'session-one'"#, vec![])
    .await
    .unwrap()
    .unwrap();
  let kept = agency_server::apart_sessions::Kept::of(&row).unwrap();

  assert_eq!(kept.runs, 2);
  assert!(!kept.held);
  assert!(valid(&kept, now), "the next run of its key would resume it");
  assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "role_sessions""#, vec![]).await, 1);
}

#[tokio::test]
async fn a_kept_session_at_its_roles_compaction_threshold_is_taken_for_its_fold_and_kept_valid_after_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = crate::support::grounds::seeded(scratch.db(), home.path()).await;
  let db = &ground.db;
  let now = now_ms();
  let held = key("worker", "/checkouts/folding", "claude-opus-5");

  store::run(db, r#"UPDATE "roles" SET "fold_ratio" = 0.5 WHERE "name" = 'worker'"#, vec![])
    .await
    .unwrap();

  assert!(fold_due(Some(0.5), 0.5) && fold_due(Some(0.9), 0.5));
  assert!(
    !fold_due(Some(0.49), 0.5) && !fold_due(None, 0.5),
    "a turn under the threshold, or one reporting no share, is not due"
  );

  keep(
    db,
    &held,
    "session-low",
    Answered {
      context_share: Some(0.3),
      ..answered_at(now)
    },
  )
  .await
  .unwrap();
  assert_eq!(take_for_fold(db, "worker", "session-low", Some(0.3)).await.unwrap(), None);

  let full = key("worker", "/checkouts/full-fold", "claude-opus-5");

  keep(
    db,
    &full,
    "session-full",
    Answered {
      context_tokens: Some(900_000),
      context_share: Some(0.9),
      ..answered_at(now)
    },
  )
  .await
  .unwrap();

  assert_eq!(
    take_for_fold(db, "worker", "session-full", Some(0.9)).await.unwrap().as_deref(),
    Some("session-full"),
    "the role's own threshold makes it due"
  );
  assert_eq!(take(db, &full, now).await.unwrap(), None, "no run resumes a session while it folds");

  folded(db, "session-full", now + 1).await.unwrap();

  let row = store::one(db, r#"SELECT * FROM "role_sessions" WHERE "session" = 'session-full'"#, vec![])
    .await
    .unwrap()
    .unwrap();
  let kept = Kept::of(&row).unwrap();

  assert_eq!((kept.context_tokens, kept.context_share, kept.answered_at), (None, None, Some(now + 1)));
  assert!(valid(&kept, now + 2), "a folded session stays valid for the next run");
  assert_eq!(take(db, &full, now + 2).await.unwrap().as_deref(), Some("session-full"));
  assert_eq!(take_for_fold(db, "no-such-role", "session-full", Some(0.9)).await.unwrap(), None);
}
