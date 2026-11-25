//! A job's turn on its worker's session (docs/api.md, 「Sessions」, a work turn's parts): the session its
//! resume settles on, or the turn back to its runner; a lost session asked again once under `auto` and put
//! back under a session named; the composed turn handed its variables, its agent, its job's marker, its
//! work key and its unit; and its end read whole — the reply, a turn back, a person waited on, a stop, and
//! every failure by its kind — with what it went out on, where its time went and its compaction. An
//! answered turn's usage is recorded under its agent and the run it is logged under. A worker's
//! state names its generation, its context and what its next turn would continue, on a store of the test's
//! own.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::door::execution::Identity;
use agency_server::door::record::{
  AccountWindow, LiveTask, ModelSpend, Reply, ReplyWindow, RequestUsage, RunWorkflow, UsageSpent,
};
use agency_server::door::stand::Interrupt;
use agency_server::door::tool_clock::ToolClock;
use agency_server::door::turn::{Seat, Timings, Turn};
use agency_server::herdr_backend::{HerdrSessions, carried_of};
use agency_server::job_run::claude::failure::{AccountFailure, Failure, RunEnding};
use agency_server::job_run::claude::turn::{
  Beside, Choice, Settled, TASK_LOCKED, TURN_AGENT, Unanswered, after_unanswered, choose, compacts, ended,
  failure_of, noted, with_work, work_clocks,
};
use agency_server::places::{Ground, Places};
use agency_server::seeds::deployment;
use agency_server::worker_session::records::{self, generation, note_carried, record};
use agency_server::worker_session::{STALE_SESSION, UNKNOWN_SESSION};
use agent_session::{
  self as contract, Context, EngineError, JobMark, Mapping, Outcome, Resume, SessionId, Sessions, Span,
  WentOut, WorkParts,
};
use harness_providers::Machine;
use policy::providers::claude_code::CLAUDE_CODE;
use policy::tool_clock::TOOL_CLOCK;
use serde_json::json;

use crate::support::scratch::Scratch;

fn work(resume: Resume) -> WorkParts {
  WorkParts {
    provider: "claude-code".into(),
    model: "opus".into(),
    mapping: Mapping {
      model: "claude-opus-5".into(),
      effort: Some("high".into()),
      tier: Some("frontier".into()),
      answer_tokens: None,
    },
    resume,
    cwd: "/checkouts/a".into(),
    name: "bmo-job-41".into(),
    title: "#1802 build".into(),
    chat: -5,
    unit: Some("bmo-task-41".into()),
    user: None,
    work_key: "job-worker".into(),
    hears_words: true,
    variables: BTreeMap::from([
      ("BMO_EXTRA".into(), "1".into()),
      ("BMO_RUN_NAME".into(), "elsewhere".into()),
    ]),
  }
}

#[test]
fn the_session_a_turn_goes_on_is_the_one_its_resume_names_or_the_turn_goes_back() {
  let kept = || Some("harness-1".to_owned());

  assert_eq!(choose(&Resume::Auto, kept()), Choice::Go(kept()));
  assert_eq!(choose(&Resume::Auto, None), Choice::Go(None));
  assert_eq!(choose(&Resume::Fresh, kept()), Choice::Go(None));
  assert_eq!(
    choose(
      &Resume::Session {
        id: "harness-1".into()
      },
      kept()
    ),
    Choice::Go(kept()),
    "the session named, where the login carrying the turn keeps it"
  );
  assert_eq!(
    choose(
      &Resume::Session {
        id: "harness-0".into()
      },
      kept()
    ),
    Choice::Back(kept()),
    "a login whose session differs puts the turn back with the one it would continue"
  );
  assert_eq!(
    choose(
      &Resume::Session {
        id: "harness-0".into()
      },
      None
    ),
    Choice::Back(None)
  );
}

