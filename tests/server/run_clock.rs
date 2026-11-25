//! A run held to a clock is asked about past it, never cut by it (docs/agent-control.md, 「A clock asks, it
//! never cuts」): its clock read by its name and the unit it runs under, an ask opened and published past the
//! clock, a keep holding it until its moment, the supervisor's cut cutting it, an ask with no verdict standing
//! for as long as the run goes, a verdict on a closed ask refused, and the run's end closing what stood open —
//! on a store of the test's own. A job's tool call gone silent and idle is asked about the same way, with the
//! facts the supervisor judges it by, and not while the job's item holds a question still waiting.

use std::time::Duration;

use chrono::TimeZone as _;
use serde_json::json;

use agency_server::door::tool_clock::{CallVerdict, SilentCall, WaitedWork};
use agency_server::door::turn::{Overrun, Overtime, Quiet, Watch};
use agency_server::events::Bus;
use agency_server::now_ms;
use agency_server::queue::orderer::JobService;
use agency_server::run_clock::{
  Asked, CallAsks, ClockAskClosed, ClockAsked, Clocked, NotSettled, Verdict, clock_of, close, open,
  open_asks, settle,
};

use crate::support::scratch::Scratch;

fn clocked(run: &str) -> Clocked {
  Clocked {
    run: run.to_owned(),
    parent: Some("batch-unit".to_owned()),
    work_key: "voice-reading".to_owned(),
    agent: Some("supervisor".to_owned()),
    what: "The voice-reading run".to_owned(),
    clock: Duration::from_secs(60),
  }
}

fn overdue() -> Overrun {
  Overrun {
    started_at: now_ms() - 61_000,
    bound_ms: 60_000,
    last_activity_at: None,
    last_activity: Some("Read".to_owned()),
  }
}

#[tokio::test]
async fn a_clocked_run_is_asked_about_kept_and_cut_on_the_chat_agents_verdict() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let bus = Bus::default();
  let mut asked_events = bus.subscribe::<ClockAsked>();
  let mut closed_events = bus.subscribe::<ClockAskClosed>();
  let clock = clocked("run-one");

  open(db, &clock).await.unwrap();

  let read = clock_of(db, "batch-unit").await.unwrap();

  assert_eq!(read, clock_of(db, "run-one").await.unwrap());
  assert!(read.cap_ends.unwrap() >= now_ms() + 50_000);
  assert_eq!(read.kept_until, None);
  assert_eq!(clock_of(db, "another").await.unwrap().cap_ends, None);

  let mut watch = Asked::new(Quiet, db, &bus, &clock);

  assert!(matches!(watch.overtime(&overdue()).await, Overtime::AskAgainAt(_)));

  let ask = asked_events.ready().expect("the ask was published");

  assert_eq!(
    (ask.run.as_str(), ask.agent.as_str(), ask.verb.as_str()),
    ("run-one", "supervisor", "bmo-agency overtime")
  );
  assert_eq!(clock_of(db, "run-one").await.unwrap().kept_until, None);
  assert_eq!(open_asks(db).await.unwrap().len(), 1);

  let kept = settle(db, &bus, ask.ask, &Verdict::Keep(Some(20)), "still reading").await.unwrap();
  let until = kept.kept_until.unwrap();

  assert!((until - now_ms() - 20 * 60_000).abs() < 5_000);
  assert_eq!(closed_events.ready().unwrap().by.as_deref(), Some("agent"));
  assert!(matches!(
    settle(db, &bus, ask.ask, &Verdict::Cut, "late").await,
    Err(NotSettled::Closed)
  ));
  assert!(matches!(settle(db, &bus, 999_999, &Verdict::Cut, "").await, Err(NotSettled::Unknown)));
  assert_eq!(watch.overtime(&overdue()).await, Overtime::AskAgainAt(until));
  assert_eq!(clock_of(db, "batch-unit").await.unwrap().kept_until, Some(until));

  // The keep run out, the run is asked again, and cut on the verdict with the supervisor's reason.
  scratch.run(r#"UPDATE "clock_asks" SET "kept_until" = 1"#, vec![]).await;

  assert!(matches!(watch.overtime(&overdue()).await, Overtime::AskAgainAt(_)));

  let again = asked_events.ready().expect("the second ask was published");

  settle(db, &bus, again.ask, &Verdict::Cut, "stuck on one file").await.unwrap();

  assert_eq!(watch.overtime(&overdue()).await, Overtime::Cut);
  assert!(watch.cut.as_deref().unwrap().contains("stuck on one file"));

  close(db, &bus, "run-one").await.unwrap();

  assert_eq!(clock_of(db, "run-one").await.unwrap().cap_ends, None);
}

