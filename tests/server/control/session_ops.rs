//! A turn's context as the turn route reads it from a body: every part optional, the room's block whole,
//! the road and the persona trimmed and none where blank, a work turn's parts whole by the names a job's
//! runner sends, and a body that does not read refused. A start's scope, a question's answer, a session's
//! state and a turn's end as the routes read and write them. A turn's work carried to its end though its
//! caller lets go. The interrupt's answer, which says stopped only of a turn that ended.

use std::collections::BTreeMap;

use agency_server::control::session_ops::{
  answer_read, carried, context_read, ended_within, interrupted, scope_read, scope_value, state_value,
  work_asked, work_of,
};
use agent_session::{
  AccountFailure, Answer, Apart, Asks, Context, Conversation, Failure, GateAsk, JobMark, Mapping, Outcome,
  Question, Reply, Resume, RoomBlock, RunEnding, Scope, SessionId, Span, Spend, State, TurnContext,
  TurnEnd, WentOut, WorkParts,
};
use serde_json::json;

use crate::support::scratch::Scratch;

#[test]
fn a_gates_one_turn_reads_its_system_prompt_thinking_and_mapping() {
  assert_eq!(
    context_read(&json!({
      "gate": {
        "system": "  the gate's instructions  ",
        "thinking": false,
        "model": "a-model",
        "effort": " low ",
        "answerTokens": 2048,
      }
    }))
    .unwrap()
    .gate,
    Some(GateAsk {
      system: Some("the gate's instructions".into()),
      thinking: Some(false),
      model: Some("a-model".into()),
      effort: Some("low".into()),
      answer_tokens: Some(2048),
    })
  );
}

#[test]
fn a_turns_context_reads_every_part_it_names_and_none_of_what_it_does_not() {
  assert_eq!(context_read(&json!({ "session": "a", "prompt": "p" })).unwrap(), TurnContext::default());
  assert_eq!(
    context_read(&json!({
      "room": { "chatId": -5, "text": "the room's block", "mark": "[a]" },
      "via": " console ",
      "basePersona": "  ",
      "privileged": true,
      "answers": [7, 8],
      "began": 1_000,
      "round": " round:-5:1000:1 ",
    }))
    .unwrap(),
    TurnContext {
      key: None,
      room: Some(RoomBlock {
        chat_id: -5,
        text: "the room's block".into(),
        mark: "[a]".into()
      }),
      via: Some("console".into()),
      base_persona: None,
      privileged: true,
      answers: vec![7, 8],
      began: Some(1_000),
      round: Some("round:-5:1000:1".into()),
      job: None,
      work: None,
      gate: None,
    }
  );
  assert!(context_read(&json!({ "room": { "text": "no room" } })).is_err());
  assert_eq!(
    context_read(&json!({ "job": 41, "item": 1802 })).unwrap().job,
    Some(JobMark {
      job: 41,
      item: Some(1802)
    })
  );
  assert_eq!(
    context_read(&json!({ "item": 1802 })).unwrap().job,
    None,
    "an item with no job names none"
  );

  assert_eq!(
    context_read(&json!({ "run": "a-run", "agent": "supervisor", "chatId": -7, "desk": "desk-1" }))
      .unwrap(),
    TurnContext::default(),
    "a turn carries no run parts"
  );
  assert!(context_read(&json!({ "answers": "7" })).is_err());
}

// A role's session's turn as its caller sends it, every part of its own named, beside a folder the turn does
// not read.
fn work_body() -> serde_json::Value {
  json!({
    "prompt": "p",
    "cwd": "/elsewhere",
    "resume": { "kind": "session", "id": "harness-1" },
    "name": "job-41",
    "title": "#1802 build",
    "job": 41,
    "item": 1802,
    "chat": -5,
    "unit": "bmo-job-41",
    "user": "7",
    "hearsWords": true,
    "variables": { "BMO_EXTRA": "1" },
  })
}