#[test]
fn a_lost_session_is_asked_again_once_under_auto_and_put_back_where_the_runner_named_it() {
  let lost = format!("Error: {UNKNOWN_SESSION} harness-1");
  let stale = format!("Error: {STALE_SESSION}. Provide a prompt to continue the conversation.");

  assert_eq!(after_unanswered(&Resume::Auto, Some("harness-1"), &lost), Unanswered::Again);
  assert_eq!(after_unanswered(&Resume::Auto, Some("harness-1"), &stale), Unanswered::Again);
  assert_eq!(
    after_unanswered(
      &Resume::Session {
        id: "harness-1".into()
      },
      Some("harness-1"),
      &lost
    ),
    Unanswered::Back
  );
  assert_eq!(
    after_unanswered(&Resume::Auto, None, &lost),
    Unanswered::Failed,
    "a fresh session lost nothing"
  );
  assert_eq!(
    after_unanswered(&Resume::Auto, Some("harness-1"), "the pane went away"),
    Unanswered::Failed
  );
}

fn composed() -> Turn {
  Turn {
    name: "bmo-job-41".into(),
    title: "#1802 build".into(),
    seat: Seat::Own(herdr::Herdr::there("/nowhere/herdr.sock".into())),
    cwd: "/checkouts/a".into(),
    command: vec![],
    env: BTreeMap::from([("BMO_RUN_NAME".into(), "bmo-job-41".into()), (TASK_LOCKED.into(), "1".into())]),
    prompt: "build".into(),
    session: "harness-1".into(),
    identity: Identity::default(),
    line: "worker".into(),
    provider: "claude-code".into(),
    lead: String::new(),
    trusts_folder: true,
    clock: None,
    interrupt: Interrupt::EndProgram,
    timings: Timings::default(),
  }
}

#[test]
fn a_job_turn_carries_its_variables_its_agent_its_jobs_marker_its_work_key_and_its_unit() {
  let job = Some(JobMark {
    job: 41,
    item: Some(1802),
  });
  let turn = with_work(composed(), "worker-a", &work(Resume::Auto), job);

  assert_eq!(turn.env["BMO_EXTRA"], "1");
  assert_eq!(
    turn.env["BMO_RUN_NAME"], "bmo-job-41",
    "the turn's own name stands over a variable of that name"
  );
  assert_eq!(turn.env[TURN_AGENT], "worker-a");
  assert_eq!(turn.env[TASK_LOCKED], "1");
  let idle = TOOL_CLOCK
    .call
    .in_process_seconds
    .max(TOOL_CLOCK.call.min_seconds)
    .saturating_mul(1_000);
  assert_eq!(
    turn.env[&CLAUDE_CODE.launch.mcp_idle_timeout],
    idle.to_string(),
    "a job's program reads the call clock's in-process bound as its MCP idle window"
  );
  assert_eq!(turn.identity.parent.as_deref(), Some("bmo-task-41"));
  assert_eq!(
    turn.identity.root.as_deref(),
    Some("bmo-task-41"),
    "a task unit counts its turns under itself"
  );

  let research = with_work(composed(), "worker-a", &work(Resume::Fresh), None);

  assert!(!research.env.contains_key(TASK_LOCKED), "a turn with no job works for none");
  assert_eq!(
    research.env[&CLAUDE_CODE.launch.mcp_idle_timeout],
    idle.to_string(),
    "a role session's turn is held to the call clock too, so it reads the same idle window"
  );
}

#[test]
fn a_role_session_s_turn_is_held_to_the_call_clock_and_only_a_job_s_turn_to_the_bound() {
  let job = work_clocks(Some(JobMark {
    job: 41,
    item: Some(1802),
  }));
  let role = work_clocks(None);
  let clock = ToolClock::from(&TOOL_CLOCK.call);

  assert_eq!(
    format!("{:?}", job.call),
    format!("{clock:?}"),
    "a job's turn keeps the call clock it had"
  );
  assert_eq!(
    job.bound,
    Some(Duration::from_secs(TOOL_CLOCK.turn.bound_minutes * 60)),
    "a job's turn keeps its hard bound"
  );
  assert_eq!(
    format!("{:?}", role.call),
    format!("{clock:?}"),
    "a role's run is held to the same call clock"
  );
  assert_eq!(role.bound, None, "a role's run holds no item, so it has no hard bound");
}

