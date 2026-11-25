//! The provider order (docs/provider-order.md): a row's run goes out on the first provider of its own order
//! that has any quota left and can answer now, every provider it passes named with why, the quota read
//! against the window with the least left and none only where that window is full, a Claude login short of
//! full offered and a full one passed over, a balance at or below zero closing its provider to every model
//! that costs and to none its provider says costs nothing, and a road its caller fixed or the owner forced
//! walked alone — waited for, never left; a job's road chosen by the first that stands of its item's model,
//! its role's forced provider, its repository's pin and its role's order; and a road a walk left for its
//! provider's failure blocked for its kind's span and passed over by the next run while a later road can
//! answer, the work's own failure blocking nothing.

use agency_server::order::{Entry, Order, least_left, quota, window_left};
use agency_server::places::Ground;
use agency_server::repositories::Pin;
use agency_server::road::*;
use cache_siren::{KeptWindow, Reading, Slot, Window};

use crate::support::grounds::{deepseek_balance, openrouter_road, provider_balance, seeded};
use crate::support::scratch::Scratch;

fn entry(provider: &str, model: &str) -> Entry {
  Entry {
    provider: provider.into(),
    model: model.into(),
    effort: None,
    thinking: None,
    answer_tokens: None,
    advisor: None,
  }
}

fn order(entries: &[(&str, &str)]) -> Order {
  Order {
    entries: entries.iter().map(|(provider, model)| entry(provider, model)).collect(),
    forced: None,
  }
}

fn step(provider: &str, answer: Answer) -> Step {
  Step {
    provider: provider.into(),
    entry: entry(provider, &format!("{provider}-model")),
    answer,
  }
}

fn road(provider: &str) -> Road {
  Road {
    provider: provider.into(),
    model: format!("{provider}-model"),
    account: None,
  }
}

fn resolution(steps: Vec<Step>) -> Resolution {
  Resolution {
    row: "worker".into(),
    supervisor: false,
    steps,
    line: "worker".into(),
    uncarried: vec![],
    fixed: false,
    chosen_by: None,
    no_road: None,
  }
}

#[test]
fn the_first_provider_that_can_answer_carries_the_run_and_every_one_passed_says_why() {
  let walked = resolution(vec![
    step("deepseek", Answer::Cannot(Closed::plain("deepseek", "its balance is spent"))),
    step("codex", Answer::NoRoad),
    step("claude-code", Answer::Can(road("claude-code"))),
    step("openrouter", Answer::Can(road("openrouter"))),
  ]);

  assert_eq!(walked.road(), Some(&road("claude-code")));
  assert_eq!(walked.taken().map(|step| step.entry.model.as_str()), Some("claude-code-model"));
  assert_eq!(
    walked.passed().iter().map(|closed| closed.reason.as_str()).collect::<Vec<_>>(),
    vec!["its balance is spent"]
  );
  assert_eq!(
    walked.head().map(|head| head.provider),
    Some("deepseek".to_owned()),
    "the order's top heads it"
  );
}

#[test]
fn an_order_none_of_whose_providers_can_answer_is_closed_with_every_reason() {
  let closed = resolution(vec![
    step("deepseek", Answer::Cannot(Closed::plain("deepseek", "its balance is spent"))),
    step(
      "claude-code",
      Answer::Cannot(Closed {
        reopens_at: Some(1_900_000_000),
        ..Closed::plain("claude-code", "its five_hour window is full")
      }),
    ),
  ]);

  assert!(closed.road().is_none());
  assert_eq!(closed.reopens_at(), Some(1_900_000_000));
  assert!(closed.sentence().contains("deepseek: its balance is spent"));
  assert!(closed.sentence().contains("claude-code: its five_hour window is full"));
}

fn kept(name: &str, used: f64, resets_at: Option<i64>) -> KeptWindow {
  KeptWindow {
    window: Window {
      name: name.into(),
      utilization: used,
      resets_at,
      duration_minutes: None,
      limit_name: None,
    },
    seen_at: 1_000,
  }
}

