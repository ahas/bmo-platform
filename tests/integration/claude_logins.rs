//! The one judgement every Claude run is offered a login by, on a scratch store of the test's own and the
//! windows agency keeps in it (docs/cache-fallback.md, 「The Claude Code road is a list of accounts」): a
//! login whose window is kept full is closed while the reading stands and not once it is stale, a run none of whose
//! logins answers goes out on none, a full window rests the login it was met on and no other, one model's
//! limit rests that model on its login and no other model, a limit the windows do not confirm never rests the
//! login, the rest that closes the last open login is published once, a standing program left on a closed
//! login is taken down, and a login near its end is renewed in a step of its own: no run holds the account's
//! lock across its turn, and a second run on it says its wait in its job's log, goes out once that renewal is
//! done, never after the first run's turn, and ends as the login's failure where the renewal found the login
//! gone. A login that refuses a job's turn with a 403 rests alone, however far its credential's moment
//! stands, and the job goes out on the next login, its log and its roads naming the login that refused and its
//! words; a run every login of which refuses ends on the login failure naming each, which a person settles by
//! signing in.

use crate::integration::common;

use agency_server::control::claude_accounts::{
  Account, CREDENTIAL_FILE, Judging, open_accounts, rest_until, walk_for, walk_for_user,
};
use agency_server::lines::claude::{Runs, account_env};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agency_server::job_run::claude::failure::{AccountFailure, Failure, of_errored, of_unanswered};
use agency_server::job_run::claude::renewal::{Renewing, lock_path};
use agency_server::job_run::claude::turn::{JobMoves, JobWaits};
use agency_server::job_run::claude::walk::{Attempt, PassedOver, Walking, on_accounts};
use agency_server::keepalive::logins::{node_token_refused, retire_closed};
use agency_server::now_ms;
use agency_server::places::{Ground, locks};
use agency_server::rest::{self, DeploymentResting, RestKind, clear_passed_rests, watch_silence};
use agency_server::run_log::{RunLog, Target};
use cache_siren::{Keeping, Slot, Window};
use common::Scratch;
use harness_providers::Machine;
use serde_json::json;

const HOUR: i64 = 3_600;

// The model the bench's runs go out on, and another model of the same logins.
const FABLE: &str = "claude-fable-5-2";
const OPUS: &str = "claude-opus-5-5";

// What the CLI answers on a run whose model has reached its own limit.
const FABLE_LIMIT: &str =
  "You've reached your Fable limit. Run /usage-credits to continue or switch models with /model.";

// The program a standing place runs, as its record names it.
const PROGRAM: &str = "claude";

fn now() -> i64 {
  chrono::Utc::now().timestamp()
}

/// A store, a data root and the logins' homes of the test's own, and agency's ground on them, which keeps
/// the logins' windows in that store.
struct Bench {
  scratch: Scratch,
  homes: tempfile::TempDir,
  data: tempfile::TempDir,
  ground: Ground,
}

impl Bench {
  // Two logins, `main` first in the order and in use, `default` after it, each signed in its own folder.
  async fn new() -> Self {
    let scratch = Scratch::new().await;
    let homes = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let service = common::service(&scratch, &[]);
    let machine =
      Machine::with_env([("HOME".to_owned(), data.path().to_string_lossy().into_owned())], data.path());
    let ground = Ground::new(&service, &machine, None);

    for (id, place) in [("main", 1_i64), ("default", 2)] {
      let home = homes.path().join(id);

      crate::support::grounds::signed_in(&home);
      scratch
        .run(
          r#"INSERT INTO "claude_accounts" ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at")
             VALUES ($1, $1, $2, 1, 'directory', $3, 1)
             ON CONFLICT ("id") DO UPDATE SET "place" = $2, "credential_path" = $3"#,
          vec![id.into(), place.into(), home.to_string_lossy().into_owned().into()],
        )
        .await;
    }

    scratch
      .run(
        r#"INSERT INTO "settings" ("key", "value") VALUES ('claude_account_in_use', 'main')
           ON CONFLICT ("key") DO UPDATE SET "value" = 'main'"#,
        vec![],
      )
      .await;

    Self {
      scratch,
      homes,
      data,
      ground,
    }
  }

  // One login's weekly window kept as read: the share used, its reset and when it was seen.
  async fn read(&self, account: &str, used: f64, resets_at: i64, seen_at: i64) {
    week(&self.ground.windows, account, used, resets_at, seen_at).await;
  }

  fn judging(&self) -> Judging<'_> {
    Judging::of(&self.ground)
  }

  fn walking(&self) -> Walking<'_> {
    self.walking_on(FABLE)
  }

  fn walking_on<'a>(&'a self, model: &'a str) -> Walking<'a> {
    Walking {
      ground: &self.ground,
      model,
    }
  }

  async fn setting(&self, key: &str) -> Option<String> {
    store::one(
      self.scratch.db(),
      r#"SELECT "value" FROM "settings" WHERE "key" = $1"#,
      vec![key.into()],
    )
    .await
    .unwrap()
    .map(|row| row.try_get("", "value").unwrap())
  }

  async fn walk(&self) -> Vec<String> {
    walk_for(self.judging()).await.unwrap().into_iter().map(|account| account.id).collect()
  }

  // The logins a run on a model is offered now.
  async fn walk_on(&self, model: &str) -> Vec<String> {
    walk_for_user(self.judging(), None, Some(model))
      .await
      .unwrap()
      .into_iter()
      .map(|account| account.id)
      .collect()
  }
}

// One login's weekly window written into a keeping as its reader would.
async fn week(windows: &Keeping, account: &str, used: f64, resets_at: i64, seen_at: i64) {
  let week = Window {
    name: "seven_day".into(),
    utilization: used,
    resets_at: Some(resets_at),
    duration_minutes: Some(10_080),
    limit_name: None,
  };

  windows
    .replace(
      Slot {
        mode: "claude-code",
        account,
      },
      &[week],
      seen_at,
    )
    .await
    .unwrap();
}