fn session_on_slot() -> Apart {
  Apart {
    provider: "claude-code".into(),
    model: "opus-5".into(),
    effort: Some("high".into()),
    cwd: "/checkouts/a".into(),
    role: Some("worker".into()),
    ..Apart::default()
  }
}

#[tokio::test]
async fn a_role_sessions_turn_goes_out_on_the_sessions_road_with_the_parts_it_names_of_its_own() {
  let scratch = Scratch::new().await;
  let asked = work_asked(&work_body()).unwrap();
  let work = work_of(scratch.db(), "worker", &session_on_slot(), asked).await;

  assert_eq!(
    context_read(&work_body()).unwrap().work,
    None,
    "the context carries no work parts of its own"
  );
  assert_eq!(
    work,
    WorkParts {
      provider: "claude-code".into(),
      model: "opus-5".into(),
      mapping: Mapping {
        model: "opus-5".into(),
        effort: Some("high".into()),
        tier: None,
        answer_tokens: work.mapping.answer_tokens,
      },
      resume: Resume::Session {
        id: "harness-1".into()
      },
      cwd: "/checkouts/a".into(),
      name: "job-41".into(),
      title: "#1802 build".into(),
      chat: -5,
      unit: Some("bmo-job-41".into()),
      user: Some("7".into()),
      work_key: "worker".into(),
      hears_words: true,
      variables: BTreeMap::from([("BMO_EXTRA".into(), "1".into())]),
    },
    "the road and the folder are the session's, whatever the body names"
  );

  // A turn naming a road field is refused: its road is its session's.
  for field in ["provider", "model", "mapping", "workKey", "tier"] {
    let mut body = work_body();

    body[field] = json!("another");
    assert!(work_asked(&body).is_err(), "a turn naming {field} is refused");
  }

  let least = work_of(
    scratch.db(),
    "worker",
    &session_on_slot(),
    work_asked(&json!({ "prompt": "p" })).unwrap(),
  )
  .await;

  assert_eq!(least.resume, Resume::Auto);
  assert_eq!(
    (least.title.as_str(), least.chat, least.hears_words),
    ("worker", 0, true),
    "a run stands to be spoken to where its caller names no hearing"
  );
  assert!(least.name.starts_with("worker-") && least.variables.is_empty() && least.unit.is_none());

  assert!(
    work_asked(&json!({ "resume": { "kind": "latest" } })).is_err(),
    "a resume of no known kind is refused"
  );
  assert!(work_asked(&json!({ "hears_words": true })).unwrap().hears_words == Some(true));
}

#[test]
fn a_thrown_away_scope_reads_its_provider_model_and_folder_and_answers_as_it_read() {
  let scope = scope_read(&json!({
    "scope": { "kind": "role", "provider": "claude-code", "model": "sonnet", "cwd": "/places/p/01",
      "todo": 1816, "role": "scout" }
  }))
  .unwrap();
  let apart = Scope::Apart(Apart {
    provider: "claude-code".into(),
    model: "sonnet".into(),
    cwd: "/places/p/01".into(),
    todo: Some(1816),
    role: Some("scout".into()),
    ..Apart::default()
  });

  assert_eq!(scope, apart);
  assert_eq!(
    scope_value(&apart),
    json!({ "kind": "role", "provider": "claude-code", "model": "sonnet", "cwd": "/places/p/01",
      "todo": 1816, "role": "scout" })
  );
  assert_eq!(
    scope_read(&json!({ "scope": { "kind": "role", "provider": "claude-code", "model": "sonnet",
      "cwd": "/tmp/run", "todo": null } }))
    .unwrap(),
    Scope::Apart(Apart {
      provider: "claude-code".into(),
      model: "sonnet".into(),
      cwd: "/tmp/run".into(),
      todo: None,
      role: None,
      ..Apart::default()
    })
  );

  assert_eq!(
    scope_read(&json!({ "scope": { "kind": "role", "role": "scout" } })).unwrap(),
    Scope::Apart(Apart {
      role: Some("scout".into()),
      ..Apart::default()
    }),
    "a role's road and folder are its slot's, laid on it as the start is answered"
  );
  assert!(
    scope_read(&json!({ "scope": { "kind": "worker" } })).is_err(),
    "there is no worker scope"
  );

  assert!(
    scope_read(
      &json!({ "scope": { "kind": "role", "provider": "p", "model": "m", "cwd": "/c", "todo": "x" } })
    )
    .is_err()
  );
}