#[test]
fn a_provider_has_left_what_its_window_with_the_least_left_has_left() {
  let reading = Reading {
    windows: vec![kept("five_hour", 0.25, Some(5_000)), kept("seven_day", 0.9, Some(9_000))],
    fresh: std::time::Duration::from_secs(10_000),
    unread_at: None,
  };
  let (left, window, resets_at) = least_left(&reading, 1_000).unwrap();

  assert!((left - 0.1).abs() < 1e-9);
  assert_eq!(window, "seven_day");
  assert_eq!(resets_at, Some(9_000));
}

#[test]
fn a_full_window_has_nothing_left_and_one_reset_counts_for_nothing() {
  assert_eq!(window_left(1.0, Some(5_000), 1_000), Some(0.0), "full: nothing left");
  assert_eq!(window_left(1.0, Some(500), 1_000), None, "reset by now: it counts for nothing");
  assert_eq!(window_left(0.4, None, 1_000), Some(0.6));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_provider_has_quota_while_anything_is_left_and_none_once_its_window_is_full() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let codex = harness_providers::provider("codex").unwrap();
  let now = agency_server::now_ms() / 1000;
  // The test's machine runs no Codex reader, so a Codex reading stands for no span past the second it was
  // seen at: it is seen a minute ahead, so it still stands at whichever second the quota is read.
  let seen_at = now + 60;
  let used = |utilization: f64| {
    let ground = &ground;

    async move {
      ground
        .windows
        .replace(
          Slot {
            mode: "codex",
            account: "",
          },
          &[Window {
            name: "codex:primary".into(),
            utilization,
            resets_at: Some(now + 3_600),
            duration_minutes: None,
            limit_name: None,
          }],
          seen_at,
        )
        .await
        .unwrap();

      quota(ground, codex, None, None).await
    }
  };

  let enough = used(0.995).await;

  assert!(enough.has, "half a percent left is quota, with no share held back: {enough:?}");

  let spent = used(1.0).await;

  assert!(!spent.has, "a full window leaves no quota: {spent:?}");
  assert_eq!(spent.window.as_deref(), Some("codex:primary"));
  assert_eq!(spent.resets_at, Some(now + 3_600));
}