// A turn that meets a limit on the logins named, and answers on any other; every login it was handed is
// kept. The limit is a full window with its reset unless the turn says otherwise, and where the turn holds
// a keeping, the login's weekly window is read full as the limit is met, as the reader finds it.
struct Turn {
  full: Vec<&'static str>,
  tried: Vec<Option<String>>,
  said: Option<&'static str>,
  fills: Option<(Keeping, i64)>,
}

impl Turn {
  fn full_on(full: Vec<&'static str>) -> Self {
    Self {
      full,
      tried: vec![],
      said: None,
      fills: None,
    }
  }

  fn saying(full: Vec<&'static str>, said: &'static str) -> Self {
    Self {
      said: Some(said),
      ..Self::full_on(full)
    }
  }
}

impl Renewing for Turn {}

impl Attempt<String> for Turn {
  async fn attempt(&mut self, account: Option<Account>) -> Result<String, Failure> {
    let id = account.map(|account| account.id);

    self.tried.push(id.clone());

    match id {
      Some(id) if self.full.contains(&id.as_str()) => {
        if let Some((windows, resets_at)) = &self.fills {
          week(windows, &id, 1.0, *resets_at, now()).await;
        }

        Err(match self.said {
          Some(said) => of_errored(said, "bmo"),
          None => Failure::limit(Some(now() + HOUR), None),
        })
      }
      Some(id) => Ok(id),
      None => Ok("the machine's own".to_owned()),
    }
  }
}

// A login whose window is kept full is never offered, whatever became of the rest written for it, and one whose
// window has reset since closes nothing.
#[tokio::test]
async fn a_login_whose_window_is_read_full_is_never_offered_whatever_became_of_its_rest() {
  let bench = Bench::new().await;

  bench.read("main", 1.0, now() + 4 * 24 * HOUR, now()).await;
  bench.read("default", 0.19, now() + 6 * 24 * HOUR, now()).await;

  assert_eq!(bench.walk().await, ["default"]);

  bench.read("main", 1.0, now() - 60, now()).await;

  assert_eq!(bench.walk().await, ["main", "default"]);
}

// A full window that names its reset closes its login until that reset however old its reading is, since
// only a newer reading can say it came back early; one whose reset has passed closes nothing.
#[tokio::test]
async fn a_full_window_closes_its_login_until_its_reset_however_old_its_reading() {
  let bench = Bench::new().await;
  let fresh = bench.ground.windows.fresh("claude-code").as_secs() as i64;

  bench.read("main", 1.0, now() + 4 * 24 * HOUR, now() - fresh - 60).await;
  bench.read("default", 1.0, now() - 60, now() - fresh - 60).await;

  assert_eq!(bench.walk().await, ["default"]);
}

// With every login closed the run goes out on none, not on the closed ones and not on the machine's own
// login, and ends as the limit the first of them reopens from, naming no login, so nothing after the walk
// writes a rest under a login the run was not on.
#[tokio::test]
async fn a_run_none_of_whose_logins_answers_goes_out_on_none() {
  let bench = Bench::new().await;
  let reopens = now() + 2 * HOUR;

  bench.read("main", 1.0, now() + 4 * 24 * HOUR, now()).await;
  bench
    .scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('rest_until:default', $1)"#,
      vec![reopens.to_string().into()],
    )
    .await;

  assert!(open_accounts(bench.judging()).await.unwrap().is_empty());

  let mut turn = Turn::full_on(vec![]);
  let walked = on_accounts(&bench.walking(), "bmo", None, &mut turn).await;

  assert_eq!(walked, Err(Failure::limit(Some(reopens), None)));
  assert!(turn.tried.is_empty());
  assert_eq!(bench.setting("rest_until:main").await, None);
}

