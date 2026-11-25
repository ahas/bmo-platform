//! A worker's dev server and the model catalogue as `bmo-agency` carries them: the dev pane's own decisions,
//! the start verb run as a job's shell runs it over a scratch store, its server a stand-in, and the catalogue
//! verb run the same way, its engine's installer a stand-in.

use crate::integration::common;
use crate::support::binary;

use std::path::Path;

use agency_server::verbs::dev_servers::{
  DevState, DevStep, dev_environment, dev_restart_delay_ms, dev_step, outside_client, rotate_dev_log,
};
use common::Scratch;
use serde_json::{Value, json};

fn wanted(cwd: &str, command: &str) -> Option<String> {
  Some(json!([cwd, command]).to_string())
}

fn state(want: Option<String>, running: Option<String>, start_at: i64) -> DevState {
  DevState {
    wanted: want,
    running,
    start_at,
    ..DevState::default()
  }
}

// "starts the named command once its wait is over, and leaves a matching one alone"
// "stops a server whose command or directory is no longer the row's"
// "with the command taken away, stops what runs and only then ends"
#[test]
fn what_the_dev_server_pane_does_next() {
  let cmd = wanted("/repo", "bun run dev");

  assert_eq!(dev_step(&state(cmd.clone(), None, 0), 1_000), DevStep::Start);
  assert_eq!(dev_step(&state(cmd.clone(), None, 5_000), 1_000), DevStep::Wait);
  assert_eq!(dev_step(&state(cmd.clone(), cmd.clone(), 0), 1_000), DevStep::Wait);
  assert_eq!(
    dev_step(&state(wanted("/repo", "bun run dev --port 2"), cmd.clone(), 0), 1_000),
    DevStep::Stop
  );
  assert_eq!(dev_step(&state(wanted("/other", "bun run dev"), cmd.clone(), 0), 1_000), DevStep::Stop);
  assert_eq!(dev_step(&state(None, cmd.clone(), 0), 1_000), DevStep::Stop);
  assert_eq!(dev_step(&state(None, None, 0), 1_000), DevStep::Exit);
}

// "restarts on request: a matching server is stopped, and a stopped one starts without waiting"
// "ends a server nobody is on while memory is short with the pane, unless a restart was just asked for"
// "ends nothing for its age or for going unasked"
#[test]
fn a_restart_and_memory_short_with_nobody_on_the_server() {
  let cmd = wanted("/repo", "bun run dev");
  let restart = |want: Option<String>, running: Option<String>, start_at: i64| DevState {
    restart: true,
    ..state(want, running, start_at)
  };

  assert_eq!(dev_step(&restart(cmd.clone(), cmd.clone(), 0), 1_000), DevStep::Stop);
  assert_eq!(dev_step(&restart(cmd.clone(), None, 60_000), 1_000), DevStep::Start);
  assert_eq!(dev_step(&restart(None, None, 0), 1_000), DevStep::Exit);

  // A server that has run for days with nobody asking anything of it is left while memory is not short.
  assert_eq!(dev_step(&state(cmd.clone(), cmd.clone(), 0), 7 * 24 * 3_600_000), DevStep::Wait);

  let pressed = DevState {
    pressed: true,
    ..state(cmd.clone(), cmd.clone(), 0)
  };

  assert_eq!(dev_step(&pressed, 1_000), DevStep::Pressed);
  assert_eq!(
    dev_step(
      &DevState {
        restart: true,
        ..pressed.clone()
      },
      1_000
    ),
    DevStep::Stop
  );
  assert_eq!(
    dev_step(
      &DevState {
        running: None,
        ..pressed
      },
      1_000
    ),
    DevStep::Start
  );
}

// "waits longer after each quick ending, up to a minute"
#[test]
fn waits_longer_after_each_quick_ending_up_to_a_minute() {
  assert_eq!(dev_restart_delay_ms(0), 3_000);
  assert_eq!(dev_restart_delay_ms(1), 3_000);
  assert_eq!(dev_restart_delay_ms(2), 6_000);
  assert_eq!(dev_restart_delay_ms(3), 12_000);
  assert_eq!(dev_restart_delay_ms(20), 60_000);
}