// A Claude login takes runs until its window is full: one with half a percent left is offered, and the same
// login read full is passed over (docs/usage-windows.md, 「What reads the windows」).
#[tokio::test(flavor = "multi_thread")]
async fn a_claude_login_short_of_full_is_offered_and_a_full_one_is_not() {
  const OPUS: &str = "claude-opus-5-5";

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let now = agency_server::now_ms() / 1000;
  let used = |utilization: f64| {
    let ground = &ground;

    async move {
      ground
        .windows
        .replace(
          Slot {
            mode: "claude-code",
            account: "default",
          },
          &[Window {
            name: "five_hour".into(),
            utilization,
            resets_at: Some(now + 3_600),
            duration_minutes: Some(300),
            limit_name: None,
          }],
          now,
        )
        .await
        .unwrap();

      for_job(ground, None, None, Some(&pin("claude-code", OPUS)), &[], None).await.unwrap()
    }
  };

  let short = used(0.995).await;

  assert_eq!(
    short.road(),
    Some(&Road {
      provider: "claude-code".into(),
      model: OPUS.into(),
      account: Some("default".into()),
    }),
    "{short:?}"
  );

  let full = used(1.0).await;

  assert!(full.road().is_none(), "{full:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_balance_at_zero_closes_its_provider_and_a_balance_topped_up_reopens_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let walked = order(&[("deepseek", "deepseek-flash")]);
  let ask = Ask::of(Row::Role("worker"), "worker", &walked);

  deepseek_balance(&ground, 0.0).await;

  let spent = resolve(&ground, &ask).await.unwrap();

  assert!(spent.road().is_none(), "{spent:?}");
  assert!(spent.closed()[0].reason.contains("balance is spent"), "{spent:?}");

  deepseek_balance(&ground, 5.0).await;

  let topped = resolve(&ground, &ask).await.unwrap();

  assert_eq!(topped.road().map(|road| road.provider.as_str()), Some("deepseek"), "{topped:?}");
  assert_eq!(topped.road().map(|road| road.model.as_str()), Some("deepseek-flash"));
}

// Where a worker's run on one OpenRouter member goes out now.
async fn on_openrouter(ground: &Ground, model: &str) -> Resolution {
  let walked = order(&[("openrouter", model)]);

  resolve(ground, &Ask::of(Row::Role("worker"), "worker", &walked)).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_spent_balance_closes_only_the_members_that_cost_and_a_balance_left_closes_none() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let openrouter = harness_providers::provider("openrouter").unwrap();
  let costing = ["paid", "paid-though-named-free"];
  let free = ["free-by-field", "free-by-suffix"];

  openrouter_road(
    home.path(),
    &[
      ("paid", "vendor/a-model", None),
      ("paid-though-named-free", "vendor/a-model:free", Some(false)),
      ("free-by-field", "vendor/a-model", Some(true)),
      ("free-by-suffix", "vendor/a-model:free", None),
    ],
  );
  provider_balance(&ground, "openrouter", -0.5).await;

  for model in costing {
    let spent = on_openrouter(&ground, model).await;

    assert!(spent.road().is_none(), "{model}: {spent:?}");
    assert_eq!(spent.closed()[0].reason, "its balance is spent (-0.5 USD left)", "{model}");
    assert_eq!(
      spent.command(),
      Some(("openrouter", "buy OpenRouter credits at https://openrouter.ai/settings/credits"))
    );
  }

  for model in free {
    let going = on_openrouter(&ground, model).await;

    assert_eq!(going.road().map(|road| road.model.as_str()), Some(model), "{going:?}");
  }

  // The provider as a whole, naming no model, has no quota on a spent balance; a free member has.
  assert!(!quota(&ground, openrouter, None, None).await.has);
  assert!(quota(&ground, openrouter, None, Some("free-by-suffix")).await.has);
  assert!(!quota(&ground, openrouter, None, Some("paid")).await.has);

  provider_balance(&ground, "openrouter", 5.0).await;

  for model in costing.into_iter().chain(free) {
    let going = on_openrouter(&ground, model).await;

    assert_eq!(going.road().map(|road| road.model.as_str()), Some(model), "{going:?}");
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_seeded_worker_role_goes_out_on_deepseeks_flash_model_first() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;

  let resolution = for_job(&ground, None, None, None, &[], None).await.unwrap();

  assert_eq!(resolution.row, "worker");
  assert_eq!(
    resolution.road().map(|road| road.model.as_str()),
    Some("deepseek-flash"),
    "{resolution:?}"
  );
}

// A job of a role whose switched-on agents all name no road follows the supervisor that asks it, and one no
// supervisor asks has no road, which its start refuses. A role with no agent switched on is closed and waits
// (docs/role-agents.md, 「Which agent a run takes」; docs/repositories.md, 「A job」).
#[tokio::test(flavor = "multi_thread")]
async fn a_job_of_a_role_whose_agents_name_no_road_follows_the_supervisor_that_asks_it_or_is_refused() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;
  crate::support::rows::seats(scratch.db(), "worker", &[("deepseek", "deepseek-flash")]).await;
  scratch
    .run(
      r#"UPDATE "role_agents" SET "provider" = NULL, "model" = NULL WHERE "role" = 'worker'"#,
      vec![],
    )
    .await;
  crate::support::rows::supervisor(
    scratch.db(),
    "asking",
    &[
      ("provider", "deepseek".into()),
      ("model", "deepseek-flash".into()),
      ("effort", "high".into()),
    ],
  )
  .await;

  let asked = for_job(&ground, None, None, None, &[], Some("asking")).await.unwrap();

  assert_eq!(taken(&asked), Some(("deepseek".to_owned(), "deepseek-flash".to_owned())), "{asked:?}");
  assert_eq!(asked.chosen_by, Some(ChosenBy::Order));
  assert_eq!(asked.no_road, None);

  // The asking supervisor's provider cannot answer now: the job walks that road and waits for it, and its
  // start refuses it for nothing, the sentence kept for what it describes.
  deepseek_balance(&ground, 0.0).await;

  let waiting = for_job(&ground, None, None, None, &[], Some("asking")).await.unwrap();

  assert!(waiting.road().is_none(), "{waiting:?}");
  assert_eq!(waiting.no_road, None, "a provider that cannot answer now is waited for, not refused");
  assert_eq!(waiting.steps.len(), 1, "the asking supervisor's road alone is walked: {waiting:?}");
  assert!(
    waiting
      .closed()
      .iter()
      .any(|closed| closed.provider == "deepseek" && closed.reason.contains("balance is spent")),
    "{waiting:?}"
  );

  deepseek_balance(&ground, 5.0).await;

  let unasked = for_job(&ground, None, None, None, &[], None).await.unwrap();

  assert!(unasked.road().is_none(), "{unasked:?}");

  let why = unasked.no_road.clone().expect("the sentence its start refuses it with");

  assert!(why.contains("switched-on agents are on no road"), "{why}");
  assert!(why.contains("no supervisor asked this job"), "{why}");

  // A supervisor the store does not hold asked it: it has no road to follow either.
  let stranger = for_job(&ground, None, None, None, &[], Some("nobody")).await.unwrap();

  assert!(stranger.road().is_none(), "{stranger:?}");
  assert!(stranger.no_road.is_some());

  // A role with no agent switched on is closed: it waits for one, and its start refuses it for nothing.
  crate::support::rows::seats(scratch.db(), "worker", &[]).await;

  let closed = for_job(&ground, None, None, None, &[], Some("asking")).await.unwrap();

  assert!(closed.road().is_none(), "{closed:?}");
  assert_eq!(closed.no_road, None, "a closed role waits for an agent");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repositorys_pinned_model_is_walked_alone_and_its_work_waits_rather_than_falling_back() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;
  scratch
    .run(
      r#"INSERT INTO "repositories" ("name", "path", "pinned_provider", "pinned_model", "created_at", "updated_at")
         VALUES ('pinned', '/checkouts/pinned', 'registered', 'no-such-registration', 0, 0)"#,
      vec![],
    )
    .await;

  let pinned = for_job(&ground, None, Some("pinned"), None, &[], None).await.unwrap();

  assert!(pinned.fixed);
  assert_eq!(pinned.steps.len(), 1, "the pin alone is walked: {pinned:?}");
  assert!(
    pinned.road().is_none(),
    "the role's order could answer, and the pin's work waits all the same"
  );

  let unpinned = for_job(&ground, None, None, None, &[], None).await.unwrap();

  assert_eq!(unpinned.road().map(|road| road.provider.as_str()), Some("deepseek"));
}

fn pin(provider: &str, model: &str) -> Pin {
  Pin {
    provider: provider.into(),
    model: model.into(),
  }
}

// The worker role's forced provider set, or cleared where none is given.
// The worker's agents laid down on one road, the only road its runs then go out on.
async fn seat_worker(scratch: &Scratch, road: (&str, &str)) {
  crate::support::rows::seats(scratch.db(), "worker", &[road, road, road]).await;
}

fn taken(resolution: &Resolution) -> Option<(String, String)> {
  resolution.road().map(|road| (road.provider.clone(), road.model.clone()))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_goes_out_on_its_item_s_model_then_the_pin_then_its_role_s_agents_road() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let deepseek = |model: &str| Some(("deepseek".to_owned(), model.to_owned()));

  deepseek_balance(&ground, 5.0).await;
  scratch
    .run(
      r#"INSERT INTO "repositories" ("name", "path", "pinned_provider", "pinned_model", "created_at", "updated_at")
         VALUES ('pinned', '/checkouts/pinned', 'deepseek', 'pinned-model', 0, 0)"#,
      vec![],
    )
    .await;

  let order = for_job(&ground, None, None, None, &[], None).await.unwrap();

  assert_eq!((taken(&order), order.chosen_by), (deepseek("deepseek-flash"), Some(ChosenBy::Order)));

  let pinned = for_job(&ground, None, Some("pinned"), None, &[], None).await.unwrap();

  assert_eq!((taken(&pinned), pinned.chosen_by), (deepseek("pinned-model"), Some(ChosenBy::Pin)));

  // The role's agents moved to another road: a job that names nothing goes out on it, and the pin still
  // stands over it.
  seat_worker(&scratch, ("deepseek", "seated-model")).await;

  let seated = for_job(&ground, None, None, None, &[], None).await.unwrap();

  assert_eq!((taken(&seated), seated.chosen_by), (deepseek("seated-model"), Some(ChosenBy::Order)));
  assert_eq!(seated.steps.len(), 1, "the agents' one road alone is walked: {seated:?}");

  let pinned = for_job(&ground, None, Some("pinned"), None, &[], None).await.unwrap();

  assert_eq!((taken(&pinned), pinned.chosen_by), (deepseek("pinned-model"), Some(ChosenBy::Pin)));

  // The item's model stands over the pin.
  let item = pin("deepseek", "item-model");
  let named = for_job(&ground, None, Some("pinned"), Some(&item), &[], None).await.unwrap();

  assert_eq!((taken(&named), named.chosen_by), (deepseek("item-model"), Some(ChosenBy::Item)));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agents_road_or_an_item_s_model_that_cannot_answer_waits_and_never_falls_back() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;

  // The item names a registration whose engine is not there: the order could answer, and the job waits.
  let item = pin("registered", "no-such-registration");
  let named = for_job(&ground, None, None, Some(&item), &[], None).await.unwrap();

  assert!(named.road().is_none(), "{named:?}");
  assert_eq!(named.steps.len(), 1, "the item's model alone is walked: {named:?}");
  assert_eq!(named.chosen_by, Some(ChosenBy::Item));
  assert!(!named.sentence().is_empty());

  // The role's agents stand on a provider whose balance is spent: the role's runs wait for it.
  seat_worker(&scratch, ("deepseek", "seated-model")).await;
  deepseek_balance(&ground, 0.0).await;

  let spent = for_job(&ground, None, None, None, &[], None).await.unwrap();

  assert!(spent.road().is_none(), "{spent:?}");
  assert_eq!(spent.steps.len(), 1, "the agents' one road alone is walked: {spent:?}");
  assert!(spent.sentence().contains("balance is spent"), "{}", spent.sentence());

  // A run of the role that is no job waits for it alike.
  let role = agency_server::agent_roles::named(scratch.db(), "worker").await.unwrap().unwrap();
  let own = resolve(&ground, &Ask::of(Row::Role("worker"), &role.line, &role.order)).await.unwrap();

  assert!(own.road().is_none(), "{own:?}");
  assert_eq!(own.steps.len(), 1);
}

// A supervisor's turn goes out on its one road while that road can answer, and on nothing at all once it
// cannot: no other provider of the deployment is tried in its place (docs/provider-order.md, 「Whether a road can
// answer」).
#[tokio::test(flavor = "multi_thread")]
async fn a_supervisor_s_turn_goes_out_on_its_one_road_and_on_no_other_once_it_cannot_answer() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  deepseek_balance(&ground, 5.0).await;
  crate::support::rows::supervisor(
    scratch.db(),
    "one-road",
    &[("provider", "deepseek".into()), ("model", "deepseek-flash".into())],
  )
  .await;

  let supervisor = agency_server::supervisors::named(scratch.db(), "one-road").await.unwrap().unwrap();
  let order = supervisor.order();

  assert_eq!(order.entries.len(), 1, "{order:?}");
  assert_eq!(order.forced, None);

  let ask = Ask::of(Row::Supervisor("one-road"), "chat", &order);
  let turn = resolve(&ground, &ask).await.unwrap();

  assert_eq!(taken(&turn), Some(("deepseek".to_owned(), "deepseek-flash".to_owned())), "{turn:?}");

  // Its balance spent, the road cannot answer, and nothing stands in: the turn has no road, only the reason.
  deepseek_balance(&ground, 0.0).await;

  let closed = resolve(&ground, &ask).await.unwrap();

  assert!(closed.road().is_none(), "{closed:?}");
  assert_eq!(closed.open().count(), 0, "{closed:?}");
  assert_eq!(closed.steps.len(), 1, "only its own road was judged: {closed:?}");
  assert!(closed.sentence().contains("balance is spent"), "{}", closed.sentence());
}