// A full window met on one login rests that login, under its own name, and the next carries the run; where
// every login meets one, each rests under its own name and the failure names the last.
#[tokio::test]
async fn a_full_window_rests_the_login_it_was_met_on_and_no_other() {
  let bench = Bench::new().await;
  let mut turn = Turn::full_on(vec!["main"]);

  assert_eq!(on_accounts(&bench.walking(), "bmo", None, &mut turn).await, Ok("default".to_owned()));
  assert_eq!(turn.tried, [Some("main".to_owned()), Some("default".to_owned())]);
  assert!(bench.setting("rest_until:main").await.is_some());
  assert_eq!(bench.setting("rest_until:default").await, None);
  assert_eq!(bench.setting("claude_account_in_use").await.as_deref(), Some("default"));

  bench
    .scratch
    .run(r#"DELETE FROM "settings" WHERE "key" LIKE 'rest_until%'"#, vec![])
    .await;

  let mut turn = Turn::full_on(vec!["main", "default"]);
  let walked = on_accounts(&bench.walking(), "bmo", None, &mut turn).await;

  assert!(matches!(walked, Err(Failure::Limit { account: Some(ref last), .. }) if last == "main"));
  assert!(bench.setting("rest_until:main").await.is_some());
  assert!(bench.setting("rest_until:default").await.is_some());
}

// One model's own limit rests that model on the login it was met on and nothing else: the run moves to the
// next login that carries the model, no rest of the login is written, and a run on another model still goes
// out on that login. The rest runs until the soonest reset the login's windows name, where the answer named
// none, and the failure says which model on which login rests until when.
#[tokio::test]
async fn one_models_limit_rests_that_model_alone_and_another_model_goes_out_on_the_login() {
  let bench = Bench::new().await;
  let resets = now() + 3 * 24 * HOUR;

  bench.read("main", 0.85, resets, now()).await;

  let mut turn = Turn::saying(vec!["main"], FABLE_LIMIT);

  assert_eq!(on_accounts(&bench.walking(), "bmo", None, &mut turn).await, Ok("default".to_owned()));
  assert_eq!(turn.tried, [Some("main".to_owned()), Some("default".to_owned())]);
  assert_eq!(bench.setting("rest_until:main").await, None);

  let rest: serde_json::Value =
    serde_json::from_str(&bench.setting(&format!("model_rest:main:{FABLE}")).await.unwrap()).unwrap();

  assert_eq!(rest["until"], json!(resets));
  assert_eq!(bench.walk_on(FABLE).await, ["default"]);
  assert_eq!(bench.walk_on(OPUS).await, ["main", "default"]);
  assert_eq!(bench.walk().await, ["main", "default"]);

  let mut opus = Turn::full_on(vec![]);

  assert_eq!(
    on_accounts(&bench.walking_on(OPUS), "bmo", None, &mut opus).await,
    Ok("main".to_owned())
  );
  assert_eq!(opus.tried, [Some("main".to_owned())]);

  let mut both = Turn::saying(vec!["default"], FABLE_LIMIT);
  let walked = on_accounts(&bench.walking(), "bmo", None, &mut both).await.unwrap_err();

  assert!(
    matches!(&walked, Failure::Limit { model: Some(model), account: Some(on), .. } if model == FABLE && on == "default"),
    "{walked:?}"
  );
  assert!(
    walked.message().contains(&format!("{FABLE} rests on account default until")),
    "{}",
    walked.message()
  );
  assert_eq!(bench.walk_on(FABLE).await, Vec::<String>::new());
  assert_eq!(bench.walk_on(OPUS).await, ["main", "default"]);
}

// A limit that names no model and no reset rests the login where its windows confirm the ceiling, until the
// full window resets; where they do not, the login stays open to every other model and the run's model waits
// on the next reading, which then decides.
#[tokio::test]
async fn a_login_ceiling_the_windows_confirm_rests_the_login_and_no_other_limit_does() {
  let bench = Bench::new().await;
  let resets = now() + 2 * 24 * HOUR;

  bench.read("main", 0.5, resets, now() - 60).await;

  let mut turn = Turn {
    fills: Some((bench.ground.windows.clone(), resets)),
    ..Turn::saying(vec!["main"], "You've hit your limit")
  };

  assert_eq!(on_accounts(&bench.walking(), "bmo", None, &mut turn).await, Ok("default".to_owned()));
  assert_eq!(rest_until(bench.judging(), "main").await.unwrap(), Some(resets));
  assert_eq!(bench.setting(&format!("model_rest:main:{FABLE}")).await, None);
  assert_eq!(bench.walk_on(OPUS).await, ["default"]);

  let unconfirmed = now() + 4 * 24 * HOUR;

  bench.read("default", 0.5, unconfirmed, now() - 60).await;

  let mut turn = Turn::saying(vec!["default"], "You've hit your limit");

  assert!(on_accounts(&bench.walking(), "bmo", None, &mut turn).await.is_err());
  assert_eq!(bench.setting("rest_until:default").await, None);
  assert_eq!(bench.walk_on(OPUS).await, ["default"]);
  assert_eq!(bench.walk_on(FABLE).await, Vec::<String>::new());

  bench.read("default", 0.5, unconfirmed, now() + 1).await;

  assert_eq!(bench.walk_on(FABLE).await, ["default"]);
}

// The walk's rest that closes the last open login is published once as `rest.changed`, and a run
// after it, every login closed, publishes nothing again.
#[tokio::test]
async fn the_rest_that_closes_the_last_login_is_published_once() {
  let bench = Bench::new().await;
  let mut resting = bench.ground.events.subscribe::<DeploymentResting>();
  let both_full = || Turn::full_on(vec!["main", "default"]);

  assert!(matches!(
    on_accounts(&bench.walking(), "bmo", None, &mut both_full()).await,
    Err(Failure::Limit { .. })
  ));

  let published = resting.ready().expect("the rest was published");

  assert_eq!(published.kind, "limit");
  assert_eq!(resting.ready(), None);
  assert!(matches!(
    on_accounts(&bench.walking(), "bmo", None, &mut both_full()).await,
    Err(Failure::Limit { account: None, .. })
  ));
  assert_eq!(resting.ready(), None);
}

// Every login closed by a window read full, with no rest written, is a silence no writer saw begin: the watch
// tells it once, and its end is answered once to whoever clears the passed rests, never while it stands.
#[tokio::test]
async fn a_silence_the_windows_hold_is_told_once_by_the_watch_and_its_end_once() {
  let bench = Bench::new().await;
  let mut resting = bench.ground.events.subscribe::<DeploymentResting>();
  let at = now();

  bench.read("main", 1.0, at + HOUR, at).await;
  bench.read("default", 1.0, at + 2 * HOUR, at).await;
  watch_silence(&bench.ground, at).await.unwrap();

  let told = resting.ready().expect("the silence was told");

  assert_eq!((told.kind, told.account, told.until), ("limit", None, at + HOUR));

  watch_silence(&bench.ground, at).await.unwrap();

  assert_eq!(resting.ready(), None);
  assert_eq!(clear_passed_rests(bench.judging(), at).await.unwrap(), None);
  assert_eq!(clear_passed_rests(bench.judging(), at + 3 * HOUR).await.unwrap(), Some(RestKind::Limit));
  assert_eq!(clear_passed_rests(bench.judging(), at + 3 * HOUR).await.unwrap(), None);
}

// The administrator's end of the rest clears what was written and sets the full windows aside, so every
// platform login answers again at once; the silence it told is answered as ended once, and an end with nothing
// resting ends nothing.
#[tokio::test]
async fn the_rest_ended_by_hand_opens_every_login_and_its_end_is_answered_once() {
  let bench = Bench::new().await;
  let at = now();

  bench.read("main", 1.0, at + HOUR, at).await;
  rest::write_rest(bench.judging(), (at + 2 * HOUR) as f64, RestKind::Limit, "default", at)
    .await
    .unwrap();
  rest::write_model_rest(bench.scratch.db(), "default", FABLE, Some(at + HOUR), at)
    .await
    .unwrap();
  watch_silence(&bench.ground, at).await.unwrap();

  assert!(rest::resting(bench.judging(), at).await.unwrap());
  assert!(rest::end_rest(bench.judging(), at).await.unwrap());
  assert!(!rest::resting(bench.judging(), at).await.unwrap());
  assert_eq!(rest::model_rest_until(bench.judging(), "default", FABLE, at).await.unwrap(), None);
  assert_eq!(bench.setting("rest_until:default").await, None);
  assert_eq!(bench.walk().await, ["main", "default"]);
  assert_eq!(clear_passed_rests(bench.judging(), at).await.unwrap(), Some(RestKind::Limit));
  assert_eq!(clear_passed_rests(bench.judging(), at).await.unwrap(), None);
  assert!(!rest::end_rest(bench.judging(), at).await.unwrap());

  // The reader's next reading that still finds the window full closes the login again.
  bench.read("main", 1.0, at + HOUR, at + 1).await;
  bench.read("default", 1.0, at + HOUR, at + 1).await;

  assert!(rest::resting(bench.judging(), at + 1).await.unwrap());
}

// A rest's moment reported in milliseconds is kept as the second it names, and one an older writer kept in
// milliseconds is read so, so neither rests a login for ever.
#[tokio::test]
async fn a_moment_in_milliseconds_is_kept_and_read_as_its_second() {
  let bench = Bench::new().await;
  let at = now();

  rest::write_rest(bench.judging(), ((at + HOUR) * 1000) as f64, RestKind::Limit, "main", at)
    .await
    .unwrap();

  assert_eq!(bench.setting("rest_until:main").await, Some((at + HOUR).to_string()));
  assert_eq!(rest_until(bench.judging(), "main").await.unwrap(), Some(at + HOUR));

  bench
    .scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('rest_until:default', $1)"#,
      vec![((at + 2 * HOUR) * 1000).to_string().into()],
    )
    .await;

  assert_eq!(rest_until(bench.judging(), "default").await.unwrap(), Some(at + 2 * HOUR));
  assert_eq!(rest::rest_until(bench.judging(), at).await.unwrap(), Some(at + HOUR));
  assert_eq!(rest::seconds_of(((at + HOUR) * 1000) as f64), (at + HOUR) as f64);
}