// "holds none of the service's variables, and names the workbench"
// "names a support worker's number, so a server that binds ports moves them (docs/worker-slots.md)"
#[test]
fn the_dev_server_s_environment() {
  let pairs = |items: &[(&str, &str)]| {
    items
      .iter()
      .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
      .collect::<Vec<_>>()
  };
  let env = pairs(&[
    ("PATH", "/bin"),
    ("HOME", "/home/tester"),
    ("BMO_BOT_TOKEN", "secret"),
    ("BMO_CONTROL_TOKEN", "secret"),
    ("BMO_RUN_NAME", "bmo-dev-trainer"),
  ]);

  assert_eq!(
    dev_environment(env, Path::new("/data/workbench/trainer"), None),
    pairs(&[
      ("PATH", "/bin"),
      ("HOME", "/home/tester"),
      ("BMO_WORKBENCH_DIR", "/data/workbench/trainer"),
    ])
  );
  assert_eq!(
    dev_environment(
      pairs(&[("PATH", "/bin"), ("BMO_WORKER_SLOT", "9")]),
      Path::new("/data/workbench/worker-w2"),
      Some(2)
    ),
    pairs(&[
      ("PATH", "/bin"),
      ("BMO_WORKBENCH_DIR", "/data/workbench/worker-w2"),
      ("BMO_WORKER_SLOT", "2"),
    ])
  );
}

// "is moved aside past its bound, one generation kept, and left alone below it"
#[test]
fn the_dev_server_s_log_is_moved_aside_past_its_bound() {
  let dir = tempfile::tempdir().unwrap();
  let log = dir.path().join("dev-server.log");

  assert!(!rotate_dev_log(&log, 10));
  std::fs::write(&log, "short").unwrap();
  assert!(!rotate_dev_log(&log, 10));
  std::fs::write(&log, "a line longer than ten bytes").unwrap();
  assert!(rotate_dev_log(&log, 10));
  assert!(!log.exists());
  assert_eq!(
    std::fs::read_to_string(dir.path().join("dev-server.log.1")).unwrap(),
    "a line longer than ten bytes"
  );
}

// "a client outside the group connected to a port the group listens on is use; the group talking to itself is not"
#[test]
fn who_uses_the_dev_server() {
  let group = [
    "p100",
    "f20",
    "n127.0.0.1:3231",
    "f21",
    "n127.0.0.1:13232",
    // The console's server reading the stand-in bot: both ends are the group's.
    "f22",
    "n127.0.0.1:13232->127.0.0.1:40001",
    "p101",
    "f9",
    "n127.0.0.1:40001->127.0.0.1:13232",
    // The server's own call out to a service elsewhere.
    "f10",
    "n127.0.0.1:50000->127.0.0.1:5432",
  ];
  let with = |more: &[&str]| group.iter().chain(more).copied().collect::<Vec<_>>().join("\n");

  assert!(!outside_client(&group.join("\n")));
  assert!(outside_client(&with(&["p100", "f30", "n127.0.0.1:3231->127.0.0.1:41000"])));
  assert!(outside_client(
    &["p100", "f20", "n[::1]:3231", "f21", "n[::1]:3231->[::1]:41000"].join("\n")
  ));
  assert!(!outside_client(""));
}

// The binary as a shell runs it, the store at the address `--db` names, with variables of its own.
fn models(
  scratch: &Scratch,
  data_root: &Path,
  args: &[&str],
  env: &[(&str, String)],
) -> (i32, String, String) {
  let address = scratch.address();
  let done = std::process::Command::new(binary::path("bmo-agency"))
    .arg("model")
    .args(args)
    .args(["--db", &address])
    .env("BMO_DATA_DIR", data_root)
    .envs(env.iter().map(|(name, value)| (*name, value.as_str())))
    .stdin(std::process::Stdio::null())
    .output()
    .unwrap();

  (
    done.status.code().unwrap_or(-1),
    String::from_utf8_lossy(&done.stdout).into_owned(),
    String::from_utf8_lossy(&done.stderr).into_owned(),
  )
}

async fn rvn(scratch: &Scratch, data_root: &Path) {
  let sha = "ab".repeat(32);
  let (code, _, err) = models(
    scratch,
    data_root,
    &[
      "add",
      "rvn",
      "--file",
      "RVN-Q4_K_M-multilingual-mtp.gguf",
      "--bytes",
      "1234",
      "--sha256",
      &sha,
      "--engine",
      "local-llm",
      "--source",
      "https://example.com/rvn.gguf",
    ],
    &[],
  );

  assert_eq!(code, 0, "{err}");
}