#[test]
fn a_compaction_is_read_off_the_records_lines_and_nothing_else_is_one() {
  let line = |value: serde_json::Value| value.to_string();

  assert!(compacts(&[
    line(json!({ "type": "assistant" })),
    line(json!({ "type": "system", "subtype": "compact_boundary" })),
  ]));
  assert!(!compacts(&[line(json!({ "type": "system", "subtype": "init" })), "not json".into()]));
}

fn beside() -> Beside {
  Beside {
    went_out: Some(WentOut {
      provider: "claude-code".into(),
      model: "claude-opus-5".into(),
      effort: Some("high".into()),
      tier: Some("frontier".into()),
    }),
    spans: vec![Span {
      kind: "tool".into(),
      name: "Bash".into(),
      seconds: 2.0,
      times: 1,
      tokens: None,
      input_tokens: None,
      output_tokens: None,
      cache_read_tokens: None,
      cache_write_tokens: None,
    }],
    compacted: true,
    engine_error: None,
    prefix: Some("the key of the command it went out under".into()),
  }
}

#[test]
fn an_answered_turn_ends_with_its_whole_reply_what_it_went_out_on_its_spans_and_its_compaction() {
  let reply = Reply {
    mode: "claude-code".into(),
    model: Some("claude-opus-5".into()),
    effort: Some("high".into()),
    text: "done".into(),
    session_id: Some("harness-1".into()),
    context_ratio: Some(0.25),
    context_tokens: Some(50_000),
    spend: vec![ModelSpend {
      mode: "claude-code".into(),
      model: "claude-opus-5".into(),
      tokens: 10,
      input_tokens: 4,
      output_tokens: 3,
      cache_read_tokens: 2,
      cache_write_tokens: 1,
      cost: 0.5,
    }],
    live_tasks: vec![LiveTask {
      id: "t-1".into(),
      description: "a watch".into(),
    }],
    workflows: vec![RunWorkflow {
      call: "c-1".into(),
      name: "review".into(),
      status: Some("done".into()),
    }],
    account: Some("acct-a".into()),
    request_usages: Some(vec![RequestUsage {
      input_tokens: 4,
      cache_read_tokens: 2,
      cache_write_tokens: 1,
      output_tokens: 3,
    }]),
    ..Reply::default()
  };
  let end = ended("bmo-job-41", Ok(Settled::Answered(Box::new(reply))), beside());

  assert_eq!(
    serde_json::to_value(&end).unwrap().get("prefix"),
    None,
    "the key stays in agency's own process, off the turn's event"
  );

  assert_eq!(
    (end.outcome, end.answer.as_deref(), end.reason),
    (Outcome::Answered, Some("done"), None)
  );
  assert_eq!(end.run, "bmo-job-41");
  assert_eq!(end.went_out, beside().went_out);
  assert_eq!(end.spans, beside().spans);
  assert!(end.compacted);
  assert_eq!(end.prefix, beside().prefix, "the next turn tells a moved prefix by it");

  let carried = end.reply.unwrap();

  assert_eq!(
    (carried.session_id.as_deref(), carried.account.as_deref(), carried.context_tokens),
    (Some("harness-1"), Some("acct-a"), Some(50_000))
  );
  assert_eq!((carried.spend[0].cache_read_tokens, carried.spend[0].cost), (2, Some(0.5)));
  assert_eq!(carried.live_tasks[0].description, "a watch");
  assert_eq!(carried.workflows[0].status.as_deref(), Some("done"));
  assert_eq!(carried.request_usages.unwrap()[0].output_tokens, 3);
  assert_eq!(carried.upstream, None, "the Claude Code line goes to no upstream of its own");
}