// A silence the written rests alone hold is told by whatever wrote them, never by the watch, so one only
// refusals hold is not told as a limit.
#[tokio::test]
async fn the_watch_tells_no_silence_the_written_rests_alone_hold() {
  let bench = Bench::new().await;
  let mut resting = bench.ground.events.subscribe::<DeploymentResting>();
  let at = now();

  for account in ["main", "default"] {
    rest::write_rest(bench.judging(), (at + HOUR) as f64, RestKind::Refused, account, at)
      .await
      .unwrap();
  }

  assert!(rest::resting(bench.judging(), at).await.unwrap());

  watch_silence(&bench.ground, at).await.unwrap();

  assert_eq!(resting.ready(), None);
}

// An organisation's own login is no login of the platform's own work: spent or open, it neither holds the
// deployment's rest nor lifts it.
#[tokio::test]
async fn an_organisations_login_neither_holds_the_rest_nor_lifts_it() {
  let bench = Bench::new().await;
  let at = now();
  let home = bench.homes.path().join("unit");

  crate::support::grounds::signed_in(&home);
  bench
    .scratch
    .run(
      r#"INSERT INTO "claude_accounts" ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at", "owner_unit")
         VALUES ('unit', 'unit', 3, 1, 'directory', $1, 1, 7)"#,
      vec![home.to_string_lossy().into_owned().into()],
    )
    .await;
  bench.read("main", 1.0, at + HOUR, at).await;
  bench.read("default", 1.0, at + HOUR, at).await;

  assert_eq!(rest::rest_until(bench.judging(), at).await.unwrap(), Some(at + HOUR));

  bench.read("main", 0.5, at + HOUR, at + 1).await;
  bench.read("unit", 1.0, at + HOUR, at).await;

  assert_eq!(rest::rest_until(bench.judging(), at + 1).await.unwrap(), None);
}

// The loop in a pane starts the program its place holds again on the login written into it, so a program
// left on a closed login is taken down, while one with a turn in it and one on an open login stand.
#[tokio::test]
async fn a_standing_program_on_a_closed_login_is_taken_down_where_no_turn_is_in_it() {
  let bench = Bench::new().await;
  let data = bench.data.path();
  let homes = bench.homes.path();

  bench.read("main", 1.0, now() + 4 * 24 * HOUR, now()).await;

  let place = |agent: &str, login: &str, turn_open: bool| {
    let path = data.join("agent-places").join(agent);
    let env = json!({ "CLAUDE_CONFIG_DIR": homes.join(login) });
    let command = json!({ "args": [PROGRAM, "--resume", "s"], "cwd": "/tmp", "env": env });

    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
      path.join("program.json"),
      json!({ "identity": agent, "command": command, "restart": command }).to_string(),
    )
    .unwrap();

    if turn_open {
      std::fs::write(
        path.join("run.json"),
        json!({ "name": "r", "title": "t", "person": false, "startedAt": 1, "endedAt": null }).to_string(),
      )
      .unwrap();
    }

    path.join("program.json")
  };
  let closed = place("chat", "main", false);
  let working = place("worker", "main", true);
  let open = place("gate", "default", false);

  retire_closed(bench.judging(), data, chrono::Utc::now().timestamp_millis())
    .await
    .unwrap();

  assert!(!closed.exists());
  assert!(working.exists());
  assert!(open.exists());
}

// How often a run in these tests looks again while it waits on another run's renewal.
const LOOK: Duration = Duration::from_millis(1);

// The bound on a test of two runs, which only a run that never goes out reaches.
const BOUND: Duration = Duration::from_secs(30);