// "prints one entry whole, records a node its file lies on, and refuses an entry it does not hold"
#[tokio::test]
async fn the_catalogue_verb_an_installer_asks() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();

  rvn(&scratch, data.path()).await;

  let (code, out, _) = models(&scratch, data.path(), &["--entry", "rvn", "--json"], &[]);
  let read: Value = serde_json::from_str(&out).unwrap();

  assert_eq!(code, 0);
  assert_eq!(
    (read["name"].as_str(), read["file"].as_str()),
    (Some("rvn"), Some("RVN-Q4_K_M-multilingual-mtp.gguf"))
  );
  assert_eq!(read["engine"], "local-llm");
  assert_eq!(models(&scratch, data.path(), &["--placed", "rvn", "node-a"], &[]).0, 0);
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "model_catalogue" WHERE "nodes" = '["node-a"]'"#, vec![])
      .await,
    1
  );

  let (code, _, err) = models(&scratch, data.path(), &["--entry", "gone", "--json"], &[]);

  assert_eq!(code, 2);
  assert!(err.starts_with("models: ") && err.contains("gone"), "{err}");

  let (code, out, _) = models(&scratch, data.path(), &[], &[]);

  assert_eq!(code, 0);
  assert!(out.starts_with("rvn: rvn\n"), "{out}");
  assert!(out.contains("1,234 B · sha256 abababababab"), "{out}");
}

// "runs the engine's installer with the entry and ends done or failed with its last line"
#[tokio::test]
async fn runs_the_engine_s_installer_with_the_entry_and_ends_done_or_failed_with_its_last_line() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let source = data.path().join("source");
  let bin = data.path().join("bin");
  let asked = data.path().join("asked");

  std::fs::create_dir_all(source.join("gate/training/tooling/scripts")).unwrap();
  std::fs::create_dir_all(&bin).unwrap();
  std::fs::write(source.join("gate/training/tooling/scripts/local-llm-install.ts"), "// stand-in\n")
    .unwrap();
  // A stand-in bun that writes the arguments it was given and ends well for rvn alone.
  common::executable(
    &bin.join("bun"),
    &format!(
      "#!/bin/sh\nshift\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$2\" = rvn ]; then echo ok; else echo 'head: setup stopped — model-missing'; exit 1; fi\n",
      asked.display()
    ),
  );
  rvn(&scratch, data.path()).await;
  scratch
    .run(
      r#"INSERT INTO "model_catalogue" ("name", "label", "file", "bytes", "sha256", "engine", "nodes", "added_at")
         VALUES ('trained', 'trained', 'trained.gguf', 1, $1, 'local-llm', '[]', 2)"#,
      vec!["cd".repeat(32).into()],
    )
    .await;

  let env = [
    ("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default())),
    ("AGENCY_SOURCE_DIR", source.display().to_string()),
  ];
  let begin = |entry: &str| {
    format!(
      r#"INSERT INTO "model_uses" ("entry", "engine", "chosen_by", "started_at", "state") VALUES ('{entry}', 'local-llm', 'tester', 1, 'running') RETURNING "id""#
    )
  };
  let first = scratch.count(&begin("rvn"), vec![]).await.to_string();
  let (code, out, err) = models(&scratch, data.path(), &["run-use", &first], &env);

  assert_eq!((code, out.as_str()), (0, "local-llm ← rvn: done\n"), "{err}");

  let second = scratch.count(&begin("trained"), vec![]).await.to_string();
  let (code, out, _) = models(&scratch, data.path(), &["run-use", &second], &env);

  assert_eq!(
    (code, out.as_str()),
    (1, "local-llm ← trained: failed · head: setup stopped — model-missing\n")
  );
  assert_eq!(std::fs::read_to_string(&asked).unwrap(), "--model rvn\n--model trained\n");
  assert_eq!(models(&scratch, data.path(), &["run-use", &second], &env).0, 2);
}

// A process standing in for a checkout's dev server in a process group of its own, as the pane starts one, its
// group taken down when it is dropped.
struct Server(std::process::Child);

impl Server {
  fn stand() -> Self {
    use std::os::unix::process::CommandExt;

    Self(
      std::process::Command::new("sleep")
        .arg("infinity")
        .process_group(0)
        .stdin(std::process::Stdio::null())
        .spawn()
        .unwrap(),
    )
  }

  fn group(&self) -> u32 {
    self.0.id()
  }
}

impl Drop for Server {
  fn drop(&mut self) {
    // SAFETY: the group is the stand-in's own, started above.
    unsafe {
      libc::kill(-(self.0.id() as i32), libc::SIGKILL);
    }
    let _ = self.0.wait();
  }
}

// `bmo-agency dev-server` run as a job's shell runs it on the scratch store and data root, taken down where it
// is dropped before it ends.
struct DevVerb(std::process::Child);

impl DevVerb {
  fn start(scratch: &Scratch, data_root: &Path, action: &str, worker: &str) -> Self {
    Self(
      std::process::Command::new(binary::path("bmo-agency"))
        .args(["dev-server", action, worker])
        .env("HOME", data_root)
        // herdr's socket is read under XDG_CONFIG_HOME before HOME: the verb's dev pane stands in no herdr of
        // the machine running the tests, so none is left behind in it.
        .env("XDG_CONFIG_HOME", data_root.join(".config"))
        .env("BMO_DATA_DIR", data_root)
        .env("BMO_ARCHIVE_URL", scratch.address())
        .env_remove("BMO_HOME_BRIDGE")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap(),
    )
  }

