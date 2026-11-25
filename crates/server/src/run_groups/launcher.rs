//! The launcher a run's pane starts on and the reaper it leaves behind (docs/run-groups.md, 「The run
//! group」, 「Who takes it down」).

use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use agent_host::identity::ProcessIdentity;

use super::processes::{enter_group, make_group, own_group, remove_group};
use super::{RUN_NAME, Record, Timing, end_record, log_path, record_path};

/// The verb of agency's binary a run's pane is started on.
pub const LAUNCH_VERB: &str = "run-group";
/// The verb the launcher starts the run's reaper on.
pub const REAP_VERB: &str = "run-group-reap";

/// The argv a run's pane runs: agency's binary under the launcher's verb, the run's name and its record,
/// and the program's own argv after them.
pub fn launch_line(
  binary: &Path,
  data: &Path,
  run: &str,
  execution: &str,
  command: &[String],
) -> Vec<String> {
  let mut line = vec![
    binary.to_string_lossy().into_owned(),
    LAUNCH_VERB.to_owned(),
    run.to_owned(),
    "--record".to_owned(),
    record_path(data, execution).to_string_lossy().into_owned(),
    "--".to_owned(),
  ];

  line.extend(command.iter().cloned());
  line
}

// The data root a record stands under: the parent of the records' folder.
fn data_of(record: &Path) -> Option<PathBuf> {
  Some(record.parent()?.parent()?.to_path_buf())
}

// The reaper started outside the run's group, in a session of its own and without the run's name, so
// neither the pane's hangup, the group's ending nor a search by the name reaches it.
fn start_reaper(record: &Path, data: &Path) -> std::io::Result<ProcessIdentity> {
  let log = std::fs::OpenOptions::new().create(true).append(true).open(log_path(data))?;
  let mut command = Command::new(std::env::current_exe()?);

  command
    .arg(REAP_VERB)
    .arg(record)
    .env_remove(RUN_NAME)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(log);

  // SAFETY: setsid is async-signal-safe and touches nothing of the parent's memory.
  unsafe {
    command.pre_exec(|| {
      if libc::setsid() < 0 {
        return Err(std::io::Error::last_os_error());
      }

      Ok(())
    });
  }

  let child = command.spawn()?;
  let pid = child.id() as i32;

  Ok(ProcessIdentity::of(pid).unwrap_or(ProcessIdentity {
    pid,
    birth: String::new(),
  }))
}

/// The launcher: the run's group made, its record written and its reaper started, then this process
/// replaced with the program. Answers only where the program could not be started; a run that cannot be
/// grouped or recorded still runs its program.
pub fn launch(run: &str, record: &Path, command: &[String]) -> String {
  let Some(program) = command.first() else {
    return "the launcher was given no program to run".to_owned();
  };

  if let Some(data) = data_of(record) {
    begin(run, record, &data);
  }

  let failed = Command::new(program).args(&command[1..]).exec();

  format!("{program}: {failed}")
}

// What the launcher does before the program: nothing of it stops the program from starting.
fn begin(run: &str, path: &Path, data: &Path) {
  let execution = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
  let group = own_group().and_then(|parent| make_group(&parent, &execution).ok());
  let mut record = Record {
    execution,
    run: run.to_owned(),
    cwd: std::env::current_dir()
      .map(|cwd| cwd.to_string_lossy().into_owned())
      .unwrap_or_default(),
    group: group.clone(),
    program: ProcessIdentity::own(),
    reaper: None,
    started_at: crate::now_ms(),
  };

  if record.write(path).is_err() {
    if let Some(group) = &group {
      remove_group(group);
    }

    return;
  }

  record.reaper = start_reaper(path, data).ok();

  if let Some(group) = &group
    && enter_group(group).is_err()
  {
    remove_group(group);
    record.group = None;
  }

  let _ = record.write(path);
}

// Waits for a process that is not this one's child to end: on a descriptor of the process where the
// kernel gives one, else by looking again.
fn wait_for(program: &ProcessIdentity, timing: Timing) {
  // SAFETY: pidfd_open takes two integers and answers a descriptor or -1.
  let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, program.pid, 0) } as i32;

  if descriptor >= 0 && program.alive() {
    let mut asked = libc::pollfd {
      fd: descriptor,
      events: libc::POLLIN,
      revents: 0,
    };

    // SAFETY: one pollfd this function owns, waited on with no timeout.
    while unsafe { libc::poll(&mut asked, 1, -1) } < 0
      && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
    {}
  }

  if descriptor >= 0 {
    // SAFETY: the descriptor is this function's own and closed once.
    unsafe {
      libc::close(descriptor);
    }
  }

  while program.alive() {
    std::thread::sleep(timing.look);
  }
}

/// The reaper: waits on the run's program and takes the run down once it has ended, writing a line where
/// the run left anything.
pub fn reap(path: &Path) -> Result<(), String> {
  // SAFETY: ignoring a signal touches nothing of this process's memory.
  unsafe {
    libc::signal(libc::SIGHUP, libc::SIG_IGN);
  }

  let data = data_of(path).ok_or_else(|| format!("{} stands under no data root", path.display()))?;
  let timing = Timing::policy();
  // The launcher writes the reaper into the record after starting it; the program is already named.
  let record = Record::read(path).ok_or_else(|| format!("{}: no record to reap", path.display()))?;

  wait_for(&record.program, timing);

  // The record as it now stands: the launcher may have dropped the group it could not enter.
  let record = Record::read(path).unwrap_or(record);

  end_record(&data, path, &record, timing);

  Ok(())
}