// What the test lets each run do and hears from it, shared by the two runs and the test.
#[derive(Clone, Default)]
struct Stage {
  said: Arc<Mutex<Vec<String>>>,
  holds: Arc<tokio::sync::Notify>,
  waits: Arc<tokio::sync::Notify>,
  renewal_may_end: Arc<tokio::sync::Notify>,
  second_out: Arc<tokio::sync::Notify>,
  turn_may_end: Arc<tokio::sync::Notify>,
}

impl Stage {
  fn hear(&self, line: String) {
    self.said.lock().unwrap().push(line);
  }

  fn said(&self) -> Vec<String> {
    self.said.lock().unwrap().clone()
  }

  fn place(&self, line: &str) -> usize {
    let said = self.said();

    said
      .iter()
      .position(|one| one == line)
      .unwrap_or_else(|| panic!("{line:?} was not said: {said:?}"))
  }
}

// The run that takes the account's lock: its renewal waits for the test's word and then writes a credential
// far from its end, or fails on the login; its turn notes whether the lock is still held and, on `main`, ends
// only on the test's word.
struct First {
  stage: Stage,
  data: std::path::PathBuf,
  fails: bool,
}

impl Renewing for First {
  async fn renew(&mut self, _ground: &Ground, account: &Account) -> Result<(), Failure> {
    self.stage.hear(format!("first renews {}", account.id));
    self.stage.holds.notify_one();
    self.stage.renewal_may_end.notified().await;

    if self.fails {
      return Err(Failure::Account {
        failure: AccountFailure::Login,
        detail: "the refresh was refused".to_owned(),
      });
    }

    let file = account.credential_file().unwrap();

    std::fs::write(file, credential(now_ms() as i64 + HOUR * 1000)).unwrap();
    self.stage.hear("first renewed".to_owned());
    Ok(())
  }

  fn look(&self) -> Duration {
    LOOK
  }
}

impl Attempt<String> for First {
  async fn attempt(&mut self, account: Option<Account>) -> Result<String, Failure> {
    let account = account.unwrap();
    let held = locks::held(&lock_path(&self.data, &account));

    self.stage.hear(format!("first goes out on {}, the lock held: {held}", account.id));

    if account.id == "main" {
      self.stage.turn_may_end.notified().await;
    }

    self.stage.hear("first's turn ends".to_owned());
    Ok(account.id)
  }
}

// The job the second run is a run of, whose log its waits are written into.
const SECOND_JOB: i64 = 7;

// The run that finds the lock held: it says what it says into the stage and, as a job's turn does, into its
// job's log, and it never renews.
struct Second {
  stage: Stage,
  db: sea_orm::DatabaseConnection,
  log: RunLog,
}

impl Second {
  fn of(stage: &Stage, bench: &Bench) -> Self {
    Self {
      stage: stage.clone(),
      db: bench.scratch.db().clone(),
      log: RunLog::new(Target {
        agent: "worker".to_owned(),
        run: format!("job-{SECOND_JOB}"),
        chat_id: None,
        job_id: Some(SECOND_JOB),
      }),
    }
  }
}

// The notes the second run's job's log holds, in the order they were written.
async fn job_notes(bench: &Bench) -> Vec<String> {
  store::all(
    bench.scratch.db(),
    r#"SELECT "text" FROM "agent_log" WHERE "job_id" = $1 AND "kind" = 'note' ORDER BY "id""#,
    vec![SECOND_JOB.into()],
  )
  .await
  .unwrap()
  .iter()
  .map(|row| row.try_get("", "text").unwrap())
  .collect()
}

impl Renewing for Second {
  async fn renew(&mut self, _ground: &Ground, account: &Account) -> Result<(), Failure> {
    panic!("the second run renewed {}", account.id)
  }

  // The test hears of the wait once its line is in the job's log.
  async fn say(&mut self, words: &str) {
    self.stage.hear(format!("second: {words}"));
    JobWaits::of(&self.db, &mut self.log).say(words).await;

    if words.starts_with("Waiting") {
      self.stage.waits.notify_one();
    }
  }

  fn look(&self) -> Duration {
    LOOK
  }
}

impl Attempt<String> for Second {
  async fn attempt(&mut self, account: Option<Account>) -> Result<String, Failure> {
    let id = account.unwrap().id;

    self.stage.hear(format!("second goes out on {id}"));
    self.stage.second_out.notify_one();
    Ok(id)
  }
}

// A credential file of a subscription login that renews itself, its access token ending at `ends`.
fn credential(ends: i64) -> String {
  json!({ "claudeAiOauth": { "accessToken": "a", "refreshToken": "r", "expiresAt": ends } }).to_string()
}

// The bench with `main`'s login a minute from its end, inside the margin the CLI renews it within.
async fn near_its_end() -> Bench {
  let bench = Bench::new().await;

  let file = bench.homes.path().join("main").join(CREDENTIAL_FILE);

  std::fs::write(file, credential(now_ms() as i64 + 60_000)).unwrap();

  bench
}