#[tokio::test]
async fn an_ask_with_no_verdict_stands_while_the_run_goes_and_closes_as_ended_at_its_end() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let bus = Bus::default();
  let mut asked_events = bus.subscribe::<ClockAsked>();
  let mut closed_events = bus.subscribe::<ClockAskClosed>();
  let clock = clocked("run-two");

  open(db, &clock).await.unwrap();

  let mut watch = Asked::new(Quiet, db, &bus, &clock);

  watch.overtime(&overdue()).await;

  let open_ask = asked_events.ready().unwrap();

  // However long it stands unanswered, the run is only looked at again.
  scratch.run(r#"UPDATE "clock_asks" SET "asked_at" = 1"#, vec![]).await;

  for _ in 0..3 {
    assert!(matches!(watch.overtime(&overdue()).await, Overtime::AskAgainAt(_)));
  }

  assert!(watch.cut.is_none());
  assert!(closed_events.ready().is_none());
  assert_eq!(open_asks(db).await.unwrap().len(), 1);

  close(db, &bus, "run-two").await.unwrap();

  let ended = closed_events.ready().unwrap();

  assert_eq!((ended.ask, ended.verdict.as_str(), ended.by), (open_ask.ask, "ended", None));
  assert!(open_asks(db).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_silent_call_is_asked_about_with_its_facts_and_kept_or_cut_on_the_verdict() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let bus = Bus::default();
  let mut asked_events = bus.subscribe::<ClockAsked>();
  let mut closed_events = bus.subscribe::<ClockAskClosed>();
  let now = now_ms();

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt")
         VALUES (7, 2807, 1, 'runner', 1, 'worker-one', 'running', 1000, 'the work')"#,
      vec![],
    )
    .await;

  let mut asks = CallAsks::new("job-run-7", "worker-one", 7, None);
  let call = SilentCall {
    id: "toolu_1".to_owned(),
    tool: "Bash until rg -q closed close.log; do sleep 15; done".to_owned(),
    started_at: now - 1_800_000,
    output_changed_at: now - 1_500_000,
    quiet_since: now - 1_500_000,
    work: None,
    last_line: Some("waiting for the other job to close".to_owned()),
    cpu_percent: 0.4,
    window_ms: 300_000,
  };

  assert_eq!(asks.hear(db, &bus, &call).await, CallVerdict::Waiting);

  let ask = asked_events.ready().expect("the ask was published");

  assert_eq!((ask.run.as_str(), ask.agent.as_str()), ("job-run-7/call/toolu_1", "worker-one"));
  assert!(
    ask.what.contains("job 7 (item #2807)") && ask.what.contains(&call.tool) && ask.what.contains("0.4%"),
    "the ask names the job, its item, the command and its share: {}",
    ask.what
  );
  assert!(
    ask.what.contains("silent and idle for 25 minutes,"),
    "and how long it was silent: {}",
    ask.what
  );
  assert_eq!(ask.started_at, call.started_at, "how long it has run");
  assert_eq!(ask.bound_ms, 300_000);
  assert_eq!(ask.last_activity_at, Some(call.output_changed_at), "when its output last grew");
  assert_eq!(ask.last_activity.as_deref(), Some("waiting for the other job to close"));
  assert!(
    clock_of(db, "job-run-7").await.unwrap().cap_ends.is_some(),
    "the turn's run reads its clock"
  );

  // With no verdict the ask stands, and no second one is opened.
  assert_eq!(asks.hear(db, &bus, &call).await, CallVerdict::Waiting);
  assert!(asked_events.ready().is_none());

  // A keep holds the call until it runs out, read on the turn's run as well.
  let kept = settle(db, &bus, ask.ask, &Verdict::Keep(Some(30)), "a testbed line it waits on")
    .await
    .unwrap();
  let until = kept.kept_until.unwrap();

  assert_eq!(asks.hear(db, &bus, &call).await, CallVerdict::Kept(until));
  assert_eq!(clock_of(db, "job-run-7").await.unwrap().kept_until, Some(until));

  // Handed over again once the keep ran out, a new ask is opened; a cut answers with the reason.
  assert_eq!(asks.hear(db, &bus, &call).await, CallVerdict::Waiting);

  let again = asked_events.ready().expect("the second ask was published");

  assert_ne!(again.ask, ask.ask);
  settle(db, &bus, again.ask, &Verdict::Cut, "it waits on another job's close")
    .await
    .unwrap();
  assert_eq!(
    asks.hear(db, &bus, &call).await,
    CallVerdict::Cut("it waits on another job's close".to_owned())
  );

  // A call that has written nothing says so, and the turn's end closes every ask on its calls.
  let quiet = SilentCall {
    id: "toolu_2".to_owned(),
    last_line: None,
    ..call.clone()
  };

  asks.hear(db, &bus, &quiet).await;

  let nothing = asked_events.ready().unwrap();

  assert_eq!(nothing.last_activity.as_deref(), Some("it has written nothing"));
  while closed_events.ready().is_some() {}
  asks.ended(db, &bus, None).await;
  assert_eq!(closed_events.ready().map(|closed| closed.ask), Some(nothing.ask));
  assert!(open_asks(db).await.unwrap().is_empty());
  assert_eq!(clock_of(db, "job-run-7").await.unwrap().cap_ends, None);
}