#[test]
fn a_conversations_scope_reads_its_provider_and_model_and_answers_as_it_read() {
  let conversation = Conversation {
    provider: "openrouter".into(),
    model: "deepseek/deepseek-chat".into(),
    cwd: "/places/c/01".into(),
  };
  let scope = scope_read(&json!({
    "scope": { "kind": "conversation", "provider": "openrouter",
      "model": "deepseek/deepseek-chat", "cwd": "/places/c/01" }
  }))
  .unwrap();

  assert_eq!(scope, Scope::Conversation(conversation.clone()));
  assert_eq!(
    scope_value(&scope),
    json!({ "kind": "conversation", "provider": "openrouter",
      "model": "deepseek/deepseek-chat", "cwd": "/places/c/01" })
  );
  assert_eq!(
    scope_read(&json!({ "scope": { "kind": "conversation", "provider": "codex", "model": "gpt-5" } }))
      .unwrap(),
    Scope::Conversation(Conversation {
      provider: "codex".into(),
      model: "gpt-5".into(),
      cwd: String::new(),
    }),
    "a conversation with no folder named takes the deployment's own"
  );

  assert_eq!(
    scope_read(&json!({ "scope": { "kind": "conversation", "model": "m" } })).unwrap(),
    Scope::Conversation(Conversation {
      provider: "claude-code".into(),
      model: "m".into(),
      cwd: String::new(),
    }),
    "a conversation naming no provider is Claude Code's"
  );
  assert!(
    scope_read(&json!({ "scope": { "kind": "conversation", "provider": "p" } })).is_err(),
    "a conversation with no model is refused"
  );
}

#[test]
fn a_questions_answer_reads_as_allow_leave_or_a_refusal_with_its_words() {
  let read = |given: serde_json::Value| answer_read(&json!({ "answer": given }));

  assert_eq!(read(json!({ "kind": "allow" })).unwrap(), Answer::Allow);
  assert_eq!(read(json!({ "kind": "leave" })).unwrap(), Answer::Leave);
  assert_eq!(
    read(json!({ "kind": "refuse", "words": "write it as a script" })).unwrap(),
    Answer::Refuse {
      words: Some("write it as a script".into())
    }
  );
  assert_eq!(read(json!({ "kind": "refuse" })).unwrap(), Answer::Refuse { words: None });
  assert!(read(json!({ "kind": "close" })).is_err());
  assert!(answer_read(&json!({})).is_err());
}

#[test]
fn a_sessions_state_names_its_turn_its_question_and_what_it_would_continue() {
  let mut state = State::standing(
    SessionId("worker-a@worker".into()),
    "claude-code".into(),
    "opus".into(),
    "/places/worker-a".into(),
    3,
    Some(Context {
      carried: 40_000,
      window: 200_000,
    }),
  );

  assert_eq!(
    state_value(&state),
    json!({
      "session": "worker-a@worker", "provider": "claude-code", "model": "opus", "place": "/places/worker-a",
      "generation": 3, "context": { "carried": 40_000, "window": 200_000 },
      "run": null, "went_out": null, "question": null, "continues": null,
    })
  );

  state.run = Some("job-41".into());
  state.went_out = Some(WentOut {
    provider: "claude-code".into(),
    model: "opus-5".into(),
    effort: Some("high".into()),
    tier: None,
  });
  state.question = Some(Question {
    id: "q-1".into(),
    asks: Asks::Permission {
      text: "rm -f $S/*.md".into(),
      amendable: true,
    },
  });
  state.continues = Some("harness-1".into());

  let value = state_value(&state);

  assert_eq!(value["run"], "job-41");
  assert_eq!(
    value["went_out"],
    json!({ "provider": "claude-code", "model": "opus-5", "effort": "high", "tier": null })
  );
  assert_eq!(
    value["question"],
    json!({ "id": "q-1", "kind": "permission", "text": "rm -f $S/*.md", "amendable": true })
  );
  assert_eq!(value["continues"], "harness-1");
  assert_eq!(
    json!(Question {
      id: "q-2".into(),
      asks: Asks::Unknown {
        screen: "Pick one".into()
      }
    }),
    json!({ "id": "q-2", "kind": "unknown", "screen": "Pick one" })
  );
}