// Two runs on one login near its end: the first takes the lock, renews and lets the lock go before its turn,
// and the second, waiting on that renewal and saying so in its job's log, goes out on what it wrote while the
// first's turn is still running, the lock free all through that turn.
#[tokio::test]
async fn a_second_run_goes_out_after_the_renewal_not_after_the_first_run_s_turn() {
  let bench = near_its_end().await;
  let main = walk_for(bench.judging())
    .await
    .unwrap()
    .into_iter()
    .find(|one| one.id == "main")
    .unwrap();
  let stage = Stage::default();
  let mut first = First {
    stage: stage.clone(),
    data: bench.data.path().to_owned(),
    fails: false,
  };
  let mut second = Second::of(&stage, &bench);
  let walking = bench.walking();
  let runs = async {
    tokio::join!(
      on_accounts(&walking, "bmo", None, &mut first),
      async {
        stage.holds.notified().await;
        on_accounts(&walking, "bmo", None, &mut second).await
      },
      async {
        stage.waits.notified().await;

        let waiting = job_notes(&bench).await;

        stage.renewal_may_end.notify_one();
        stage.second_out.notified().await;

        // The first run's turn is still running here: the lock is free and the second's log says it all.
        let held_in_turn = locks::held(&lock_path(bench.data.path(), &main));
        let gone_out = job_notes(&bench).await;

        stage.turn_may_end.notify_one();
        (waiting, held_in_turn, gone_out)
      }
    )
  };
  let (first_walked, second_walked, (waiting, held_in_turn, gone_out)) =
    tokio::time::timeout(BOUND, runs).await.expect("both runs ended");
  let wait = "Waiting for another run to renew the login of Claude account main";
  let out = "Claude account main: another run renewed its login, going out on what it wrote";

  assert_eq!(waiting, [wait], "the wait is in the job's log while it waits");
  assert!(!held_in_turn, "no run holds the account's lock across its turn");
  assert_eq!(
    gone_out,
    [wait, out],
    "the job's log says it went out, before the first run's turn ends"
  );
  assert_eq!(first_walked, Ok("main".to_owned()));
  assert_eq!(second_walked, Ok("main".to_owned()));
  assert!(stage.place("first renewed") < stage.place("first goes out on main, the lock held: false"));
  assert!(stage.place("second goes out on main") < stage.place("first's turn ends"));
  assert!(
    stage.place("second: Waiting for another run to renew the login of Claude account main")
      < stage.place("first renewed")
  );
  assert!(
    stage.place("second: Claude account main: another run renewed its login, going out on what it wrote")
      < stage.place("second goes out on main")
  );
}

// A renewal that finds the login gone is recorded before the lock is let go, so the run waiting on it, and a
// run after them, end on that login as its failure, never going out on it nor renewing it again, and each is
// carried to the next login.
#[tokio::test]
async fn a_renewal_that_finds_the_login_gone_ends_the_run_waiting_on_it() {
  let bench = near_its_end().await;
  let stage = Stage::default();
  let mut first = First {
    stage: stage.clone(),
    data: bench.data.path().to_owned(),
    fails: true,
  };
  let mut second = Second::of(&stage, &bench);
  let walking = bench.walking();
  let runs = async {
    tokio::join!(
      on_accounts(&walking, "bmo", None, &mut first),
      async {
        stage.holds.notified().await;
        on_accounts(&walking, "bmo", None, &mut second).await
      },
      async {
        stage.waits.notified().await;
        stage.renewal_may_end.notify_one();
      }
    )
  };
  let (first_walked, second_walked, ()) = tokio::time::timeout(BOUND, runs).await.expect("both runs ended");

  assert_eq!(first_walked, Ok("default".to_owned()));
  assert_eq!(second_walked, Ok("default".to_owned()));
  assert!(!stage.said().contains(&"second goes out on main".to_owned()));

  // A run after them renews nothing on the credential whose renewal was refused and is carried on at once.
  let mut third = Second::of(&stage, &bench);

  assert_eq!(on_accounts(&walking, "bmo", None, &mut third).await, Ok("default".to_owned()));
  assert!(!stage.said().contains(&"second goes out on main".to_owned()));
}

// What the API's firewall answered a run with, as the resident program leaves it.
const CLOUDFLARE: &str = "Please run /login · API Error: 403 Attention Required! | Cloudflare";

// The job the refused turns are a turn of, whose log and roads the walk's moves are written into.
const REFUSED_JOB: i64 = 9;

// A job's turn the logins named refuse with the firewall's 403 and any other answers, every login it went out
// on kept; a login passed over is told and kept as a job's turn tells and keeps it, and the prompt the turn
// goes out with on the next login is kept too.
struct Refusing<'b> {
  refusing: Vec<&'static str>,
  tried: Vec<String>,
  prompts: Vec<Option<String>>,
  ground: &'b Ground,
  log: RunLog,
}

impl<'b> Refusing<'b> {
  async fn of(bench: &'b Bench, refusing: Vec<&'static str>) -> Self {
    bench
      .scratch
      .run(
        r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
             "token_count", "started_at", "prompt")
           VALUES ($1, 90, 1, 'harness', 1, 'coder', 'running', 0, 0, '# An item')
           ON CONFLICT ("id") DO NOTHING"#,
        vec![REFUSED_JOB.into()],
      )
      .await;

    Self {
      refusing,
      tried: vec![],
      prompts: vec![],
      ground: &bench.ground,
      log: RunLog::new(Target {
        agent: "coder".to_owned(),
        run: format!("job-{REFUSED_JOB}"),
        chat_id: None,
        job_id: Some(REFUSED_JOB),
      }),
    }
  }
}

impl Renewing for Refusing<'_> {}

impl Attempt<String> for Refusing<'_> {
  async fn attempt(&mut self, account: Option<Account>) -> Result<String, Failure> {
    let id = account.unwrap().id;

    self.tried.push(id.clone());

    if self.refusing.contains(&id.as_str()) {
      return Err(of_unanswered(CLOUDFLARE, "coder"));
    }

    Ok(id)
  }

  async fn passed_over(&mut self, passed: &PassedOver<'_>) {
    let mut moves = JobMoves::of(self.ground, &mut self.log, Some(REFUSED_JOB), "claude-opus-5-5");

    self.prompts.push(moves.passed_over(passed, "Do the work.").await);
  }
}

// The notes and the roads the refused job keeps.
async fn refused_job(bench: &Bench) -> (Vec<String>, serde_json::Value) {
  let notes = store::all(
    bench.scratch.db(),
    r#"SELECT "text" FROM "agent_log" WHERE "job_id" = $1 AND "kind" = 'note' ORDER BY "id""#,
    vec![REFUSED_JOB.into()],
  )
  .await
  .unwrap()
  .iter()
  .map(|row| row.try_get("", "text").unwrap())
  .collect();
  let roads: Option<String> = store::one(
    bench.scratch.db(),
    r#"SELECT "roads" FROM "jobs" WHERE "id" = $1"#,
    vec![REFUSED_JOB.into()],
  )
  .await
  .unwrap()
  .unwrap()
  .try_get("", "roads")
  .unwrap();

  (
    notes,
    roads.map_or(serde_json::Value::Null, |roads| serde_json::from_str(&roads).unwrap()),
  )
}