  // Whether it has ended already.
  fn has_ended(&mut self) -> bool {
    self.0.try_wait().unwrap().is_some()
  }

  // Its exit code, standard output and error stream once it has ended, within a bound.
  fn ended(mut self) -> (i32, String, String) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(60);

    while self.0.try_wait().unwrap().is_none() {
      assert!(std::time::Instant::now() < until, "the dev-server verb did not end");
      std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let mut out = String::new();
    let mut err = String::new();

    std::io::Read::read_to_string(self.0.stdout.as_mut().unwrap(), &mut out).unwrap();
    std::io::Read::read_to_string(self.0.stderr.as_mut().unwrap(), &mut err).unwrap();

    (self.0.wait().unwrap().code().unwrap_or(-1), out, err)
  }
}

impl Drop for DevVerb {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

// "Refuses ... a checkout no job is at work in"; a job counts through every stage and every review round, the
// pauses between their turns included, and stops counting once it gives its checkout up or ends
// (docs/worker-bench.md, 「It stands only while a job runs」).
#[tokio::test]
async fn a_job_starts_its_dev_server_in_a_later_stage_and_between_its_rounds() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let checkout = data.path().join("shop");
  let words = &policy::verb_words::VERB_WORDS.dev_servers.server;
  let bench = data.path().join("workbench").join("shop");
  let log = bench.join("dev-server.log").to_string_lossy().into_owned();
  let job = async |state: &str, parked: bool| {
    scratch
      .run(
        r#"UPDATE "jobs" SET "state" = $1, "review_parked" = $2 WHERE "worker_name" = 'shop'"#,
        vec![state.into(), parked.into()],
      )
      .await;
  };

  std::fs::create_dir_all(&checkout).unwrap();
  crate::support::rows::repository(
    scratch.db(),
    "shop",
    &checkout.to_string_lossy(),
    &[("label", "Shop".into()), ("dev_command", "nuxi dev".into())],
  )
  .await;
  // The second of three stages under way, in a turn of its own after the first stage's review.
  scratch
    .run(
      r#"INSERT INTO "jobs" ("item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
              "started_at", "prompt", "review_stages", "review_stage", "review_round")
         VALUES (1, 1, 'tester', 1, 'shop', 'running', 1, 'the item',
                 '["Investigate","Implement","Verify"]', 2, 2)"#,
      vec![],
    )
    .await;

  // Started in the second stage, the verb leaves the request and waits until the pane's server stands.
  let mut start = DevVerb::start(&scratch, data.path(), "start", "shop");
  let until = std::time::Instant::now() + std::time::Duration::from_secs(60);

  while !bench.join("dev-server.wanted").exists() {
    if start.has_ended() {
      let (code, out, err) = start.ended();

      panic!("the start verb ended ({code}) before it left a request: {out}{err}");
    }

    assert!(std::time::Instant::now() < until, "the start verb left no request");
    std::thread::sleep(std::time::Duration::from_millis(20));
  }

  let server = Server::stand();
  let group = server.group().to_string();

  std::fs::write(bench.join("dev-server.pid"), format!("{group}\n")).unwrap();

  let (code, out, err) = start.ended();
  let up = policy::fill_marks(
    &words.up,
    &[("worker", "shop"), ("done", &words.started), ("group", &group), ("log", &log)],
  );

  assert_eq!(code, 0, "{out}{err}");
  assert!(out.contains(&up), "{out}{err}");

  // Between the stage's rounds, its report under review, the job is still at work and the server stands.
  job("reviewing", false).await;

  let (code, out, err) = DevVerb::start(&scratch, data.path(), "start", "shop").ended();
  let already =
    policy::fill_marks(&words.already_up, &[("worker", "shop"), ("group", &group), ("log", &log)]);

  assert_eq!(code, 0, "{out}{err}");
  assert!(out.contains(&already), "{out}{err}");

  // A job that gave its checkout up awaiting the owner's word, or one that ended, is at work no more.
  let no_job = policy::fill_marks(&words.no_job, &[("worker", "shop")]);

  for (state, parked) in [("reviewing", true), ("ended", false)] {
    job(state, parked).await;

    let (code, out, err) = DevVerb::start(&scratch, data.path(), "start", "shop").ended();

    assert_ne!(code, 0, "{state}: {out}");
    assert!(format!("{out}{err}").contains(&no_job), "{state}: {out}{err}");
  }

  drop(server);
}