const BUSY: &str =
  "API Error: 503 litellm.ServiceUnavailableError: DeepseekException - Service is too busy.";

#[test]
fn a_failure_s_kind_says_how_long_the_road_it_left_is_blocked_and_the_work_s_own_blocks_nothing() {
  assert_eq!(BlockKind::of_words(BUSY), Some(BlockKind::Outage));
  assert_eq!(BlockKind::of_words("connect ECONNREFUSED 127.0.0.1:18030"), Some(BlockKind::Outage));
  assert_eq!(BlockKind::of_words("API Error: 401 invalid api key"), Some(BlockKind::Access));
  assert_eq!(
    BlockKind::of_words("API Error: Request rejected (429) · too many requests"),
    Some(BlockKind::Limit { resets_at: None })
  );
  assert_eq!(BlockKind::of_words("API Error: 400 the prompt is too long"), None);

  let now = 1_000_000_000;

  assert_eq!(BlockKind::Outage.until(now), now + 60_000);
  assert_eq!(BlockKind::Access.until(now), now + 300_000);
  assert_eq!(BlockKind::Limit { resets_at: None }.until(now), now + 900_000);
  assert_eq!(
    BlockKind::Limit {
      resets_at: Some(2_000_000)
    }
    .until(now),
    2_000_000_000,
    "until its reset"
  );
}