#[tokio::test]
async fn a_silent_call_is_not_asked_about_while_the_job_s_question_waits_and_is_once_it_is_settled() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let bus = Bus::default();
  let mut asked_events = bus.subscribe::<ClockAsked>();
  let (peers, vars) = crate::support::peers::stand().await;
  let now = now_ms();

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt")
         VALUES (8, 2808, 1, 'runner', 1, 'worker-one', 'running', 1000, 'the work')"#,
      vec![],
    )
    .await;
  peers.lock().unwrap().questions.extend([
    json!({ "id": 1, "itemId": 2807, "state": "open" }),
    json!({ "id": 2, "itemId": 2808, "state": "answered" }),
    json!({ "id": 3, "itemId": 2808, "state": "paused" }),
  ]);

  let mut asks = CallAsks::new("job-run-8", "worker-one", 8, JobService::of(&vars));
  let call = SilentCall {
    id: "toolu_1".to_owned(),
    tool: "Bash bmo job ask --talk 3 --say 'Waiting.' --ack".to_owned(),
    started_at: now - 600_000,
    output_changed_at: now - 600_000,
    quiet_since: now - 600_000,
    work: None,
    last_line: None,
    cpu_percent: 0.1,
    window_ms: 300_000,
  };

  // The item's own question still waits: the call waits on it, seen now, and nothing is asked.
  let CallVerdict::Awaiting(seen) = asks.hear(db, &bus, &call).await else {
    panic!("the call waits on the question");
  };

  assert!(seen >= now, "it was seen waiting as it was read");
  assert!(asked_events.ready().is_none());
  assert!(open_asks(db).await.unwrap().is_empty());

  // Settled, the question still holds the call for the window from when it was last seen waiting.
  peers.lock().unwrap().questions[2]["state"] = json!("answered");
  assert_eq!(asks.hear(db, &bus, &call).await, CallVerdict::Awaiting(seen));
  assert!(asked_events.ready().is_none());

  // Past that window the call is asked about as any silent call; another item's open question holds
  // nothing.
  let past = SilentCall {
    window_ms: 0,
    quiet_since: seen,
    ..call.clone()
  };

  assert_eq!(asks.hear(db, &bus, &past).await, CallVerdict::Waiting);

  // The ask counts its silence from the question, and says so.
  let ask = asked_events.ready().expect("the ask was published");
  let at = chrono::Local.timestamp_millis_opt(seen).unwrap().format("%H:%M:%S").to_string();
  let since = format!("the job's own question #3 was last seen waiting at {at}");

  assert_eq!(ask.run, "job-run-8/call/toolu_1");
  assert!(
    ask.what.contains(&format!("silent and idle for 1 minutes since {since},")),
    "the ask says the silence it was opened by and what the call waited on: {}",
    ask.what
  );
  assert_eq!(ask.last_activity_at, Some(seen));
  assert_eq!(ask.last_activity.as_deref(), Some(since.as_str()));

  // A call that polled the log of the job's own work in the background counts its silence from that work.
  let moved_at = now - 400_000;
  let polling = SilentCall {
    id: "toolu_2".to_owned(),
    tool: "Bash for i in {1..360}; do rg -q '^exit ' /tmp/check.log && break; sleep 10; done".to_owned(),
    quiet_since: moved_at,
    work: Some(WaitedWork {
      path: "/tmp/check.log".to_owned(),
      moved_at,
    }),
    ..call.clone()
  };

  assert_eq!(asks.hear(db, &bus, &polling).await, CallVerdict::Waiting);

  let ask = asked_events.ready().expect("the ask on the poll was published");
  let at = chrono::Local.timestamp_millis_opt(moved_at).unwrap().format("%H:%M:%S").to_string();
  let since = format!("the job's own work writing /tmp/check.log was last seen moving at {at}");

  assert!(
    ask.what.contains(&format!("silent and idle for 6 minutes since {since},")),
    "the ask says what the poll waited on and since when: {}",
    ask.what
  );
  assert_eq!(
    (ask.last_activity_at, ask.last_activity.as_deref()),
    (Some(moved_at), Some(since.as_str()))
  );
}