#[test]
fn a_turns_end_writes_its_reply_in_camel_case_and_the_rest_in_snake_case() {
  let answered = TurnEnd {
    went_out: Some(WentOut {
      provider: "claude-code".into(),
      model: "opus-5".into(),
      effort: Some("high".into()),
      tier: Some("frontier".into()),
    }),
    reply: Some(Box::new(Reply {
      mode: "claude-code".into(),
      model: Some("opus-5".into()),
      effort: Some("high".into()),
      text: "done".into(),
      session_id: Some("harness-1".into()),
      account: Some("acct-a".into()),
      context_ratio: Some(0.25),
      context_tokens: Some(50_000),
      session_usage: Some(json!({ "five_hour": 10 })),
      spend: vec![Spend {
        mode: Some("claude-code".into()),
        model: "opus-5".into(),
        tokens: 10,
        input_tokens: 4,
        output_tokens: 3,
        cache_read_tokens: 2,
        cache_write_tokens: 1,
        cost: Some(0.5),
      }],
      live_tasks: vec![],
      workflows: vec![],
      upstream: None,
      request_usages: None,
    })),
    spans: vec![Span {
      kind: "tool".into(),
      name: "Bash".into(),
      seconds: 1.5,
      times: 2,
      tokens: None,
      input_tokens: Some(4),
      output_tokens: None,
      cache_read_tokens: None,
      cache_write_tokens: None,
    }],
    compacted: true,
    ..TurnEnd::answered("job-41", "done")
  };
  let value = json!(answered);

  assert_eq!((value["outcome"].as_str(), value["answer"].as_str()), (Some("answered"), Some("done")));
  assert_eq!(value["reply"]["sessionId"], "harness-1");
  assert_eq!(value["reply"]["contextTokens"], 50_000);
  assert_eq!(value["reply"]["spend"][0]["cacheReadTokens"], 2);
  assert_eq!(value["went_out"]["tier"], "frontier");
  assert_eq!(value["spans"][0]["input_tokens"], 4);
  assert_eq!(value["compacted"], true);

  let failure = |failure: Failure| {
    json!(TurnEnd {
      failure: Some(failure),
      ..TurnEnd::of("r", Outcome::Failed)
    })["failure"]
      .clone()
  };

  assert_eq!(
    failure(Failure::Limit {
      resets_at: Some(1_700),
      account: Some("acct-a".into()),
      silenced: false,
      model: None,
    }),
    json!({ "kind": "limit", "resets_at": 1_700, "account": "acct-a", "silenced": false })
  );
  assert_eq!(
    failure(Failure::Account {
      failure: AccountFailure::NodeToken,
      detail: "gone".into()
    }),
    json!({ "kind": "account", "failure": "node_token", "detail": "gone" })
  );
  assert_eq!(
    failure(Failure::Died {
      ending: RunEnding::Exit { code: 137 },
      said: None,
      message: "killed".into()
    }),
    json!({ "kind": "died", "ending": { "kind": "exit", "code": 137 }, "said": null, "message": "killed" })
  );
  assert_eq!(failure(Failure::Person), json!({ "kind": "person" }));

  let reprepared = TurnEnd {
    continues: Some("harness-2".into()),
    ..TurnEnd::of("job-41", Outcome::Reprepared)
  };

  assert_eq!(
    (json!(reprepared)["outcome"].as_str(), json!(reprepared)["continues"].as_str()),
    (Some("reprepared"), Some("harness-2"))
  );
  assert_eq!(json!(TurnEnd::of("r", Outcome::Blocked))["delivery"], "blocked");
}