// The roads a role's run may take among its agents, DeepSeek's first and the registry's after it, on a
// registration with an address.
async fn walking(scratch: &Scratch, data: &std::path::Path) -> Order {
  use agency_server::control::registry::change;

  let save = serde_json::json!({
    "action": "save", "name": "a-registration", "model": "m", "contextTokens": 32_768, "answerTokens": 4_096,
  });
  let address = serde_json::json!({
    "action": "set-address", "name": "a-registration", "node": "alpha",
    "url": "http://[::1]:9/v1/chat/completions", "key": null,
  });

  change(scratch.db(), data, &save).await.unwrap().unwrap();
  change(scratch.db(), data, &address).await.unwrap().unwrap();

  Order::of_row(
    r#"[{"provider":"deepseek","model":"deepseek-flash"},{"provider":"registered","model":"a-registration"}]"#,
  )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_road_a_walk_left_on_a_503_is_passed_over_by_the_next_run_and_the_work_s_own_error_blocks_nothing()
 {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let order = walking(&scratch, &home.path().join("data")).await;
  let ask = Ask::of(Row::Role("walker"), "chat", &order);
  let deepseek = || Some(("deepseek".to_owned(), "deepseek-flash".to_owned()));
  let registered = || Some(("registered".to_owned(), "a-registration".to_owned()));

  deepseek_balance(&ground, 5.0).await;

  let first = resolve(&ground, &ask).await.unwrap();
  let left = first.road().cloned().unwrap();

  assert_eq!(taken(&first), deepseek(), "{first:?}");
  assert_eq!(first.open().count(), 2, "both roads answer now: {first:?}");

  // A request refused for its shape is the work's own: no block, and the next run takes the same road.
  assert_eq!(leave(&ground, &left, "API Error: 400 the prompt is too long").await, None);
  assert_eq!(taken(&resolve(&ground, &ask).await.unwrap()), deepseek());

  // The walk leaves DeepSeek on a 503: its road is blocked for an outage's minute, and the next run passes it
  // over with the block's reason.
  assert_eq!(leave(&ground, &left, BUSY).await, Some(BlockKind::Outage));

  let next = resolve(&ground, &ask).await.unwrap();

  assert_eq!(taken(&next), registered(), "{next:?}");

  let passed = next.passed();

  assert_eq!(passed.len(), 1, "{passed:?}");
  assert_eq!(passed[0].provider, "deepseek");
  assert!(passed[0].reason.contains("passed over until"), "{}", passed[0].reason);
  assert!(passed[0].reason.contains("outage: API Error: 503"), "{}", passed[0].reason);

  let blocks = agency_server::queue::routes::kept_blocks(scratch.db()).await.unwrap();
  let block = blocks.iter().find(|block| block.selection == "deepseek::deepseek-flash").unwrap();
  let now = agency_server::now_ms();

  assert!(block.until > now + 50_000 && block.until <= now + 60_000, "{block:?}");

  // A forced provider is walked alone and never passed over for a block.
  let forced = Order {
    forced: Some(Pin {
      provider: "deepseek".into(),
      model: "deepseek-flash".into(),
    }),
    ..order.clone()
  };
  let alone = resolve(&ground, &Ask::of(Row::Role("walker"), "chat", &forced)).await.unwrap();

  assert_eq!(taken(&alone), deepseek(), "{alone:?}");

  // Every road that can answer blocked: a block is no reason a provider cannot answer, so the run goes out on
  // the first of them rather than waiting.
  leave(&ground, next.road().unwrap(), BUSY).await;

  let all_blocked = resolve(&ground, &ask).await.unwrap();

  assert_eq!(taken(&all_blocked), deepseek(), "{all_blocked:?}");
  assert!(all_blocked.passed().is_empty(), "{all_blocked:?}");

  // The block opened, the order's head carries the next run again.
  agency_server::queue::routes::clear_block_by_id(scratch.db(), block.id).await.unwrap();
  assert_eq!(taken(&resolve(&ground, &ask).await.unwrap()), deepseek());
}

// A run that meets the CLI's own words for one model's limit on every login it is handed.
struct ModelLimited;

impl agency_server::job_run::claude::renewal::Renewing for ModelLimited {}

impl agency_server::job_run::claude::walk::Attempt<()> for ModelLimited {
  async fn attempt(
    &mut self,
    _account: Option<agency_server::control::claude_accounts::Account>,
  ) -> Result<(), agency_server::job_run::claude::failure::Failure> {
    Err(agency_server::job_run::claude::failure::of_errored(
      "You've reached your Fable limit. Run /usage-credits to continue or switch models with /model.",
      "job-worker",
    ))
  }
}

// One model's limit met on the deployment's one login rests that model there and nothing else: a job on
// another model of the same login still goes out on it, while a job on the model that rests waits, its
// reason naming the login, the model and until when. The login's own window has room, so the login never
// rests.
#[tokio::test(flavor = "multi_thread")]
async fn one_model_s_limit_leaves_the_login_s_other_models_running() {
  use agency_server::job_run::claude::failure::Failure;
  use agency_server::job_run::claude::walk::{Walking, on_accounts};

  const FABLE: &str = "claude-fable-5-2";
  const OPUS: &str = "claude-opus-5-5";

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;
  let now = agency_server::now_ms() / 1000;
  let resets = now + 2 * 3_600;

  // The login has about 15% of its own window left.
  ground
    .windows
    .replace(
      Slot {
        mode: "claude-code",
        account: "default",
      },
      &[Window {
        name: "five_hour".into(),
        utilization: 0.85,
        resets_at: Some(resets),
        duration_minutes: Some(300),
        limit_name: None,
      }],
      now,
    )
    .await
    .unwrap();

  let walking = Walking {
    ground: &ground,
    model: FABLE,
  };
  let walked = on_accounts(&walking, "job-worker", None, &mut ModelLimited).await;

  assert!(
    matches!(&walked, Err(Failure::Limit { model: Some(model), account: Some(on), resets_at: Some(at), .. })
      if model == FABLE && on == "default" && *at == resets),
    "{walked:?}"
  );

  let rests: i64 = scratch
    .count(r#"SELECT COUNT(*) FROM "settings" WHERE "key" LIKE 'rest_until%'"#, vec![])
    .await;

  assert_eq!(rests, 0, "the login itself never rests for one model's limit");

  let opus = for_job(&ground, None, None, Some(&pin("claude-code", OPUS)), &[], None).await.unwrap();

  assert_eq!(
    opus.road(),
    Some(&Road {
      provider: "claude-code".into(),
      model: OPUS.into(),
      account: Some("default".into()),
    }),
    "{opus:?}"
  );

  let fable = for_job(&ground, None, None, Some(&pin("claude-code", FABLE)), &[], None).await.unwrap();

  assert!(fable.road().is_none(), "{fable:?}");
  assert!(
    fable.sentence().contains(&format!("is at its {FABLE} limit until")),
    "{}",
    fable.sentence()
  );
}

/// a road that moved only its advisor has not moved, and one that moved its effort has
#[test]
fn a_road_that_moved_only_its_advisor_has_not_moved() {
  let held = entry("claude-code", "claude-sonnet-5");
  let advised = Entry {
    advisor: Some("opus".to_owned()),
    ..held.clone()
  };
  let harder = Entry {
    effort: Some("high".to_owned()),
    ..held.clone()
  };

  assert!(!advised.moved_from(&held));
  assert!(harder.moved_from(&held));
  assert_eq!(advised.advisor_taken().as_deref(), Some("opus"));
  assert_eq!(
    Entry {
      advisor: Some("opus".to_owned()),
      ..entry("deepseek", "deepseek-flash")
    }
    .advisor_taken(),
    None,
    "a provider that takes no advisor carries none"
  );
}