#[test]
fn a_turn_put_back_a_person_waited_on_and_a_stop_end_as_themselves() {
  let back = ended("r", Ok(Settled::Back(Some("harness-2".into()))), Beside::default());

  assert_eq!((back.outcome, back.continues.as_deref()), (Outcome::Reprepared, Some("harness-2")));
  assert_eq!(ended("r", Ok(Settled::Back(None)), Beside::default()).continues, None);

  let person = ended("r", Err(Failure::Person), Beside::default());

  assert_eq!((person.outcome, person.delivery), (Outcome::Blocked, contract::Delivery::Blocked));
  assert_eq!(person.failure, Some(contract::Failure::Person));

  let stopped = ended("r", Err(Failure::Stopped), Beside::default());

  assert_eq!((stopped.outcome, stopped.failure), (Outcome::Stopped, Some(contract::Failure::Stopped)));
}

#[test]
fn every_failure_a_job_turn_comes_to_ends_failed_by_its_kind_with_its_words() {
  let died = Failure::Died {
    ending: RunEnding::Exit { code: 1 },
    said: Some("gone".into()),
    message: "claude exited with code 1: gone".into(),
  };
  let cases = [
    (
      Failure::Limit {
        resets_at: Some(1_700),
        account: Some("acct-a".into()),
        silenced: true,
        named: None,
        model: None,
      },
      json!({ "kind": "limit", "resets_at": 1_700, "account": "acct-a", "silenced": true }),
    ),
    (
      Failure::Account {
        failure: AccountFailure::Login,
        detail: "job-worker: the login is gone".into(),
      },
      json!({ "kind": "account", "failure": "login", "detail": "job-worker: the login is gone" }),
    ),
    (
      Failure::Account {
        failure: AccountFailure::Outage,
        detail: "API Error: 529".into(),
      },
      json!({ "kind": "account", "failure": "outage", "detail": "API Error: 529" }),
    ),
    (
      Failure::Account {
        failure: AccountFailure::NodeToken,
        detail: "its token for the work nodes is gone".into(),
      },
      json!({ "kind": "account", "failure": "node_token", "detail": "its token for the work nodes is gone" }),
    ),
    (
      died.clone(),
      json!({ "kind": "died", "ending": { "kind": "exit", "code": 1 }, "said": "gone",
        "message": "claude exited with code 1: gone" }),
    ),
    (
      Failure::Untrusted {
        folder: "/checkouts/a".into(),
        config: "/checkouts/.claude.json".into(),
      },
      json!({ "kind": "untrusted", "folder": "/checkouts/a", "config": "/checkouts/.claude.json" }),
    ),
    (
      Failure::Transport {
        message: "the pane went away".into(),
      },
      json!({ "kind": "transport", "message": "the pane went away" }),
    ),
    (
      Failure::Refused {
        message: "prompt is too long".into(),
      },
      json!({ "kind": "refused", "message": "prompt is too long" }),
    ),
    (
      Failure::Unavailable {
        scope: "job-worker:worker-a".into(),
        mode: "claude-code".into(),
        reason: "Claude Code is switched off".into(),
      },
      json!({ "kind": "unavailable", "scope": "job-worker:worker-a", "mode": "claude-code",
        "reason": "Claude Code is switched off" }),
    ),
    (
      Failure::error("agent lock: busy"),
      json!({ "kind": "error", "name": "Error", "message": "agent lock: busy" }),
    ),
  ];

  for (failure, wire) in cases {
    let words = failure.message();
    let end = ended("r", Err(failure), Beside::default());

    assert_eq!(end.outcome, Outcome::Failed);
    assert_eq!(end.reason.as_deref(), Some(words.as_str()));
    assert_eq!(json!(end.failure), wire);
  }

  let errored = ended(
    "r",
    Err(died),
    Beside {
      engine_error: Some(EngineError {
        kind: Some("overloaded".into()),
      }),
      ..Beside::default()
    },
  );

  assert_eq!(
    errored.engine_error,
    Some(EngineError {
      kind: Some("overloaded".into())
    })
  );
  assert_eq!(
    failure_of(&Failure::Exhausted { attempted: vec![] }),
    contract::Failure::Error {
      name: "Error".into(),
      message: Failure::Exhausted { attempted: vec![] }.message()
    }
  );
}