#[tokio::test]
async fn work_a_caller_lets_go_of_is_carried_to_its_end() {
  let (done, ended) = tokio::sync::oneshot::channel();
  let work = async move {
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let _ = done.send("ended");
    "answered"
  };

  // The caller waits a moment and lets go, as a sender whose connection drops does.
  let waited = tokio::time::timeout(std::time::Duration::from_millis(20), carried(work)).await;

  assert!(waited.is_err());
  assert_eq!(
    tokio::time::timeout(std::time::Duration::from_secs(5), ended).await.unwrap().unwrap(),
    "ended"
  );
  assert_eq!(carried(async { "answered" }).await, "answered");
}

// A role's start laid on the slot its caller was let in on: the slot's role, road, effort and path, a field
// contradicting it refused, a slot not let in refused, and a start naming no slot refused whatever road it names.
#[tokio::test(flavor = "multi_thread")]
async fn a_roles_start_runs_on_the_slot_it_names() {
  use agency_server::control::session_ops::on_slot;
  use agency_server::order_run::dispatch::{self, Asked, Wanted};

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = crate::support::grounds::seeded(scratch.db(), home.path()).await;

  crate::support::grounds::deepseek_balance(&ground, 5.0).await;

  let ask = |key: &str| Asked {
    request_key: format!("test:{key}"),
    owner: format!("owner:{key}"),
    wanted: Wanted {
      role: "worker",
      path: Some("/checkouts/slot"),
      fixed: None,
      agent: None,
    },
    ..Asked::default()
  };
  let held = dispatch::enqueue(&ground.db, &ground.events, &ask("start"), "job", async |_, _| Ok(()))
    .await
    .unwrap();
  let refused_waiting = on_slot(&ground.db, &json!({ "slot": held.token }), &mut Apart::default()).await;

  assert_eq!(refused_waiting.unwrap_err().code.as_deref(), Some("slot-not-active"));

  let active = dispatch::acquire(&ground, &held).await.unwrap().expect("let in");
  let mut apart = Apart::default();

  on_slot(&ground.db, &json!({ "slot": active.token, "provider": active.provider }), &mut apart)
    .await
    .unwrap();
  assert_eq!(
    (apart.role.as_deref(), apart.provider.as_str(), apart.model.as_str(), apart.cwd.as_str()),
    (Some("worker"), active.provider.as_str(), active.model.as_str(), "/checkouts/slot")
  );
  assert_eq!(apart.effort, active.effort);

  // A slot whose lease holds a demand has its session stand on that demand, so the turns ask none of their own.
  sea_orm::ConnectionTrait::execute_unprepared(
    &ground.db,
    &format!(
      r#"INSERT INTO "card_demands" ("demand_id", "service", "registration", "holder", "lease", "within", "asked_at")
         VALUES (77, 'engine', 'a-registration', 'owner:start', '{}', NULL, 1)"#,
      active.token
    ),
  )
  .await
  .unwrap();
  let mut on_lease = Apart::default();

  on_slot(&ground.db, &json!({ "slot": active.token }), &mut on_lease).await.unwrap();
  assert_eq!(on_lease.demand, Some(77), "the session stands on its slot's demand");

  let mut handed_in = Apart {
    demand: Some(5),
    ..Apart::default()
  };

  on_slot(&ground.db, &json!({ "slot": active.token }), &mut handed_in).await.unwrap();
  assert_eq!(handed_in.demand, Some(5), "a demand the caller handed in stands over its slot's");

  // Named by no caller, the rank is the supervisor's that asked the slot.
  let supervisor = "a-supervisor".to_owned();

  crate::support::rows::supervisor(&ground.db, &supervisor, &[]).await;

  store::run(
    &ground.db,
    r#"UPDATE "supervisors" SET "card_rank" = 3 WHERE "name" = $1"#,
    vec![supervisor.as_str().into()],
  )
  .await
  .unwrap();
  store::run(
    &ground.db,
    r#"UPDATE "role_slot_requests" SET "asked_by" = $2 WHERE "token" = $1"#,
    vec![active.token.as_str().into(), supervisor.as_str().into()],
  )
  .await
  .unwrap();

  let mut for_supervisor = Apart::default();

  on_slot(&ground.db, &json!({ "slot": active.token }), &mut for_supervisor)
    .await
    .unwrap();
  assert_eq!(for_supervisor.card_rank, Some(3));

  let mut named_by_caller = Apart {
    card_rank: Some(1),
    ..Apart::default()
  };

  on_slot(&ground.db, &json!({ "slot": active.token }), &mut named_by_caller)
    .await
    .unwrap();
  assert_eq!(named_by_caller.card_rank, Some(1), "a rank its caller names stands");

  let mut other_road = Apart {
    model: "another-model".into(),
    ..Apart::default()
  };
  let mut other_role = Apart {
    role: Some("builder".into()),
    ..Apart::default()
  };

  for apart in [&mut other_road, &mut other_role] {
    let refused = on_slot(&ground.db, &json!({ "slot": active.token }), apart).await.unwrap_err();

    assert_eq!(refused.code.as_deref(), Some("contradicts-slot"));
  }

  let mut named = Apart {
    provider: "claude-code".into(),
    model: "opus-5".into(),
    role: Some("worker".into()),
    ..Apart::default()
  };

  assert!(
    on_slot(&ground.db, &json!({}), &mut named).await.is_err(),
    "no slot, whatever road the start names"
  );
  assert!(
    on_slot(&ground.db, &json!({}), &mut Apart::default()).await.is_err(),
    "no slot and no road"
  );
}