// A call due for its stall before it was ever handed over is found waiting on the item's question, so once the
// question is settled it is still held for the window, as a call handed over is.
#[tokio::test]
async fn a_call_due_for_its_stall_is_found_waiting_on_the_job_s_question_and_held_once_it_is_settled() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let bus = Bus::default();
  let mut asked_events = bus.subscribe::<ClockAsked>();
  let (peers, vars) = crate::support::peers::stand().await;
  let now = now_ms();

  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt")
         VALUES (8, 2808, 1, 'runner', 1, 'worker-one', 'running', 1000, 'the work'),
                (9, 2809, 2, 'runner', 2, 'worker-two', 'running', 1000, 'the work')"#,
      vec![],
    )
    .await;
  peers.lock().unwrap().questions.extend([
    json!({ "id": 1, "itemId": 2808, "state": "open" }),
    json!({ "id": 2, "itemId": 2809, "state": "withdrawn" }),
  ]);

  let mut asks = CallAsks::new("job-run-8", "worker-one", 8, JobService::of(&vars));
  let mut other = CallAsks::new("job-run-9", "worker-two", 9, JobService::of(&vars));

  assert!(asks.waits_on_question(db, "toolu_1").await, "its item's question waits");
  assert!(!other.waits_on_question(db, "toolu_1").await, "a withdrawn question holds nothing");

  peers.lock().unwrap().questions[0]["state"] = json!("answered");

  let call = SilentCall {
    id: "toolu_1".to_owned(),
    tool: "Bash bmo job ask --talk 1 --ack".to_owned(),
    started_at: now - 600_000,
    output_changed_at: now - 600_000,
    quiet_since: now - 600_000,
    work: None,
    last_line: None,
    cpu_percent: 0.1,
    window_ms: 300_000,
  };
  let CallVerdict::Awaiting(seen) = asks.hear(db, &bus, &call).await else {
    panic!("the settled question still holds the call");
  };

  assert!(seen >= now, "held from when the stall found it waiting");
  assert!(asked_events.ready().is_none(), "nothing is asked");
}