// A login whose run the API's firewall refused with a 403 rests alone, though its credential states a moment
// an hour ahead, which lifts its mark of waiting for a person at once; the same job goes out on the next
// login, opened with the line saying it continues the work; and the job's log and its roads name the login
// that refused and its own words.
#[tokio::test]
async fn a_login_that_refuses_a_job_s_turn_rests_alone_and_the_job_goes_out_on_the_next() {
  let bench = Bench::new().await;

  std::fs::write(
    bench.homes.path().join("main").join(CREDENTIAL_FILE),
    credential(now_ms() as i64 + HOUR * 1000),
  )
  .unwrap();

  let mut turn = Refusing::of(&bench, vec!["main"]).await;
  let walked = on_accounts(&bench.walking(), "coder", None, &mut turn).await;

  assert_eq!(walked, Ok("default".to_owned()));
  assert_eq!(turn.tried, ["main", "default"]);

  let rest: i64 = bench.setting("rest_until:main").await.expect("main rests").parse().unwrap();

  assert!(rest > now() + 20 * 60, "main rests for the provider's span: {rest}");
  assert_eq!(bench.setting("rest_until:default").await, None);
  assert_eq!(bench.walk().await, ["default"], "the rest holds where the credential lifts the mark");

  let why = format!("Claude account main refused the turn: \"{CLOUDFLARE}\"");
  let (notes, roads) = refused_job(&bench).await;

  assert_eq!(notes, [format!("{why}. The job goes out again on Claude account default.")]);
  assert_eq!(roads[0]["account"], "default");
  assert_eq!(roads[0]["provider"], "claude-code");
  assert_eq!(roads[0]["reason"], why.as_str());

  let prompt = turn.prompts[0].clone().expect("a prompt for the next login");

  assert!(prompt.contains("already ran on claude-code · claude-opus-5-5 (main)"), "{prompt}");
  assert!(prompt.ends_with("Do the work."), "{prompt}");
}

// A run every login of which refuses ends on the login failure naming each login with its words and saying a
// person must sign in, never on the last refusal's bare line; each login rests, and the job's log says no
// login is left.
#[tokio::test]
async fn a_run_every_login_of_which_refuses_ends_on_the_login_failure_naming_each() {
  let bench = Bench::new().await;

  for id in ["main", "default"] {
    std::fs::write(
      bench.homes.path().join(id).join(CREDENTIAL_FILE),
      credential(now_ms() as i64 + HOUR * 1000),
    )
    .unwrap();
  }

  let mut turn = Refusing::of(&bench, vec!["main", "default"]).await;
  let walked = on_accounts(&bench.walking(), "coder", None, &mut turn).await;

  assert_eq!(
    walked,
    Err(Failure::Account {
      failure: AccountFailure::Login,
      detail: format!(
        "claude coder: every Claude account of the road refused this run, and a person must sign in to each \
         again: main answered \"{CLOUDFLARE}\"; default answered \"{CLOUDFLARE}\""
      ),
    })
  );
  assert_eq!(turn.tried, ["main", "default"]);
  assert!(bench.setting("rest_until:main").await.is_some());
  assert!(bench.setting("rest_until:default").await.is_some());

  let (notes, _) = refused_job(&bench).await;

  assert_eq!(
    notes.last().map(String::as_str),
    Some(
      format!(
        "Claude account default refused the turn: \"{CLOUDFLARE}\". No Claude account of the road is left."
      )
      .as_str()
    )
  );
  assert_eq!(turn.prompts.last(), Some(&None));
}

// What the CLI answers a run on a login whose sign-in has ended.
const SIGNED_OUT: &str = "Not logged in · Please run /login";

// A subscription login whose access token ended a minute ago and whose file states a refresh a month ahead:
// a sign-in that ended before the moment its file states, as only a run on it can tell.
fn ended_early() -> String {
  let now = now_ms() as i64;

  json!({ "claudeAiOauth": {
    "accessToken": "a", "refreshToken": "r", "expiresAt": now - 60_000,
    "refreshTokenExpiresAt": now + 720 * HOUR * 1000,
  } })
  .to_string()
}

// A job's turn on this machine as the handed login answers it: the logins named answer, a token for the work
// nodes answers while the turn says so, and anything else is refused as an ended sign-in refuses it, a
// renewal among them. Every login it went out on is kept, with what it was handed.
struct Handed {
  data: std::path::PathBuf,
  logins: Vec<&'static str>,
  token_answers: bool,
  tried: Vec<String>,
}

impl Handed {
  fn of(bench: &Bench, logins: Vec<&'static str>) -> Self {
    Self {
      data: bench.data.path().to_owned(),
      logins,
      token_answers: true,
      tried: vec![],
    }
  }
}

impl Renewing for Handed {
  async fn renew(&mut self, _ground: &Ground, account: &Account) -> Result<(), Failure> {
    self.tried.push(format!("{} renewed", account.id));

    if self.logins.contains(&account.id.as_str()) {
      Ok(())
    } else {
      Err(of_unanswered(SIGNED_OUT, "coder"))
    }
  }
}

impl Attempt<String> for Handed {
  async fn attempt(&mut self, account: Option<Account>) -> Result<String, Failure> {
    let account = account.unwrap();
    let env = account_env(
      Some(&account),
      &Runs::Here {
        data_root: self.data.clone(),
      },
    )
    .unwrap();
    let on_token = env["CLAUDE_CODE_OAUTH_TOKEN"].as_deref() == Some("sk-ant-oat01-node");

    self
      .tried
      .push(format!("{} on its {}", account.id, if on_token { "token" } else { "login" }));

    if (on_token && self.token_answers) || (!on_token && self.logins.contains(&account.id.as_str())) {
      Ok(account.id)
    } else {
      Err(of_unanswered(SIGNED_OUT, "coder"))
    }
  }
}

