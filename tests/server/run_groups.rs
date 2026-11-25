//! What a run leaves running, taken down when the run ends (docs/run-groups.md): a run's processes found
//! by its name inside its window and by its place outside its program's session, killed where they ignore
//! the ask; and the launcher and its reaper leaving nothing of a run behind, whether its program finished,
//! was hung up on or was killed.

use std::ffi::OsString;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use agency_server::run_groups::{
  LAUNCH_VERB, Mark, PLACE_VARIABLE, RUN_NAME, Record, Timing, record_path, take_down, ticks_now,
};

use crate::support::binary;

// How long a condition the test waits on may take before the test fails.
const BOUND: Duration = Duration::from_secs(30);

// The open files a launched run may hold at once: well under the number of processes any machine runs, as a
// unit's soft limit is under a busy machine's, so a take-down that held a file for each process it read would
// miss the run's own.
const FEW_FILES: libc::rlim_t = 32;

// A take-down's waits, short: the test's processes end at once on what they are sent.
const SHORT: Timing = Timing {
  grace: Duration::from_millis(300),
  kill_wait: Duration::from_secs(5),
  look: Duration::from_millis(20),
};

fn wait_until(what: &str, mut holds: impl FnMut() -> bool) {
  let deadline = Instant::now() + BOUND;

  while !holds() {
    assert!(Instant::now() < deadline, "{what} did not come within {BOUND:?}");
    std::thread::sleep(Duration::from_millis(20));
  }
}

fn start_tick(pid: u32) -> u64 {
  procfs::process::Process::new(pid as i32)
    .and_then(|process| process.stat())
    .map(|stat| stat.starttime)
    .unwrap()
}

fn runs(pid: u32) -> bool {
  procfs::process::Process::new(pid as i32)
    .and_then(|process| process.stat())
    .is_ok_and(|stat| stat.state != 'Z')
}

// Every process still running whose environment names `value` under `variable`.
fn named(variable: &str, value: &str) -> Vec<i32> {
  let Ok(all) = procfs::process::all_processes() else { return vec![] };

  all
    .flatten()
    .filter(|process| process.stat().is_ok_and(|stat| stat.state != 'Z'))
    .filter(|process| {
      process
        .environ()
        .is_ok_and(|environ| environ.get(&OsString::from(variable)).is_some_and(|named| named == value))
    })
    .map(|process| process.pid)
    .collect()
}

// Takes down everything a test marked, however the test ended, with the children it started reaped.
struct Marked {
  variable: &'static str,
  value: String,
  children: Vec<Child>,
}

impl Marked {
  fn new(variable: &'static str, value: String) -> Self {
    Self {
      variable,
      value,
      children: vec![],
    }
  }

  fn start(&mut self, script: &str, value: &str) -> u32 {
    let child = Command::new("setsid")
      .args(["sh", "-c", script])
      .env(self.variable, value)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .unwrap();
    let pid = child.id();

    self.children.push(child);
    pid
  }

  fn reap(&mut self) {
    for child in &mut self.children {
      let _ = child.try_wait();
    }
  }
}

impl Drop for Marked {
  fn drop(&mut self) {
    for pid in named(self.variable, &self.value)
      .into_iter()
      .chain(named(self.variable, &format!("{}-other", self.value)))
    {
      // SAFETY: a signal to a process by its number touches nothing of this process's memory.
      unsafe {
        libc::kill(pid, libc::SIGKILL);
      }
    }

    for child in &mut self.children {
      let _ = child.kill();
      let _ = child.wait();
    }
  }
}

// A name no other test of this run holds, since the tests run at once.
fn unique(what: &str) -> String {
  static TAKEN: AtomicUsize = AtomicUsize::new(0);

  format!("{what}-{}-{}", std::process::id(), TAKEN.fetch_add(1, Ordering::Relaxed))
}

#[test]
fn a_run_is_taken_down_by_its_name_inside_its_window_and_what_ignores_the_ask_is_killed() {
  let name = unique("run-window");
  let mut marked = Marked::new(RUN_NAME, name.clone());
  let other = marked.start("exec sleep 600", &format!("{name}-other"));
  let deaf = marked.start("trap '' TERM; sleep 600 & wait", &name);
  let daemon = marked.start("exec sleep 600", &name);

  wait_until("the deaf shell's sleep", || named(RUN_NAME, &name).len() >= 3);

  let newest = named(RUN_NAME, &name).into_iter().map(|pid| start_tick(pid as u32)).max().unwrap();

  wait_until("a later clock tick", || ticks_now() > newest + 1);

  let later = marked.start("exec sleep 600", &name);
  let until = start_tick(later);

  let taken = take_down(
    &Mark::Run {
      name: name.clone(),
      from: 0,
      until: Some(until),
      group: None,
    },
    SHORT,
  );

  marked.reap();

  assert!(taken.found.len() >= 3, "found {:?}", taken.found);
  assert!(!taken.left);
  assert!(!runs(deaf) && !runs(daemon), "the run's own processes still run");
  assert_eq!(named(RUN_NAME, &name), [later as i32], "only the later run's process stands");
  assert!(runs(other), "a process of another name was taken");
}

