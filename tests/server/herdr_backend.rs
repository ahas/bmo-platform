//! The herdr backend's session ids: each names its agent and its scope, and reads back as both, but for a
//! thrown-away session's, which only its start numbers; and a failed
//! turn's ending, which carries the engine's error kind where its engine ended it with one.

use agency_server::herdr_backend::{TurnEnded, failure_of, id_of, scope_of};
use agency_server::supervisor_turn::run::NoAnswer;
use agent_session::{Apart, Scope, SessionId, TurnEnd};
use serde_json::json;

#[test]
fn a_session_id_names_its_agent_and_scope_and_reads_back() {
  for scope in [Scope::Agent, Scope::Worker, Scope::Room("-1001".into())] {
    let id = id_of("supervisor-a", &scope).expect("the scope names a session");

    assert_eq!(scope_of(&id), ("supervisor-a".to_owned(), scope));
  }

  assert_eq!(id_of("worker-a", &Scope::Worker), Some(SessionId("worker-a@worker".into())));
  assert_eq!(
    id_of("supervisor-a", &Scope::Room("7".into())),
    Some(SessionId("supervisor-a@7".into()))
  );
}

#[test]
fn no_id_is_named_for_a_thrown_away_session_ahead_of_its_start() {
  let apart = Apart {
    provider: "claude-code".into(),
    model: "claude-opus-5".into(),
    cwd: "/checkouts/a".into(),
    todo: Some(1802),
    role: Some("build".into()),
    ..Apart::default()
  };

  assert_eq!(id_of("branch-a", &Scope::Apart(apart)), None);
}

#[test]
fn a_failed_turn_carries_its_engines_error_kind_and_null_for_any_other_failure() {
  let ended = |no: NoAnswer| {
    let (reason, engine_error) = failure_of(no);

    serde_json::to_value(TurnEnded {
      session: "supervisor-a@-7".into(),
      key: None,
      end: TurnEnd::failed("run-1", reason, engine_error),
    })
    .unwrap()
  };
  let errored = |kind: Option<&str>| NoAnswer::Errored {
    error: "API Error".into(),
    kind: kind.map(str::to_owned),
  };

  assert_eq!(
    ended(errored(Some("rate_limit"))),
    json!({
      "session": "supervisor-a@-7",
      "turn": 0,
      "delivery": "taken",
      "run": "run-1",
      "outcome": "failed",
      "ending": null,
      "answer": null,
      "answers": [],
      "reason": "API Error",
      "engineError": { "kind": "rate_limit" },
      "went_out": null,
      "reply": null,
      "failure": null,
      "spans": [],
      "compacted": false,
      "continues": null,
    })
  );
  assert_eq!(ended(errored(None))["engineError"], json!({ "kind": null }));
  assert_eq!(ended(NoAnswer::Failed("the pane went away".into()))["engineError"], json!(null));
  assert_eq!(ended(NoAnswer::Cut("too many tool calls".into()))["engineError"], json!(null));
}

#[test]
fn a_road_sums_what_each_model_reported_and_none_where_none_reported() {
  use agency_server::door::record::ModelSpend;
  use agency_server::herdr_backend::RoadWalked;

  let spend = |model: &str, input: i64| ModelSpend {
    mode: "claude-code".into(),
    model: model.into(),
    tokens: input + 1,
    input_tokens: input,
    output_tokens: 1,
    cache_read_tokens: 10,
    cache_write_tokens: 0,
    cost: 0.0,
  };
  let started = std::time::Instant::now();
  let answered = RoadWalked::of(
    -5,
    "run-1",
    "claude-code",
    "model-a",
    true,
    started,
    &[spend("a", 3), spend("b", 4)],
    Some(0.5),
  );

  assert_eq!(
    (
      answered.stage.as_str(),
      answered.outcome,
      answered.input_tokens,
      answered.output_tokens,
      answered.cache_read_tokens
    ),
    ("chat", "answered", Some(7), Some(2), Some(20))
  );

  let failed = RoadWalked::of(-5, "run-1", "claude-code", "model-a", false, started, &[], None);

  assert_eq!((failed.outcome, failed.input_tokens, failed.context_ratio), ("error", None, None));
}

#[tokio::test]
async fn an_agents_session_across_rooms_is_kept_on_its_row_and_let_go() {
  use agency_server::herdr_backend::{agent_session, keep_agent_session};

  let scratch = crate::support::scratch::Scratch::new().await;

  crate::support::rows::supervisor(scratch.db(), "chat-agent", &[]).await;

  assert_eq!(agent_session(scratch.db(), "chat-agent").await.unwrap(), None);

  keep_agent_session(scratch.db(), "chat-agent", Some(("s-1", -5))).await.unwrap();

  assert_eq!(agent_session(scratch.db(), "chat-agent").await.unwrap(), Some("s-1".into()));

  keep_agent_session(scratch.db(), "chat-agent", None).await.unwrap();

  assert_eq!(agent_session(scratch.db(), "chat-agent").await.unwrap(), None);
  assert_eq!(
    scratch
      .count(r#"SELECT "chat_id" FROM "supervisors" WHERE "name" = 'chat-agent'"#, vec![])
      .await,
    -5,
    "the room its last turn answered stays"
  );
}