#[test]
fn a_records_context_reads_as_the_tokens_it_carried_against_the_window_they_filled() {
  assert_eq!(
    carried_of(Some(50_000), Some(0.25)),
    Some(Context {
      carried: 50_000,
      window: 200_000
    })
  );
  assert_eq!(carried_of(Some(50_000), None), None);
  assert_eq!(carried_of(None, Some(0.25)), None);
  assert_eq!(carried_of(Some(50_000), Some(0.0)), None);
}

fn answered(tokens: i64, ratio: f64) -> Reply {
  Reply {
    mode: "claude-code".into(),
    context_tokens: Some(tokens),
    context_ratio: Some(ratio),
    ..Reply::default()
  }
}

#[tokio::test]
async fn a_fresh_session_is_counted_as_a_generation_a_resumed_one_is_not_and_a_forgotten_record_keeps_the_count()
 {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  assert_eq!(
    generation(db, "worker-a", "opus").await.unwrap(),
    1,
    "the first session is the first generation"
  );

  record(db, "worker-a", "opus", "default", "harness-1", None, None).await.unwrap();
  note_carried(db, "worker-a", "opus", "default", true, &answered(40_000, 0.2))
    .await
    .unwrap();
  record(db, "worker-a", "opus", "default", "harness-1", None, None).await.unwrap();
  note_carried(db, "worker-a", "opus", "default", false, &answered(60_000, 0.3))
    .await
    .unwrap();

  assert_eq!(generation(db, "worker-a", "opus").await.unwrap(), 1);

  let kept = records::sessions(db, "worker-a").await.unwrap();

  assert_eq!((kept[0].context_tokens, kept[0].context_ratio), (Some(60_000), Some(0.3)));

  records::forget(db, "worker-a", "opus", None).await.unwrap();
  record(db, "worker-a", "opus", "default", "harness-2", None, None).await.unwrap();
  note_carried(db, "worker-a", "opus", "default", true, &answered(1_000, 0.01))
    .await
    .unwrap();

  assert_eq!(generation(db, "worker-a", "opus").await.unwrap(), 2);
  assert_eq!(generation(db, "worker-a", "sonnet").await.unwrap(), 1, "each model counts its own");
}

// A ground on the test's own store and home, with the repository `worker-a` registered on it.
async fn ground_on(scratch: &Scratch, home: &std::path::Path) -> Ground {
  let at = |path: std::path::PathBuf| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.to_path_buf())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.join("data"),
  );

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();
  crate::support::rows::repository(scratch.db(), "worker-a", &at(home.join("worker-a")), &[]).await;

  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());

  Ground::new(&service, &machine, None)
}