#[test]
fn a_standing_programs_run_takes_down_only_what_began_since_outside_the_programs_session() {
  let place = unique("/places/standing");
  let mut marked = Marked::new(PLACE_VARIABLE, place.clone());
  let before = marked.start("exec sleep 600", &place);

  wait_until("a later clock tick", || ticks_now() > start_tick(before) + 1);

  let from = ticks_now();
  let program = marked.start("sleep 600 & wait", &place);
  let server = marked.start("exec sleep 600", &place);

  wait_until("the program's own child", || named(PLACE_VARIABLE, &place).len() >= 4);

  let taken = take_down(
    &Mark::Place {
      place: place.clone(),
      from,
      program_session: program as i32,
    },
    SHORT,
  );

  marked.reap();

  assert_eq!(taken.found.iter().map(|seen| seen.pid).collect::<Vec<_>>(), [server as i32]);
  assert!(!runs(server));
  assert!(runs(before) && runs(program), "what the run did not start was taken");
  assert_eq!(named(PLACE_VARIABLE, &place).len(), 3, "the program's own child was taken");
}

// How a launched run's program comes to its end.
#[derive(Clone, Copy, Debug)]
enum Ending {
  Finishes,
  HungUp,
  Killed,
}

// A run launched as a pane launches it: a background process, a daemon that leaves the program's session
// and is re-parented, and one that moves itself out of the run's group as a browser does; then the program
// ends the way the test asks. Nothing of the run is left once its reaper is done, though the launcher, its
// reaper and the run may hold only a few files open.
fn a_launched_run_leaves_nothing(ending: Ending) {
  let data = tempfile::tempdir().unwrap();
  let name = unique("run-launched");
  let mut marked = Marked::new(RUN_NAME, name.clone());
  let record = record_path(data.path(), &unique("bmo-execution"));
  let own_group = std::fs::read_to_string("/proc/self/cgroup")
    .ok()
    .and_then(|line| Some(Path::new("/sys/fs/cgroup").join(line.trim().strip_prefix("0::/")?)))
    .unwrap_or_default();
  let end = match ending {
    Ending::Finishes => "exit 0",
    Ending::HungUp | Ending::Killed => "exec sleep 600",
  };
  let script = format!(
    "sleep 600 & setsid sleep 600 & sh -c 'echo $$ > \"$ESCAPE_TO/cgroup.procs\" 2>/dev/null; exec setsid sleep 600' & {end}"
  );
  let mut launch = Command::new(binary::path("bmo-agency"));

  launch
    .args([LAUNCH_VERB, &name, "--record"])
    .arg(&record)
    .args(["--", "sh", "-c", &script])
    .env(RUN_NAME, &name)
    .env("ESCAPE_TO", &own_group)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());

  // SAFETY: getrlimit and setrlimit are async-signal-safe and touch only the child's own limit.
  unsafe {
    launch.pre_exec(|| {
      let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
      };

      if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) < 0 {
        return Err(std::io::Error::last_os_error());
      }

      limit.rlim_cur = limit.rlim_cur.min(FEW_FILES);

      if libc::setrlimit(libc::RLIMIT_NOFILE, &limit) < 0 {
        return Err(std::io::Error::last_os_error());
      }

      Ok(())
    });
  }

  let mut program = launch.spawn().unwrap();
  let pid = program.id();

  wait_until("the launcher's record with its reaper", || {
    Record::read(&record).is_some_and(|written| written.reaper.is_some())
  });

  let written = Record::read(&record).unwrap();
  let reaper = written.reaper.clone().unwrap();

  assert_eq!(written.program.pid, pid as i32, "the program keeps the launcher's process");
  assert_eq!(written.run, name);

  if !matches!(ending, Ending::Finishes) {
    wait_until("the run's processes", || named(RUN_NAME, &name).len() >= 4);

    let sent = if matches!(ending, Ending::HungUp) { libc::SIGHUP } else { libc::SIGKILL };

    // SAFETY: a signal to the test's own child touches nothing of this process's memory.
    unsafe {
      libc::kill(pid as i32, sent);
    }
  }

  let _ = program.wait();
  marked.children.push(program);

  wait_until("the record removed", || !record.exists());
  wait_until("the reaper gone", || !runs(reaper.pid as u32));

  assert_eq!(named(RUN_NAME, &name), Vec::<i32>::new(), "{ending:?}: the run left processes behind");

  if let Some(group) = &written.group {
    assert!(!group.exists(), "{ending:?}: the run's group was left");
  }

  let log = std::fs::read_to_string(data.path().join("run-groups.log")).unwrap_or_default();

  assert!(log.contains(&format!("run {name} ")), "{ending:?}: no line was written: {log}");
}

#[test]
fn a_run_that_finishes_leaves_nothing_running() {
  a_launched_run_leaves_nothing(Ending::Finishes);
}

#[test]
fn a_run_hung_up_on_as_a_stop_closes_its_pane_leaves_nothing_running() {
  a_launched_run_leaves_nothing(Ending::HungUp);
}

#[test]
fn a_run_whose_program_is_killed_leaves_nothing_running() {
  a_launched_run_leaves_nothing(Ending::Killed);
}