// "the interrupt answers stopped only of a turn that ended, and a turn still going as running with its wait"
#[test]
fn the_interrupt_answers_stopped_only_of_a_turn_that_ended_and_a_turn_still_going_as_running_with_its_wait()
{
  let session = SessionId("actor@run:3".to_owned());
  let waiting = agent_session::Waiting {
    kind: agent_session::WaitKind::NodeLink,
    service: "pc-7".to_owned(),
    reason: "the link of pc-7 is not heard".to_owned(),
    since: 1,
  };
  let (status, ended) = interrupted(&session, 9, true, None);

  assert_eq!(status, 200);
  assert_eq!(
    (ended["state"].as_str(), ended["outcome"].as_str()),
    (Some("interrupted"), Some("stopped"))
  );

  let (status, going) = interrupted(&session, 9, false, Some(&waiting));

  assert_eq!(status, 202);
  assert_eq!(going["state"], "running");
  assert!(going["outcome"].is_null());
  assert_eq!(going["waiting"]["for"], "node-link");
  assert_eq!(going["waiting"]["service"], "pc-7");
}

// "the interrupt waits for the turn's end up to its bound, and no longer"
#[tokio::test(start_paused = true)]
async fn the_interrupt_waits_for_the_turn_s_end_up_to_its_bound_and_no_longer() {
  let look = std::time::Duration::from_millis(200);
  let bound = std::time::Duration::from_secs(10);
  let began = tokio::time::Instant::now();
  let looks = std::sync::atomic::AtomicU32::new(0);
  let ends_at_third = || async { looks.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 2 };

  assert!(ended_within(ends_at_third, bound, look).await);
  assert_eq!(began.elapsed(), look * 2);

  let began = tokio::time::Instant::now();

  assert!(!ended_within(|| async { false }, bound, look).await);
  assert_eq!(began.elapsed(), bound);
}