#[tokio::test]
async fn an_answered_job_turns_usage_is_recorded_under_its_agent_and_the_run_it_is_logged_under() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(&scratch, home.path()).await;
  let mut spent = ground.events.subscribe::<UsageSpent>();
  let second = cache_siren::Slot {
    mode: "claude-code",
    account: "second",
  };
  let window = AccountWindow {
    name: "five_hour".into(),
    utilization: 0.4,
    resets_at: Some(1_800_000_000),
    duration_minutes: Some(300),
    limit_name: None,
  };
  let reply = Reply {
    mode: "claude-code".into(),
    model: Some("claude-opus-5".into()),
    effort: Some("high".into()),
    account: Some("second".into()),
    windows: vec![ReplyWindow::Account(window.clone())],
    spend: vec![ModelSpend {
      mode: String::new(),
      model: "claude-opus-5".into(),
      tokens: 1_200,
      input_tokens: 200,
      output_tokens: 1_000,
      cache_read_tokens: 0,
      cache_write_tokens: 0,
      cost: 0.5,
    }],
    ..Reply::default()
  };

  noted(&ground, "worker-a", "bmo-task-41", &reply).await.unwrap();

  let kept = ground.windows.read(second).await.unwrap().windows;

  assert_eq!(
    kept.into_iter().map(|kept| kept.window).collect::<Vec<_>>(),
    std::slice::from_ref(&window),
    "the windows the reply read are kept under the login that answered"
  );

  let usage = spent.ready().expect("what the turn spent is published");

  assert_eq!((usage.agent.as_str(), usage.run.as_deref()), ("worker-a", Some("bmo-task-41")));
  assert_eq!(
    usage
      .rows
      .iter()
      .map(|row| (row.account.as_str(), row.spend.mode.as_str()))
      .collect::<Vec<_>>(),
    [("second", "claude-code")],
    "a row its program named no provider for is the turn's own provider's"
  );

  noted(
    &ground,
    "worker-a",
    "bmo-job-41",
    &Reply {
      windows: vec![],
      ..reply.clone()
    },
  )
  .await
  .unwrap();

  assert_eq!(
    ground.windows.read(second).await.unwrap().windows.len(),
    1,
    "a reply that read no window leaves the slot as it was"
  );
  assert_eq!(spent.ready().unwrap().run.as_deref(), Some("bmo-job-41"));

  noted(
    &ground,
    "worker-a",
    "bmo-job-42",
    &Reply {
      account: None,
      ..reply
    },
  )
  .await
  .unwrap();

  assert!(
    ground
      .windows
      .read(cache_siren::Slot {
        mode: "claude-code",
        account: "default"
      })
      .await
      .unwrap()
      .windows
      .is_empty(),
    "a reading whose login cannot be named is not written"
  );
}

#[tokio::test]
async fn a_workers_state_names_its_generation_its_context_and_the_session_its_next_turn_would_continue() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let sessions = HerdrSessions::new(Places::new(ground_on(&scratch, home.path()).await, Arc::default()));
  let db = scratch.db();

  record(db, "worker-a", "opus", "default", "harness-1", None, None).await.unwrap();
  note_carried(db, "worker-a", "opus", "default", true, &answered(50_000, 0.25))
    .await
    .unwrap();

  let session = SessionId("worker-a@worker".into());
  let state = sessions.read_state(&session, None).await.unwrap();

  assert_eq!(
    (state.provider.as_str(), state.model.as_str(), state.generation),
    ("claude-code", "opus", 1)
  );
  assert_eq!(
    state.context,
    Some(Context {
      carried: 50_000,
      window: 200_000
    })
  );
  assert_eq!((state.run, state.went_out, state.continues), (None, None, None));
  assert_eq!(
    sessions.read_state(&session, Some("opus")).await.unwrap().continues.as_deref(),
    Some("harness-1")
  );
  assert_eq!(sessions.read_state(&session, Some("sonnet")).await.unwrap().continues, None);
}

// A checkout's state answers the provider its repository is pinned to, not Claude Code whatever its pin.
#[tokio::test]
async fn a_workers_state_names_the_provider_its_pin_stands_on() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let sessions = HerdrSessions::new(Places::new(ground_on(&scratch, home.path()).await, Arc::default()));
  let db = scratch.db();

  record(db, "worker-a", "opus", "default", "harness-1", None, None).await.unwrap();
  scratch
    .run(
      r#"UPDATE "repositories" SET "pinned_provider" = 'openrouter', "pinned_model" = 'vendor/model'
         WHERE "name" = 'worker-a'"#,
      vec![],
    )
    .await;

  let state = sessions.read_state(&SessionId("worker-a@worker".into()), None).await.unwrap();

  assert_eq!(state.provider, "openrouter", "a pinned worker stands on the provider it is pinned to");
  assert_eq!(state.model, "opus", "its model is still the one its session answered on");
}