impl Bench {
  // `main`'s token for the work nodes, kept where this machine keeps it.
  async fn node_token(&self) {
    let main = walk_for(self.judging())
      .await
      .unwrap()
      .into_iter()
      .find(|one| one.id == "main")
      .unwrap();
    let file = main.node_token_file(self.data.path()).unwrap();

    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, "sk-ant-oat01-node\n").unwrap();
  }

  async fn marked(&self, id: &str) -> bool {
    store::one(
      self.scratch.db(),
      r#"SELECT "needs_login_at" FROM "claude_accounts" WHERE "id" = $1"#,
      vec![id.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<Option<i64>>("", "needs_login_at")
    .unwrap()
    .is_some()
  }
}

// A sign-in that ended before its file says: the renewal its run asks for is refused, the login is marked and
// takes no rest, and the same run goes out again on the same account on its token for the work nodes, beside
// its directory; the next run goes out on the token at once, renewing nothing; and once a person signs in
// again the mark lifts off the credential and the next run is handed the login alone.
#[tokio::test]
async fn a_login_here_that_ends_hands_its_runs_to_its_token_and_takes_them_back_once_signed_in() {
  let bench = Bench::new().await;
  let credentials = bench.homes.path().join("main").join(CREDENTIAL_FILE);

  bench.node_token().await;
  std::fs::write(&credentials, ended_early()).unwrap();

  let mut turn = Handed::of(&bench, vec![]);

  assert_eq!(on_accounts(&bench.walking(), "coder", None, &mut turn).await, Ok("main".to_owned()));
  assert_eq!(turn.tried, ["main renewed", "main on its token"]);
  assert!(bench.marked("main").await, "the login here waits for a person");
  assert_eq!(bench.setting("rest_until:main").await, None, "an account on its token takes no rest");
  assert_eq!(bench.walk().await, ["main", "default"], "the account stays open on its token");

  let mut next = Handed::of(&bench, vec![]);

  assert_eq!(on_accounts(&bench.walking(), "coder", None, &mut next).await, Ok("main".to_owned()));
  assert_eq!(next.tried, ["main on its token"]);

  // A credential that still states a moment ahead, never written since the mark, does not take the run back to
  // the login that refused it.
  bench
    .scratch
    .run(
      r#"UPDATE "claude_accounts" SET "needs_login_at" = "needs_login_at" + 5 WHERE "id" = 'main'"#,
      vec![],
    )
    .await;

  let mut still = Handed::of(&bench, vec![]);

  std::fs::write(&credentials, credential(now_ms() as i64 + HOUR * 1000)).unwrap();
  assert_eq!(on_accounts(&bench.walking(), "coder", None, &mut still).await, Ok("main".to_owned()));
  assert_eq!(still.tried, ["main on its token"]);

  // Signed in again, after the mark.
  bench
    .scratch
    .run(
      r#"UPDATE "claude_accounts" SET "needs_login_at" = "needs_login_at" - 10 WHERE "id" = 'main'"#,
      vec![],
    )
    .await;
  std::fs::write(&credentials, credential(now_ms() as i64 + HOUR * 1000)).unwrap();

  let mut back = Handed::of(&bench, vec!["main"]);

  assert_eq!(on_accounts(&bench.walking(), "coder", None, &mut back).await, Ok("main".to_owned()));
  assert_eq!(back.tried, ["main on its login"]);
  assert!(!bench.marked("main").await, "the mark lifted off the credential");
}

// A run here on the token that fails on its login has found the token gone: it is marked gone, the account,
// its login here gone too, is closed, and the run goes out on the next account.
#[tokio::test]
async fn a_token_that_fails_here_is_marked_gone_and_the_run_goes_to_the_next_account() {
  let bench = Bench::new().await;

  bench.node_token().await;
  std::fs::write(bench.homes.path().join("main").join(CREDENTIAL_FILE), ended_early()).unwrap();
  std::fs::write(
    bench.homes.path().join("default").join(CREDENTIAL_FILE),
    credential(now_ms() as i64 + HOUR * 1000),
  )
  .unwrap();

  let mut turn = Handed {
    token_answers: false,
    ..Handed::of(&bench, vec!["default"])
  };
  let walked = on_accounts(&bench.walking(), "coder", None, &mut turn).await;

  assert_eq!(turn.tried, ["main renewed", "main on its token", "default on its login"]);
  assert_eq!(walked, Ok("default".to_owned()));
  assert!(bench.setting("claude_node_token_gone:main").await.is_some());
  assert_eq!(bench.walk().await, ["default"]);
}

// A supervisor's turn into a program handed a directory account's token for the work nodes that fails on its
// login marks that token gone, as a job's run does; a turn that fails on anything else, and a program on the
// login alone, marks nothing.
#[tokio::test]
async fn a_turn_refused_on_the_token_for_the_work_nodes_marks_that_token_gone() {
  let bench = Bench::new().await;

  bench.node_token().await;

  let home = bench.homes.path().join("main").to_string_lossy().into_owned();
  let on_login = std::collections::BTreeMap::from([("CLAUDE_CONFIG_DIR".to_owned(), home.clone())]);
  let mut on_token = on_login.clone();

  on_token.insert("CLAUDE_CODE_OAUTH_TOKEN".to_owned(), "sk-ant-oat01-node".to_owned());

  let refused = "Please run /login · API Error: 401 OAuth token has been revoked";

  node_token_refused(&bench.ground, &on_token, "API Error: 529 Overloaded").await;
  node_token_refused(&bench.ground, &on_login, refused).await;
  assert_eq!(bench.setting("claude_node_token_gone:main").await, None);

  node_token_refused(&bench.ground, &on_token, refused).await;
  assert!(bench.setting("claude_node_token_gone:main").await.is_some());
}
