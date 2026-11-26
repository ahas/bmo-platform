//! The role slots (docs/role-slots.md): a role's runs are let in under its cap and no further, a slot given
//! back lets the next in, a run whose provider has no quota waits — never refused — and goes once quota
//! comes back, an unknown role is an error, and a role whose agents name no road follows the supervisor asking
//! it: a slot of that role on the supervisor's own provider and model.

use agency_server::order_run::dispatch::{self, Asked, Wanted, acquire, enqueue, finish, used};
use agency_server::places::Ground;

use crate::support::grounds::{deepseek_balance, openrouter_road, provider_balance, seeded, seeded_with};
use crate::support::scratch::Scratch;

fn asked<'a>(role: &'a str, key: &str) -> Asked<'a> {
  Asked {
    request_key: format!("test:{key}"),
    owner: format!("owner:{key}"),
    wanted: Wanted {
      role,
      path: Some("/checkouts/one"),
      fixed: None,
      agent: None,
    },
    place_key: None,
    ..Asked::default()
  }
}

async fn ask(ground: &Ground, role: &str, key: &str) -> dispatch::Request {
  enqueue(ground, &asked(role, key), "job", async |_, _| Ok(())).await.unwrap()
}

async fn admitted(ground: &Ground, held: &dispatch::Request) -> bool {
  let now = dispatch::request(&ground.db, &held.token).await.unwrap().unwrap();

  acquire(ground, &now).await.unwrap().is_some()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_roles_runs_are_let_in_up_to_its_cap_and_the_next_waits_for_a_slot_given_back() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;

  let mut held = vec![];

  for key in ["one", "two", "three", "four"] {
    held.push(ask(&ground, "worker", key).await);
  }

  for request in &held[..3] {
    assert!(admitted(&ground, request).await, "within the worker role's cap of 3");
  }

  assert!(!admitted(&ground, &held[3]).await, "the fourth waits for a slot");
  assert_eq!(used(&ground.db, "worker", None).await.unwrap(), 3);

  let let_in = dispatch::request(&ground.db, &held[0].token).await.unwrap().unwrap();

  assert_eq!(let_in.provider, "deepseek", "let in on the role's active provider");
  assert_eq!(let_in.model, "deepseek-flash");
  assert_eq!(let_in.capacity, 3, "the cap is the lease's snapshot");

  assert!(finish(&ground.db, &ground.events, &held[0].token, "done", false).await.unwrap());
  assert!(admitted(&ground, &held[3]).await, "a slot given back lets the next in");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_raised_cap_lets_in_the_runs_waiting_at_their_next_ask() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;
  scratch
    .run(r#"UPDATE "roles" SET "max_concurrent" = 1 WHERE "name" = 'worker'"#, vec![])
    .await;
  crate::support::rows::reseated(scratch.db()).await;

  let first = ask(&ground, "worker", "first").await;
  let second = ask(&ground, "worker", "second").await;

  assert!(admitted(&ground, &first).await);
  assert!(!admitted(&ground, &second).await);

  scratch
    .run(r#"UPDATE "roles" SET "max_concurrent" = 2 WHERE "name" = 'worker'"#, vec![])
    .await;
  crate::support::rows::reseated(scratch.db()).await;

  assert!(admitted(&ground, &second).await, "the cap is read as every run asks to be let in");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_whose_providers_have_no_quota_waits_with_the_reason_and_goes_once_quota_comes_back() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  // DeepSeek's balance spent and every other provider of the worker's order switched off: none has quota.
  deepseek_balance(&ground, 0.0).await;

  for provider in ["claude-code", "codex", "openrouter", "registered"] {
    agency_server::control::harness::set_provider_enabled(&ground.db, provider, false)
      .await
      .unwrap();
  }

  let waiting = ask(&ground, "worker", "waits").await;

  let let_in = admitted(&ground, &waiting).await;
  let seen = dispatch::request(&ground.db, &waiting.token).await.unwrap().unwrap();

  assert!(!let_in, "no provider of the order has quota: it waits: {seen:?}");

  let held = dispatch::request(&ground.db, &waiting.token).await.unwrap().unwrap();

  assert_eq!(held.state, "waiting", "a run waiting for quota is never refused");
  assert!(held.held_for.as_deref().is_some_and(|why| why.contains("deepseek")), "{held:?}");
  assert_eq!(
    used(&ground.db, "worker", None).await.unwrap(),
    0,
    "a run waiting for quota holds no slot"
  );

  deepseek_balance(&ground, 5.0).await;

  assert!(admitted(&ground, &waiting).await, "quota back: the run goes out");

  let gone = dispatch::request(&ground.db, &waiting.token).await.unwrap().unwrap();

  assert_eq!(gone.held_for, None);
  assert_eq!(gone.provider, "deepseek");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_slot_asked_under_a_role_the_store_does_not_hold_is_refused() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let refused = enqueue(&ground, &asked("general-purpose", "unknown"), "native", async |_, _| Ok(())).await;

  assert!(refused.is_err_and(|why| why.contains("No role named general-purpose")));
  assert_eq!(
    scratch.count(r#"SELECT COUNT(*) FROM "role_slot_requests""#, vec![]).await,
    0,
    "nothing starts"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_of_a_role_that_follows_its_supervisor_holds_a_slot_of_that_role_on_the_supervisors_road() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;
  scratch
    .run(
      r#"INSERT INTO "supervisors" ("name", "session_scope", "turn_shape", "provider", "model", "effort",
           "created_at", "updated_at")
         VALUES ('chat', 'agent', 'window', 'deepseek', 'deepseek-flash', 'high', 0, 0)"#,
      vec![],
    )
    .await;

  let fixed = dispatch::fixed_road(&ground.db, "coder", Some("chat"))
    .await
    .unwrap()
    .expect("the supervisor's own road");

  assert_eq!((fixed.provider.as_str(), fixed.model.as_str()), ("deepseek", "deepseek-flash"));
  assert_eq!(fixed.effort.as_deref(), Some("high"), "at the effort the supervisor stands on");

  let runs: Vec<_> = {
    let mut held = vec![];

    for key in ["first", "second", "third", "fourth"] {
      let run = Asked {
        request_key: format!("follows:{key}"),
        owner: format!("follows:{key}"),
        wanted: Wanted {
          role: "coder",
          path: Some("/scratch/chat"),
          fixed: Some(fixed.clone()),
          agent: None,
        },
        asked_by: Some("chat"),
        ..Asked::default()
      };

      held.push(enqueue(&ground, &run, "job", async |_, _| Ok(())).await.unwrap());
    }

    held
  };

  for run in &runs[..3] {
    assert!(admitted(&ground, run).await);
  }

  assert!(!admitted(&ground, &runs[3]).await, "the coder role's cap of 3 counts every run of it");
  assert_eq!(used(&ground.db, "coder", None).await.unwrap(), 3);
  assert_eq!(
    used(&ground.db, "producer", None).await.unwrap(),
    0,
    "a coder's run holds no producer slot"
  );

  let running = dispatch::request(&ground.db, &runs[0].token).await.unwrap().unwrap();

  assert_eq!(running.role, "coder");
  assert_eq!((running.provider.as_str(), running.model.as_str()), ("deepseek", "deepseek-flash"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_of_a_role_that_follows_its_supervisor_waits_on_that_road_and_never_moves_off_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 0.0).await;

  let run = Asked {
    request_key: "follows:waits".into(),
    owner: "follows:waits".into(),
    wanted: Wanted {
      role: "coder",
      path: Some("/scratch/chat"),
      fixed: Some(agency_server::order::Entry {
        provider: "deepseek".into(),
        model: "deepseek-flash".into(),
        effort: None,
        thinking: None,
        answer_tokens: None,
        advisor: None,
      }),
      agent: None,
    },
    ..Asked::default()
  };
  let held = enqueue(&ground, &run, "job", async |_, _| Ok(())).await.unwrap();

  assert!(!admitted(&ground, &held).await, "its supervisor's provider has no quota: it waits");

  deepseek_balance(&ground, 5.0).await;

  assert!(admitted(&ground, &held).await);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_key_answers_its_standing_request_and_is_spent_once_the_request_is_given_back() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let first = ask(&ground, "worker", "again").await;
  let again = ask(&ground, "worker", "again").await;

  assert_eq!(again.token, first.token, "asked again while it stands, the key answers its request");

  assert!(finish(&ground.db, &ground.events, &first.token, "done", false).await.unwrap());

  let spent = enqueue(&ground, &asked("worker", "again"), "job", async |_, _| Ok(())).await;

  assert!(spent.is_err(), "a key whose request was given back opens nothing: {spent:?}");
  assert_ne!(
    ask(&ground, "worker", "afresh").await.token,
    first.token,
    "a new key opens a new request"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bound_lease_is_given_back_only_once_its_child_was_seen_standing_and_then_is_not() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;

  let held = ask(&ground, "worker", "bound").await;

  assert!(admitted(&ground, &held).await);
  dispatch::bind(
    &ground.db,
    &ground.events,
    &dispatch::request(&ground.db, &held.token).await.unwrap().unwrap(),
    "unit-one",
    None,
  )
  .await
  .unwrap();

  let nothing = std::collections::HashSet::new();
  let standing: std::collections::HashSet<String> = ["unit-one".to_owned()].into();

  // A child never seen standing is not read as ended, however long it takes to come up.
  let leases = dispatch::bound_leases(&ground.db).await.unwrap();

  assert_eq!(leases, vec![(held.token.clone(), "unit-one".to_owned(), false)]);
  assert_eq!(dispatch::release_ended(&ground.db, &ground.events, &leases, &nothing).await.unwrap(), 0);
  assert_eq!(dispatch::request(&ground.db, &held.token).await.unwrap().unwrap().state, "active");

  // Seen standing, then gone: given back, as the line's on evidence its run ended.
  let leases = dispatch::bound_leases(&ground.db).await.unwrap();

  assert_eq!(
    dispatch::release_ended(&ground.db, &ground.events, &leases, &standing).await.unwrap(),
    0
  );

  let leases = dispatch::bound_leases(&ground.db).await.unwrap();

  assert!(leases[0].2, "marked seen");
  assert_eq!(dispatch::release_ended(&ground.db, &ground.events, &leases, &nothing).await.unwrap(), 1);

  let ended = dispatch::request(&ground.db, &held.token).await.unwrap().unwrap();

  assert_eq!(
    (ended.state.as_str(), ended.ended_by.as_deref()),
    ("released", Some(dispatch::BY_RUN_ENDED))
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lease_bound_to_a_role_session_ends_with_that_session() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;

  let held = ask(&ground, "worker", "session").await;

  assert!(admitted(&ground, &held).await);
  dispatch::bind_session(&ground.db, &ground.events, &held.token, "worker@run:7")
    .await
    .unwrap();

  // A later binding naming the run keeps the lease on its session and the name as the run's.
  let now = dispatch::request(&ground.db, &held.token).await.unwrap().unwrap();
  let bound = dispatch::bind(&ground.db, &ground.events, &now, "research-7-review", None)
    .await
    .unwrap();

  assert_eq!(bound.child_id.as_deref(), Some("worker@run:7"));
  assert_eq!(bound.session.as_deref(), Some("worker@run:7"));
  assert_eq!(bound.agent_name.as_deref(), Some("research-7-review"));

  // The pass reads no session-bound lease against units.
  assert!(dispatch::bound_leases(&ground.db).await.unwrap().is_empty());

  assert_eq!(
    dispatch::release_sessions(&ground.db, &ground.events, &["worker@run:7".to_owned()], "ended")
      .await
      .unwrap(),
    1
  );

  let ended = dispatch::request(&ground.db, &held.token).await.unwrap().unwrap();

  assert_eq!(
    (ended.state.as_str(), ended.ended_by.as_deref()),
    ("released", Some(dispatch::BY_RUN_ENDED))
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_owner_s_release_gives_back_everything_it_holds_and_nothing_of_another_owner() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;
  scratch
    .run(r#"UPDATE "roles" SET "max_concurrent" = 1 WHERE "name" = 'worker'"#, vec![])
    .await;
  crate::support::rows::reseated(scratch.db()).await;

  let mut first = asked("worker", "first");
  let mut second = asked("worker", "second");

  first.owner = "restarting".to_owned();
  second.owner = "restarting".to_owned();

  let first = enqueue(&ground, &first, "job", async |_, _| Ok(())).await.unwrap();
  let second = enqueue(&ground, &second, "job", async |_, _| Ok(())).await.unwrap();
  let other = ask(&ground, "worker", "other").await;

  assert!(admitted(&ground, &first).await);
  dispatch::bind_session(&ground.db, &ground.events, &first.token, "worker@run:9")
    .await
    .unwrap();

  let done = dispatch::release_owner(&ground.db, &ground.events, "restarting").await.unwrap();

  assert_eq!((done.released, done.cancelled), (1, 1));
  assert_eq!(done.sessions, vec!["worker@run:9".to_owned()]);

  for token in [&first.token, &second.token] {
    let ended = dispatch::request(&ground.db, token).await.unwrap().unwrap();

    assert_eq!(ended.ended_by.as_deref(), Some(dispatch::BY_OWNER));
  }

  assert_eq!(dispatch::request(&ground.db, &other.token).await.unwrap().unwrap().state, "waiting");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_slot_asked_for_a_model_alone_goes_out_on_the_provider_its_role_s_order_names_it_under() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let role = dispatch::role_of(&ground.db, "worker").await.unwrap();
  let entry = role.order.first().unwrap().clone();

  assert_eq!(
    dispatch::provider_for_model(&ground, "worker", &entry.model).await.unwrap(),
    entry.provider,
    "the entry naming that model"
  );

  // A model no entry names goes out on the role's active provider, else its order's first.
  let provider = dispatch::provider_for_model(&ground, "worker", "no-such-model").await.unwrap();

  assert!(role.order.entries.iter().any(|one| one.provider == provider), "{provider}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_slot_carries_three_sessions_in_turn_and_is_given_back_with_the_last() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;

  let held = ask(&ground, "worker", "rounds").await;
  let read = async |token: &str| dispatch::request(&ground.db, token).await.unwrap().unwrap();

  assert!(admitted(&ground, &held).await);

  // The first answer's session, a run named on it by a binding.
  dispatch::bind_session(&ground.db, &ground.events, &held.token, "worker@run:1")
    .await
    .unwrap();
  dispatch::bind(&ground.db, &ground.events, &read(&held.token).await, "research-1-answer", None)
    .await
    .unwrap();
  assert_eq!(read(&held.token).await.agent_name.as_deref(), Some("research-1-answer"));

  // The round's session starts on the slot before the first ends: the lease moves onto it, and the first
  // session's end no longer gives it back.
  dispatch::bind_session(&ground.db, &ground.events, &held.token, "worker@run:2")
    .await
    .unwrap();

  let moved = read(&held.token).await;

  assert_eq!(
    (moved.child_id.as_deref(), moved.session.as_deref(), moved.agent_name.as_deref()),
    (Some("worker@run:2"), Some("worker@run:2"), None),
    "the run named on the session before stays with it"
  );
  assert_eq!(
    dispatch::release_sessions(&ground.db, &ground.events, &["worker@run:1".to_owned()], "ended")
      .await
      .unwrap(),
    0
  );

  // The review's session, the same way.
  dispatch::bind_session(&ground.db, &ground.events, &held.token, "worker@run:3")
    .await
    .unwrap();
  assert_eq!(
    dispatch::release_sessions(&ground.db, &ground.events, &["worker@run:2".to_owned()], "ended")
      .await
      .unwrap(),
    0
  );
  assert_eq!(read(&held.token).await.state, "active", "still held across three sessions");

  // Starting the same session again keeps what is named on it.
  dispatch::bind(&ground.db, &ground.events, &read(&held.token).await, "research-1-review", None)
    .await
    .unwrap();
  dispatch::bind_session(&ground.db, &ground.events, &held.token, "worker@run:3")
    .await
    .unwrap();
  assert_eq!(read(&held.token).await.agent_name.as_deref(), Some("research-1-review"));

  // The last session's end gives the lease back.
  assert_eq!(
    dispatch::release_sessions(&ground.db, &ground.events, &["worker@run:3".to_owned()], "ended")
      .await
      .unwrap(),
    1
  );

  let ended = read(&held.token).await;

  assert_eq!(
    (ended.state.as_str(), ended.ended_by.as_deref()),
    ("released", Some(dispatch::BY_RUN_ENDED))
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lease_let_in_whose_owner_never_starts_a_session_stands_until_its_owner_releases_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;

  let mut lone = asked("worker", "never-started");

  lone.owner = "research:185".to_owned();

  let held = enqueue(&ground, &lone, "search", async |_, _| Ok(())).await.unwrap();

  assert!(admitted(&ground, &held).await);

  // Bound to nothing, the line's pass reads no end of it: it stands.
  assert!(dispatch::bound_leases(&ground.db).await.unwrap().is_empty());
  assert_eq!(dispatch::request(&ground.db, &held.token).await.unwrap().unwrap().state, "active");

  let done = dispatch::release_owner(&ground.db, &ground.events, "research:185").await.unwrap();

  assert_eq!((done.released, done.cancelled), (1, 0));
  assert!(done.sessions.is_empty(), "no session was bound to it");

  let ended = dispatch::request(&ground.db, &held.token).await.unwrap().unwrap();

  assert_eq!(
    (ended.state.as_str(), ended.ended_by.as_deref()),
    ("released", Some(dispatch::BY_OWNER))
  );
  assert_eq!(used(&ground.db, "worker", None).await.unwrap(), 0, "its slot is free again");
}

// The coder role is seeded with an empty order, so a run of it follows the supervisor asking it; an order the
// owner sets on the role stands over the supervisor's road, and a slot of a role with an empty order that
// no supervisor asks has no road and is refused (docs/role-slots.md, 「A role that follows its supervisor」).
#[tokio::test(flavor = "multi_thread")]
async fn a_run_follows_its_supervisor_until_the_owner_sets_the_role_an_order() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  scratch
    .run(
      r#"INSERT INTO "supervisors" ("name", "session_scope", "turn_shape", "provider", "model", "effort",
           "created_at", "updated_at")
         VALUES ('chat', 'agent', 'window', 'deepseek', 'deepseek-flash', 'max', 0, 0)"#,
      vec![],
    )
    .await;

  let seeded_role = agency_server::agent_roles::named(&ground.db, "coder").await.unwrap().unwrap();

  assert!(seeded_role.order.follows_supervisor(), "the coder's order is seeded empty");

  let followed = dispatch::fixed_road(&ground.db, "coder", Some("chat"))
    .await
    .unwrap()
    .expect("the supervisor's road");

  assert_eq!(
    (followed.provider.as_str(), followed.model.as_str(), followed.effort.as_deref()),
    ("deepseek", "deepseek-flash", Some("max")),
    "a supervisor with no turn yet goes by its order's first"
  );

  let nobody = dispatch::fixed_road(&ground.db, "coder", None).await;

  assert!(
    nobody.is_err_and(|why| why.contains("no supervisor asked")),
    "an empty order needs its asker"
  );

  // The entry a turn of a following role goes out on, on any road, is the asking supervisor's for the
  // provider; a run of a following role that names no supervisor is refused.
  let entry = agency_server::order::row_entry(&ground.db, "coder", "deepseek", Some("chat"))
    .await
    .unwrap();

  assert_eq!((entry.model.as_str(), entry.effort.as_deref()), ("deepseek-flash", Some("max")));
  assert!(
    agency_server::order::row_entry(&ground.db, "coder", "deepseek", None)
      .await
      .is_err_and(|why| why.contains("follows the supervisor")),
    "a following role's run names its supervisor"
  );

  scratch
    .run(
      r#"UPDATE "roles" SET "provider_order" = '[{"provider":"codex","model":"gpt-5.6-terra","effort":"high"}]'
         WHERE "name" = 'coder'"#,
      vec![],
    )
    .await;
  crate::support::rows::reseated(scratch.db()).await;

  assert_eq!(
    dispatch::fixed_road(&ground.db, "coder", Some("chat")).await.unwrap(),
    None,
    "the owner's order stands over the supervisor's road: the slot walks the role's order"
  );
}

// A session started on a following role's slot carries the supervisor the slot names as asked by, and the step
// each of its turns takes on a provider's road other than Claude Code is read from that supervisor's entry
// (docs/role-slots.md, 「A role that follows its supervisor」).
#[tokio::test(flavor = "multi_thread")]
async fn a_following_roles_session_goes_out_on_its_asking_supervisors_step_on_a_providers_road() {
  use agency_server::chat_stand_in::road_step;
  use agency_server::control::session_ops::on_slot;
  use agency_server::lines::worker::Reasoning;
  use agency_server::road::Road;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;
  scratch
    .run(
      r#"INSERT INTO "supervisors" ("name", "session_scope", "turn_shape", "provider", "model", "effort",
           "created_at", "updated_at")
         VALUES ('chat', 'agent', 'window', 'deepseek', 'deepseek-flash', 'high', 0, 0)"#,
      vec![],
    )
    .await;

  let fixed = dispatch::fixed_road(&ground.db, "coder", Some("chat")).await.unwrap();
  let run = Asked {
    request_key: "follows:session".to_owned(),
    owner: "follows:session".to_owned(),
    wanted: Wanted {
      role: "coder",
      path: Some("/scratch/chat"),
      fixed,
      agent: None,
    },
    asked_by: Some("chat"),
    ..Asked::default()
  };
  let held = enqueue(&ground, &run, "job", async |_, _| Ok(())).await.unwrap();
  let active = acquire(&ground, &held).await.unwrap().expect("let in on the supervisor's road");
  let mut apart = agent_session::Apart::default();

  on_slot(&ground.db, &serde_json::json!({ "slot": active.token }), &mut apart)
    .await
    .unwrap();
  assert_eq!(apart.asked_by.as_deref(), Some("chat"), "the session carries its slot's asker");
  assert_eq!(apart.provider, "deepseek");

  let taken = Road {
    provider: apart.provider.clone(),
    model: apart.model.clone(),
    account: None,
  };
  let machine = ground.machine();
  let reasoning = Reasoning::default();
  let step = road_step(&ground, &machine, "coder", &taken, &reasoning, apart.asked_by.as_deref())
    .await
    .expect("the supervisor's entry on its road");

  assert_eq!((step.provider.as_str(), step.model.as_str()), ("deepseek", "deepseek-flash"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_reads_its_prompt_bundles_and_threshold_off_the_agent_it_is_let_in_on() {
  use agency_server::role_agents::{bundles_at, prompt_at, seated_at};

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let db = &ground.db;

  deepseek_balance(&ground, 5.0).await;

  // The role's first agent lays a prompt, bundles and a threshold of its own over its role's.
  let first = agency_server::role_agents::of_role(db, "worker").await.unwrap().remove(0);

  scratch
    .run(
      r#"UPDATE "role_agents" SET "prompt" = 'Work as the first seat.', "bundles" = '["web.read"]',
           "fold_ratio" = 0.4 WHERE "name" = $1"#,
      vec![first.name.as_str().into()],
    )
    .await;

  let role_prompt = agency_server::agent_roles::prompt_of(db, "worker").await.unwrap();
  let role_bundles = agency_server::agent_roles::bundles_of(db, "worker").await.unwrap();

  // No run stands anywhere: the role's own preset.
  assert_eq!(seated_at(db, Some("worker"), Some("/checkouts/one")).await.unwrap(), None);
  assert_eq!(prompt_at(db, "worker", Some("/checkouts/one")).await.unwrap(), role_prompt);
  assert_eq!(bundles_at(db, "worker", Some("/checkouts/one")).await.unwrap(), role_bundles);

  // A run let in at a folder sits on the role's first free agent, and reads that agent.
  let held = ask(&ground, "worker", "one").await;

  assert!(admitted(&ground, &held).await);

  let seat = seated_at(db, Some("worker"), Some("/checkouts/one")).await.unwrap().unwrap();

  assert_eq!(seat.name, first.name);
  assert_eq!(
    prompt_at(db, "worker", Some("/checkouts/one")).await.unwrap().as_deref(),
    Some("Work as the first seat.")
  );
  assert_eq!(bundles_at(db, "worker", Some("/checkouts/one/")).await.unwrap(), ["web.read"]);
  // The folder alone names the seat too, whatever its role, and a run that names no folder takes its role's
  // only active request.
  assert_eq!(seated_at(db, None, Some("/checkouts/one")).await.unwrap().unwrap().name, first.name);
  assert_eq!(seated_at(db, Some("worker"), None).await.unwrap().unwrap().name, first.name);
  // Another folder, and another role at the same folder, name no seat.
  assert_eq!(seated_at(db, Some("worker"), Some("/checkouts/two")).await.unwrap(), None);
  assert_eq!(seated_at(db, Some("coder"), Some("/checkouts/one")).await.unwrap(), None);
  assert_eq!(prompt_at(db, "worker", Some("/checkouts/two")).await.unwrap(), role_prompt);

  // The job's threshold is its agent's where the agent sets one, and its role's elsewhere.
  let role_ratio = agency_server::agent_roles::named(db, "worker").await.unwrap().unwrap().fold_ratio;

  assert_eq!(
    agency_server::worker_session::fold::worker_fold_ratio(
      &ground,
      "deepseek",
      None,
      Some("/checkouts/one")
    )
    .await,
    0.4
  );
  assert_eq!(
    agency_server::worker_session::fold::worker_fold_ratio(
      &ground,
      "deepseek",
      None,
      Some("/checkouts/two")
    )
    .await,
    role_ratio
  );

  // The slot given back, nothing stands on the agent, and the role's own preset is read again.
  assert!(finish(db, &ground.events, &held.token, "done", false).await.unwrap());
  assert_eq!(seated_at(db, Some("worker"), Some("/checkouts/one")).await.unwrap(), None);
  assert_eq!(bundles_at(db, "worker", Some("/checkouts/one")).await.unwrap(), role_bundles);
}

// A role that holds no agent is given its agents from what its row names (docs/role-agents.md, 「The roles the
// policy seeds」): as many as its count, each on the first road of its order whose program carries the role's
// line, or on the provider forced on it.
#[tokio::test(flavor = "multi_thread")]
async fn a_role_is_given_its_agents_on_the_first_road_of_its_order_that_carries_its_line() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let db = &ground.db;
  let order = r#"[{"provider":"deepseek","model":"deepseek-flash","effort":"high"},
    {"provider":"registered","model":"flash-next-coder"},
    {"provider":"claude-code","model":"claude-opus-5-5","effort":"high"}]"#;

  for role in ["worker", "voyager", "seeker", "translator"] {
    scratch
      .run(
        r#"UPDATE "roles" SET "provider_order" = $1, "max_concurrent" = 2, "active_provider" = NULL,
             "active_model" = NULL WHERE "name" = $2"#,
        vec![order.into(), role.into()],
      )
      .await;
  }

  scratch
    .run(
      r#"UPDATE "roles" SET "provider_order" = $1, "max_concurrent" = 1, "active_provider" = 'claude-code',
           "active_model" = 'claude-sonnet-5' WHERE "name" = 'producer'"#,
      vec![order.into()],
    )
    .await;
  crate::support::rows::reseated(db).await;

  let mut roads = vec![];

  for role in ["worker", "voyager", "seeker", "translator", "producer"] {
    let agents = agency_server::role_agents::of_role(db, role).await.unwrap();
    let first = agents.first().unwrap();

    assert!(
      agents
        .iter()
        .all(|agent| agent.provider == first.provider && agent.model == first.model)
    );
    roads.push((
      role,
      agents.len(),
      first.provider.clone().unwrap(),
      first.model.clone().unwrap(),
      first.effort.clone(),
    ));
  }

  assert_eq!(
    roads,
    [
      // The worker line and the search line stand on the order's first road.
      ("worker", 2, "deepseek".to_owned(), "deepseek-flash".to_owned(), Some("high".to_owned())),
      (
        "voyager",
        2,
        "deepseek".to_owned(),
        "deepseek-flash".to_owned(),
        Some("high".to_owned())
      ),
      // The browse line is carried by Claude Code alone, and the translate line by no road before the registry.
      (
        "seeker",
        2,
        "claude-code".to_owned(),
        "claude-opus-5-5".to_owned(),
        Some("high".to_owned())
      ),
      ("translator", 2, "registered".to_owned(), "flash-next-coder".to_owned(), None),
      // A forced provider stands on its forced model, at the effort the order names for it.
      (
        "producer",
        1,
        "claude-code".to_owned(),
        "claude-sonnet-5".to_owned(),
        Some("high".to_owned())
      ),
    ]
  );
}

// A role that holds an agent on a road of its own beside agents that name none (docs/role-agents.md, 「Which
// agent a run takes」): a run a supervisor asks takes the first free agent in the one order, an agent that
// names no road on that supervisor's road; a run no supervisor asks passes those agents over; and a run that
// names a road no agent stands on is let in on an agent that names none.
#[tokio::test(flavor = "multi_thread")]
async fn a_role_of_both_kinds_of_agent_seats_a_supervisors_run_in_one_order() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let db = &ground.db;

  deepseek_balance(&ground, 5.0).await;
  scratch
    .run(
      r#"INSERT INTO "supervisors" ("name", "session_scope", "turn_shape", "provider", "model", "effort",
           "created_at", "updated_at")
         VALUES ('chat', 'agent', 'window', 'deepseek', 'deepseek-flash', 'max', 0, 0)"#,
      vec![],
    )
    .await;
  // The coder's second agent stands on a road of its own; its first and third name none.
  scratch
    .run(
      r#"UPDATE "role_agents" SET "provider" = 'deepseek', "model" = 'deepseek-flash', "effort" = 'low'
         WHERE "name" = 'coder-02'"#,
      vec![],
    )
    .await;

  let ask =
    async |key: &str, asked_by: Option<&'static str>, fixed: Option<agency_server::order::Entry>| {
      let asked = Asked {
        request_key: format!("mixed:{key}"),
        owner: format!("mixed:{key}"),
        wanted: Wanted {
          role: "coder",
          path: Some("/scratch/mixed"),
          fixed,
          agent: None,
        },
        asked_by,
        ..Asked::default()
      };
      let held = enqueue(&ground, &asked, "job", async |_, _| Ok(())).await.unwrap();

      acquire(&ground, &held).await.unwrap()
    };
  let seat = |request: &Option<dispatch::Request>| {
    request
      .as_ref()
      .map(|let_in| (let_in.seat.clone().unwrap_or_default(), let_in.effort.clone()))
  };

  // No supervisor asked: the agents that name no road are passed over, and the run sits on the one that does.
  let alone = ask("alone", None, None).await;

  assert_eq!(seat(&alone), Some(("coder-02".to_owned(), Some("low".to_owned()))));
  // A second run nobody asked waits for that agent, though two agents stand free.
  assert_eq!(seat(&ask("alone-again", None, None).await), None);

  // A supervisor's runs take the free agents in order, each on the supervisor's own road.
  let first = ask("first", Some("chat"), None).await;
  let third = ask("third", Some("chat"), None).await;

  assert_eq!(seat(&first), Some(("coder-01".to_owned(), Some("max".to_owned()))));
  assert_eq!(seat(&third), Some(("coder-03".to_owned(), Some("max".to_owned()))));
  assert_eq!(first.as_ref().unwrap().model, "deepseek-flash");

  // With coder-02 free again, a supervisor's run takes it on the agent's own road, in its place in the order.
  assert!(finish(db, &ground.events, &alone.unwrap().token, "done", false).await.unwrap());
  assert!(finish(db, &ground.events, &first.unwrap().token, "done", false).await.unwrap());

  // The waiting run nobody asked is ahead in the line and takes coder-02; the supervisor's next takes coder-01.
  let waited = dispatch::requests(db, &["waiting"]).await.unwrap();
  let waited = acquire(&ground, &waited[0]).await.unwrap();

  assert_eq!(seat(&waited), Some(("coder-02".to_owned(), Some("low".to_owned()))));
  assert_eq!(
    seat(&ask("again", Some("chat"), None).await),
    Some(("coder-01".to_owned(), Some("max".to_owned())))
  );

  // A run naming a road no agent stands on is let in on an agent that names none; with none of those free it
  // waits, and it is never refused while the role holds one.
  let pinned = agency_server::order::Entry {
    provider: "claude-code".to_owned(),
    model: "claude-opus-5".to_owned(),
    effort: Some("high".to_owned()),
    thinking: None,
    answer_tokens: None,
    advisor: None,
  };

  assert_eq!(seat(&ask("pinned", None, Some(pinned)).await), None);
}

// Whether a run keeps its session is its agent's own (docs/role-agents.md, 「What an agent row holds」): a run
// reads the sessions setting of the agent it is let in on.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_keeps_its_session_as_its_agent_says() {
  use agency_server::role_agents::sessions_at;
  use policy::roles::Sessions;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let db = &ground.db;

  deepseek_balance(&ground, 5.0).await;
  // The worker's first agent keeps no session; its others keep theirs.
  scratch
    .run(r#"UPDATE "role_agents" SET "sessions" = 'fresh' WHERE "name" = 'worker-01'"#, vec![])
    .await;

  // No run stands at the folder, so no agent can be told and nothing is kept.
  assert_eq!(sessions_at(db, "worker", Some("/checkouts/one")).await.unwrap(), Sessions::Fresh);

  let first = ask(&ground, "worker", "one").await;
  let second = Asked {
    wanted: Wanted {
      role: "worker",
      path: Some("/checkouts/two"),
      fixed: None,
      agent: None,
    },
    ..asked("worker", "two")
  };
  let second = enqueue(&ground, &second, "job", async |_, _| Ok(())).await.unwrap();

  assert!(admitted(&ground, &first).await && admitted(&ground, &second).await);
  assert_eq!(sessions_at(db, "worker", Some("/checkouts/one")).await.unwrap(), Sessions::Fresh);
  assert_eq!(sessions_at(db, "worker", Some("/checkouts/two")).await.unwrap(), Sessions::Keep);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_names_an_agent_is_let_in_on_that_agent_alone_and_waits_for_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let db = &ground.db;

  deepseek_balance(&ground, 5.0).await;

  let agents = agency_server::role_agents::of_role(db, "worker").await.unwrap();
  let second = agents[1].name.clone();
  let wanted = |key: &str| {
    let mut asked = asked("worker", key);

    asked.wanted.agent = Some(second.as_str());
    asked
  };

  // The first ask naming the second agent is let in on it, passing the first agent over.
  let one = enqueue(&ground, &wanted("one"), "job", async |_, _| Ok(())).await.unwrap();

  assert!(admitted(&ground, &one).await, "the named agent is free");

  let let_in = dispatch::request(db, &one.token).await.unwrap().unwrap();

  assert_eq!(let_in.agent_wanted.as_deref(), Some(second.as_str()));
  assert_eq!(
    let_in.seat.as_deref(),
    Some(second.as_str()),
    "let in on the agent it named, not the first"
  );

  // A second ask naming the same agent waits, though the role has free agents.
  let two = enqueue(&ground, &wanted("two"), "job", async |_, _| Ok(())).await.unwrap();

  assert!(!admitted(&ground, &two).await, "the named agent carries a run, so the ask waits");
  assert!(
    admitted(&ground, &ask(&ground, "worker", "three").await).await,
    "an ask naming none takes another agent"
  );

  // Given back, the named agent takes the waiting ask.
  finish(db, &ground.events, &one.token, "ended", true).await.unwrap();
  assert!(admitted(&ground, &two).await);
  assert_eq!(
    dispatch::request(db, &two.token).await.unwrap().unwrap().seat.as_deref(),
    Some(second.as_str())
  );

  // An agent the role does not hold is an error.
  let mut stranger = asked("worker", "four");

  stranger.wanted.agent = Some("nobody-01");
  assert!(enqueue(&ground, &stranger, "job", async |_, _| Ok(())).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_accounts_runs_take_its_own_agents_and_count_against_its_own_cap() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;
  scratch
    .run(
      r#"INSERT INTO "supervisors" ("name", "session_scope", "turn_shape", "provider", "model", "effort",
           "created_at", "updated_at", "owner_account")
         VALUES ('secretary-a', 'agent', 'window', 'deepseek', 'deepseek-flash', 'high', 0, 0, 7)"#,
      vec![],
    )
    .await;

  let of_account = |key: &'static str| Asked {
    asked_by: Some("secretary-a"),
    ..asked("worker", key)
  };
  let first = enqueue(&ground, &of_account("account-one"), "job", async |_, _| Ok(())).await.unwrap();

  assert_eq!(first.account.as_deref(), Some("7"), "a slot belongs to its asking supervisor's account");
  assert!(
    !admitted(&ground, &first).await,
    "an account with no agent of the role runs nothing of it"
  );

  // One seat of the role for the account, on the road the deployment's workers stand on.
  let mut seat = agency_server::role_agents::of_role(&ground.db, "worker").await.unwrap().remove(0);

  seat.name = "worker-account-7".into();
  seat.owner_account = Some(7);
  agency_server::role_agents::write(&ground.db, &seat).await.unwrap();

  // The deployment's three seats fill with its own runs, and the account's run still takes its own.
  let mut deployment = vec![];

  for key in ["one", "two", "three"] {
    deployment.push(ask(&ground, "worker", key).await);
  }

  for request in &deployment {
    assert!(admitted(&ground, request).await, "the deployment's cap is its own seats'");
  }

  assert!(admitted(&ground, &first).await, "the account's run takes the account's seat");

  let let_in = dispatch::request(&ground.db, &first.token).await.unwrap().unwrap();

  assert_eq!(let_in.seat.as_deref(), Some("worker-account-7"));
  assert_eq!(used(&ground.db, "worker", Some("7")).await.unwrap(), 1);
  assert_eq!(used(&ground.db, "worker", None).await.unwrap(), 3);

  // A second run of the account waits on the account's cap of one, whatever the deployment's seats do.
  let second = enqueue(&ground, &of_account("account-two"), "job", async |_, _| Ok(())).await.unwrap();

  assert!(!admitted(&ground, &second).await, "the account's cap is its own agents switched on");
  assert!(finish(&ground.db, &ground.events, &deployment[0].token, "done", false).await.unwrap());
  assert!(
    !admitted(&ground, &second).await,
    "a deployment seat given back is no seat of the account's"
  );
  assert!(finish(&ground.db, &ground.events, &first.token, "done", false).await.unwrap());
  assert!(
    admitted(&ground, &second).await,
    "the account's own seat given back lets its next run in"
  );
}

// The chain as the stand-in answers it for the seats test: account 7 stands in unit 4 under unit 3, and no
// other account stands under any unit.
async fn chain_service() -> (String, tokio::task::JoinHandle<()>) {
  let app = axum::Router::new().route(
    "/chain/kinds/{key}/resolve",
    axum::routing::get(
      |axum::extract::Query(asked): axum::extract::Query<std::collections::HashMap<String, String>>| async move {
        let path: Vec<&str> = match asked.get("person").map(String::as_str) {
          Some("7") => vec!["platform", "unit:3", "unit:4", "person:7"],
          _ => vec!["platform"],
        };

        axum::Json(serde_json::json!({ "value": true, "layers": [], "path": path }))
      },
    ),
  );
  let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let serving = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (address, serving)
}

// A unit's seat runs its people's work (docs/role-agents.md, 「A unit's seats run their people's work」): the
// ask of an account whose path names the unit takes the unit's seat and counts against that account's cap, an
// account whose path names no unit is left with none of it, and the seat given back lets the next of its
// people in.
#[tokio::test(flavor = "multi_thread")]
async fn a_person_on_a_unit_s_path_takes_the_unit_s_seat_and_counts_against_their_own_cap() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (address, serving) = chain_service().await;
  let vars = std::collections::BTreeMap::from([
    ("WEB_ADDRESS".to_owned(), address),
    ("WEB_TOKEN".to_owned(), "chain".to_owned()),
  ]);
  let ground = seeded_with(scratch.db(), home.path(), &vars).await;

  deepseek_balance(&ground, 5.0).await;

  // Two secretaries: one of the person whose path names unit 4, one of an account standing under no unit.
  scratch
    .run(
      r#"INSERT INTO "supervisors" ("name", "session_scope", "turn_shape", "provider", "model", "effort",
           "created_at", "updated_at", "owner_account")
         VALUES ('secretary-path', 'agent', 'window', 'deepseek', 'deepseek-flash', 'high', 0, 0, 7),
                ('secretary-other', 'agent', 'window', 'deepseek', 'deepseek-flash', 'high', 0, 0, 9)"#,
      vec![],
    )
    .await;

  // One seat of the role filed under unit 4.
  let mut seat = agency_server::role_agents::of_role(&ground.db, "worker").await.unwrap().remove(0);

  seat.name = "worker-unit-4".into();
  seat.owner_unit = Some(4);
  agency_server::role_agents::write(&ground.db, &seat).await.unwrap();

  let of = |asked_by: &'static str, key: &'static str| Asked {
    asked_by: Some(asked_by),
    ..asked("worker", key)
  };
  let first = enqueue(&ground, &of("secretary-path", "unit-seat-one"), "job", async |_, _| Ok(()))
    .await
    .unwrap();

  assert_eq!(first.account.as_deref(), Some("7"));
  assert!(
    admitted(&ground, &first).await,
    "the unit's seat stands in the pool of a person whose path names the unit"
  );

  let let_in = dispatch::request(&ground.db, &first.token).await.unwrap().unwrap();

  assert_eq!(
    let_in.seat.as_deref(),
    Some("worker-unit-4"),
    "the run stands under the seat it was let in on"
  );
  assert_eq!(used(&ground.db, "worker", Some("7")).await.unwrap(), 1);
  assert_eq!(
    used(&ground.db, "worker", None).await.unwrap(),
    0,
    "the unit's seat counts against no deployment"
  );

  // The seat holds one run: the person's next ask waits on a cap of one seat.
  let second = enqueue(&ground, &of("secretary-path", "unit-seat-two"), "job", async |_, _| Ok(()))
    .await
    .unwrap();

  assert!(!admitted(&ground, &second).await, "the cap counts the unit's seat the account may take");

  // An account whose path names no unit takes none of the unit's seats.
  let other = enqueue(&ground, &of("secretary-other", "off-path"), "job", async |_, _| Ok(()))
    .await
    .unwrap();

  assert!(
    !admitted(&ground, &other).await,
    "a unit's seat runs no account whose path does not name the unit"
  );

  // The seat given back lets the waiting run of its people in, and still no other account's.
  assert!(finish(&ground.db, &ground.events, &first.token, "done", false).await.unwrap());
  assert!(
    admitted(&ground, &second).await,
    "the unit's seat given back lets its people's next run in"
  );
  assert!(!admitted(&ground, &other).await);

  serving.abort();
}

// One role's asks are judged one at a time (docs/role-slots.md, 「The line」): two asks woken at once by the one
// agent freeing never both take it; one is let in and the other waits.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_asks_woken_at_once_never_both_take_the_one_free_agent() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = std::sync::Arc::new(seeded(scratch.db(), home.path()).await);

  deepseek_balance(&ground, 5.0).await;
  crate::support::rows::seats(scratch.db(), "producer", &[("deepseek", "deepseek-flash")]).await;

  let mut holding = ask(&ground, "producer", "holding").await;

  assert!(admitted(&ground, &holding).await);

  for round in 0..30 {
    let first = ask(&ground, "producer", &format!("first-{round}")).await;
    let second = ask(&ground, "producer", &format!("second-{round}")).await;

    assert!(finish(&ground.db, &ground.events, &holding.token, "done", false).await.unwrap());

    // Both woken at once, each judged on a thread of its own.
    let woken = |held: dispatch::Request| {
      let ground = ground.clone();

      tokio::spawn(async move {
        let now = dispatch::request(&ground.db, &held.token).await.unwrap().unwrap();

        acquire(&ground, &now).await.unwrap().is_some()
      })
    };
    let (one, other) = tokio::join!(woken(first.clone()), woken(second.clone()));
    let (one, other) = (one.unwrap(), other.unwrap());

    assert!(!(one && other), "round {round}: both asks took the one agent");
    assert_eq!(agency_server::role_agents::carrying(&ground.db, "producer").await.unwrap().len(), 1);

    let (taken, left) = if one { (first, second) } else { (second, first) };

    assert!(one || other || admitted(&ground, &taken).await, "round {round}: neither was let in");
    assert!(finish(&ground.db, &ground.events, &left.token, "gone", true).await.unwrap());
    holding = taken;
  }
}

// The judging of an ask waits for its role's row (docs/role-slots.md, 「The line」): while another transaction
// holds that row the ask is not judged, and it is judged once the row is let go.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_ask_is_judged_only_while_it_holds_its_role() {
  use sea_orm::TransactionTrait;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = std::sync::Arc::new(seeded(scratch.db(), home.path()).await);

  deepseek_balance(&ground, 5.0).await;
  crate::support::rows::seats(scratch.db(), "producer", &[("deepseek", "deepseek-flash")]).await;

  let held = ground.db.begin().await.unwrap();

  store::run(&held, r#"SELECT 1 FROM "roles" WHERE "name" = 'producer' FOR UPDATE"#, vec![])
    .await
    .unwrap();

  let asking = {
    let ground = ground.clone();

    tokio::spawn(async move {
      let request = enqueue(&ground, &asked("producer", "judged"), "job", async |_, _| Ok(()))
        .await
        .unwrap();

      acquire(&ground, &request).await.unwrap().is_some()
    })
  };

  tokio::time::sleep(std::time::Duration::from_millis(400)).await;
  assert!(!asking.is_finished(), "the ask was judged while another held its role");

  held.commit().await.unwrap();
  assert!(asking.await.unwrap(), "let go, the role's free agent takes the ask");
}

// A slot of a role whose agents follow their asker while naming roads of their own (docs/role-agents.md, 「An agent
// that follows its asker」): the transliterator, seeded so, goes out on its asking supervisor's road; on its agent's
// own road where no supervisor asks or the asker's road cannot answer, as OpenRouter's, whose program does not
// carry the search line, cannot; and on its own road alone once the agent's own setting says it does not follow.
// A turn's entry is the asker's on the asker's provider and the agent's own elsewhere.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_following_its_asker_takes_the_askers_road_and_falls_back_to_its_own() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  openrouter_road(home.path(), &[("paid", "vendor/a-model", None)]);

  // A second model on DeepSeek's road, the asking supervisor's, beside the flash model the agents stand on.
  let members = home.path().join("data").join("llm-gateway").join("members.json");
  let mut held: Vec<serde_json::Value> =
    serde_json::from_str(&std::fs::read_to_string(&members).unwrap()).unwrap();
  let mut other = held[0].clone();

  other["name"] = "deepseek-other".into();
  other["model"] = "deepseek-other".into();
  held.push(other);
  std::fs::write(&members, serde_json::to_string(&held).unwrap()).unwrap();

  provider_balance(&ground, "openrouter", 5.0).await;
  deepseek_balance(&ground, 5.0).await;
  scratch
    .run(
      r#"INSERT INTO "supervisors" ("name", "session_scope", "turn_shape", "provider", "model", "effort",
           "created_at", "updated_at")
         VALUES ('chat', 'agent', 'window', 'deepseek', 'deepseek-other', 'high', 0, 0),
                ('elsewhere', 'room', 'window', 'openrouter', 'paid', 'high', 0, 0)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"UPDATE "role_agents" SET "provider" = 'deepseek', "model" = 'deepseek-flash', "effort" = 'low',
           "enabled" = 1
         WHERE "role" = 'transliterator'"#,
      vec![],
    )
    .await;

  let role = agency_server::agent_roles::named(&ground.db, "transliterator").await.unwrap().unwrap();

  assert!(role.follows_asker, "the transliterator is seeded following its asker");

  let road_of = async |key: &str, asked_by: Option<&str>| {
    let run = Asked {
      request_key: format!("follows-asker:{key}"),
      owner: format!("follows-asker:{key}"),
      wanted: Wanted {
        role: "transliterator",
        path: None,
        fixed: None,
        agent: None,
      },
      asked_by,
      ..Asked::default()
    };
    let held = enqueue(&ground, &run, "voice", async |_, _| Ok(())).await.unwrap();

    assert!(admitted(&ground, &held).await, "{key}: let in");

    let running = dispatch::request(&ground.db, &held.token).await.unwrap().unwrap();

    assert!(finish(&ground.db, &ground.events, &held.token, "done", false).await.unwrap());

    (running.provider, running.model)
  };
  let own = ("deepseek".to_owned(), "deepseek-flash".to_owned());

  assert_eq!(
    road_of("asked", Some("chat")).await,
    ("deepseek".to_owned(), "deepseek-other".to_owned()),
    "a supervisor's ask goes out on that supervisor's road"
  );
  assert_eq!(
    road_of("unasked", None).await,
    own,
    "an ask no supervisor makes goes out on the agent's own road"
  );
  assert_eq!(
    road_of("elsewhere", Some("elsewhere")).await,
    own,
    "an asker's road that cannot answer gives way to the agent's own"
  );

  // The seat that gave way says both: the asker and its road with the sentence that road answered, and the own road
  // the request is let in on, in the line its admission logs.
  let run = Asked {
    request_key: "follows-asker:passed-over".into(),
    owner: "follows-asker:passed-over".into(),
    wanted: Wanted {
      role: "transliterator",
      path: None,
      fixed: None,
      agent: None,
    },
    asked_by: Some("elsewhere"),
    ..Asked::default()
  };
  let held = enqueue(&ground, &run, "voice", async |_, _| Ok(())).await.unwrap();
  let role = agency_server::agent_roles::named(&ground.db, "transliterator").await.unwrap().unwrap();
  let seats = dispatch::seats_for(&ground, &held, &role).await.unwrap();
  let seat = &seats[0];
  let why = seat.passed_over.clone().expect("the asker's road was passed over");

  assert_eq!(seat.road.as_ref().map(|road| road.model.as_str()), Ok("deepseek-flash"));
  assert!(
    why.starts_with("it follows elsewhere, whose road openrouter/paid cannot answer now: "),
    "{why}"
  );
  assert!(
    why.len() > "it follows elsewhere, whose road openrouter/paid cannot answer now: ".len(),
    "{why}"
  );
  assert!(admitted(&ground, &held).await);

  let running = dispatch::request(&ground.db, &held.token).await.unwrap().unwrap();
  let line = dispatch::fallback_line(&running, &seat.agent, &why);

  assert!(
    line.starts_with(&format!(
      "transliterator slot {}: let in on {}'s own road deepseek/deepseek-flash in place of its asker's; it follows \
       elsewhere, whose road openrouter/paid cannot answer now: ",
      held.token, seat.agent
    )),
    "{line}"
  );
  assert!(finish(&ground.db, &ground.events, &held.token, "done", false).await.unwrap());

  let entry = agency_server::order::row_entry(&ground.db, "transliterator", "deepseek", Some("chat"))
    .await
    .unwrap();

  assert_eq!(
    (entry.model.as_str(), entry.effort.as_deref()),
    ("deepseek-other", Some("high")),
    "the asker's entry"
  );

  let entry = agency_server::order::row_entry(&ground.db, "transliterator", "deepseek", Some("elsewhere"))
    .await
    .unwrap();

  assert_eq!(
    (entry.model.as_str(), entry.effort.as_deref()),
    ("deepseek-flash", Some("low")),
    "its own entry"
  );

  scratch
    .run(
      r#"UPDATE "role_agents" SET "follows_asker" = 0 WHERE "role" = 'transliterator'"#,
      vec![],
    )
    .await;

  assert_eq!(road_of("own", Some("chat")).await, own, "an agent's own setting stands over its role's");
}
