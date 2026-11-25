//! One execution's supervisor, the `supervise` verb (docs/agent-control.md, 「The host」): it holds
//! its envelope, waits for the grant, asks the store once more, starts the child in a process
//! group of its own, copies its output byte for byte, stops the group when it is asked to, and publishes
//! the outcome once the output is whole and the group is gone. It does what one execution needs and nothing else, and outlives
//! the host that started it.

use std::fs;
use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::disk::{private_file, publish};
use crate::envelope::{Cause, Envelope, EnvelopeDir, Outcome, Started};
use crate::identity::{ProcessIdentity, group_running, process_birth};
use crate::policy::{Refusal, refusal};
use crate::signals::{self, signal_group};

const LOOK_EVERY: Duration = Duration::from_millis(25);

/// This program, as the kernel holds it: still runnable after a deployment renamed a new binary over
/// the path it was started from.
pub const OWN_PROGRAM: &str = "/proc/self/exe";

struct Supervision {
  dir: EnvelopeDir,
  envelope: Envelope,
}

impl Supervision {
  fn owner_gone(&self) -> bool {
    self
      .envelope
      .owner
      .as_ref()
      .is_some_and(|owner| process_birth(owner.pid).as_deref() != Some(owner.birth.as_str()))
  }

  fn cancelled(&self) -> bool {
    self.dir.has("cancel")
  }

  // The envelope held for this supervisor alone: its identity written whole beside the name and
  // linked into place, so a second supervisor started for the same envelope finds it held.
  fn hold(&self) -> io::Result<bool> {
    let me = ProcessIdentity::own();
    let draft = self.dir.path(&format!("supervisor.{}", me.pid));

    publish(&draft, &me)?;

    let held = match fs::hard_link(&draft, self.dir.supervisor()) {
      Ok(()) => Ok(true),
      Err(why) if why.kind() == io::ErrorKind::AlreadyExists => Ok(false),
      Err(why) => Err(why),
    };

    let _ = fs::remove_file(&draft);

    held
  }

  // Ends with no child: empty output, the reason on the error stream, and the outcome.
  fn refuse(&self, cause: Cause, reason: &str) -> io::Result<()> {
    private_file(&self.dir.path("stdout"))?;
    private_file(&self.dir.path("stderr"))?.write_all(reason.as_bytes())?;
    publish(
      &self.dir.outcome(),
      &Outcome {
        exit_code: 1,
        cause,
      },
    )
  }

  // The grant, waited for until it comes; false where the cancel file, a signal to this process or its
  // collector's disappearance said not to start first.
  fn granted(&self) -> bool {
    while !self.dir.has("grant") {
      if self.cancelled() || signals::asked() || self.owner_gone() {
        return false;
      }

      thread::sleep(LOOK_EVERY);
    }

    true
  }

  // Why the child must not start now, read at the last moment before it would.
  fn refused_at_launch(&self) -> Option<(Cause, String)> {
    if let Some(policy) = &self.envelope.policy {
      match refusal(policy) {
        Err(why) => return Some((Cause::LaunchError, why)),
        Ok(Some(Refusal::Cancelled)) => {
          return Some((Cause::Cancelled, "execution cancelled at child launch".to_owned()));
        }
        Ok(Some(Refusal::Disabled)) => {
          return Some((Cause::LaunchError, "provider disabled at child launch".to_owned()));
        }
        Ok(None) => {}
      }
    }

    (self.cancelled() || self.owner_gone())
      .then(|| (Cause::LaunchError, "Error: launch cancelled or its collector gone".to_owned()))
  }

  fn spawn(&self) -> io::Result<Child> {
    let command = &self.envelope.command;
    let (program, args) = command
      .args
      .split_first()
      .ok_or_else(|| io::Error::other("the command names no program"))?;
    let mut child = Command::new(program);

    child
      .args(args)
      .current_dir(&command.cwd)
      .env_clear()
      .envs(command.env.iter().filter_map(|(name, value)| Some((name, value.as_ref()?))))
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .process_group(0);

    child.spawn()
  }
}

// A stream copied byte for byte into the envelope as it comes.
fn copy(mut from: impl io::Read + Send + 'static, into: fs::File) -> JoinHandle<()> {
  thread::spawn(move || {
    let mut into = into;
    let _ = io::copy(&mut from, &mut into);
  })
}

// The guardian: a second small process holding a pipe from this one, which kills the child's group
// once the pipe closes — at the end of supervision, or when this process is killed outright.
fn guardian(group: i32) -> Option<(Child, ChildStdin)> {
  let mut guard = Command::new(OWN_PROGRAM)
    .arg0("agent-host")
    .args(["guard", &group.to_string()])
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .process_group(0)
    .spawn()
    .ok()?;
  let pipe = guard.stdin.take()?;

  Some((guard, pipe))
}

// The guardian let go once the group is down: its pipe closed, and the guardian waited for.
fn dismiss(guard: Option<(Child, ChildStdin)>) {
  if let Some((mut guard, pipe)) = guard {
    drop(pipe);
    let _ = guard.wait();
  }
}

/// Supervises the execution whose envelope is `directory`.
pub fn supervise(directory: &Path) -> io::Result<()> {
  signals::listen();

  let dir = EnvelopeDir(directory.to_owned());
  let envelope: Envelope = serde_json::from_slice(&fs::read(dir.command())?).map_err(io::Error::other)?;
  let run = Supervision { dir, envelope };

  if !run.hold()? {
    return Ok(());
  }

  if !run.granted() {
    return run.refuse(Cause::LaunchError, "launch cancelled or its collector gone before the grant");
  }

  if run.cancelled() || signals::asked() {
    return run.refuse(Cause::Cancelled, "cancelled before launch");
  }

  if let Some((cause, reason)) = run.refused_at_launch() {
    return run.refuse(cause, &reason);
  }

  let stdout = private_file(&run.dir.path("stdout"))?;
  let mut stderr = private_file(&run.dir.path("stderr"))?;
  let mut child = match run.spawn() {
    Ok(child) => child,
    Err(why) => {
      let _ = write!(stderr, "{why}");
      return publish(
        &run.dir.outcome(),
        &Outcome {
          exit_code: 127,
          cause: Cause::LaunchError,
        },
      );
    }
  };
  let group = child.id() as i32;
  // Standing before the acknowledgement, so a child anyone has been told of never outlives this
  // process.
  let guard = guardian(group);
  let acknowledged = publish(
    &run.dir.started(),
    &Started {
      pid: group,
      birth: process_birth(group).unwrap_or_default(),
      supervisor: ProcessIdentity::own(),
    },
  );

  // A child nobody can be told of is never left running.
  if let Err(why) = acknowledged {
    signal_group(group, libc::SIGKILL);
    let _ = child.wait();
    dismiss(guard);
    return Err(why);
  }

  let input = child.stdin.take();
  let said = run.envelope.command.stdin.clone().unwrap_or_default();
  let feeding = thread::spawn(move || {
    if let Some(mut input) = input {
      let _ = input.write_all(said.as_bytes());
    }
  });
  let copies = [
    child.stdout.take().map(|out| copy(out, stdout)),
    child.stderr.take().map(|err| copy(err, stderr)),
  ];
  let outcome = watch(&run, &mut child, group);

  // The run is over once its leader has ended, so a descendant still standing is an orphan of it and
  // must not keep pipes or model work alive: it is killed at once, and the outcome waits for the group
  // to be gone.
  signal_group(group, libc::SIGKILL);
  group_ended(group);
  let _ = feeding.join();

  for copying in copies.into_iter().flatten() {
    let _ = copying.join();
  }

  dismiss(guard);
  publish(&run.dir.outcome(), &outcome)
}

// The child watched until its leader ends. The cancel file, its collector's disappearance or a signal to
// this process asks the group to stop once, and the leader is then waited on however long its own ending
// takes.
fn watch(run: &Supervision, child: &mut Child, group: i32) -> Outcome {
  let mut cause = Cause::Exited;
  let mut asked = false;

  loop {
    if let Ok(Some(status)) = child.try_wait() {
      return Outcome {
        exit_code: status.code().unwrap_or(1),
        cause,
      };
    }

    if !asked && (run.cancelled() || run.owner_gone() || signals::asked()) {
      cause = Cause::Cancelled;
      asked = true;
      signal_group(group, libc::SIGTERM);
    }

    thread::sleep(LOOK_EVERY);
  }
}

// A SIGKILL is delivered when the kernel next runs its target, which on a busy machine is a while
// after the signal is sent; what follows the kill waits for the group to be gone.
fn group_ended(group: i32) {
  while group_running(group) {
    thread::sleep(LOOK_EVERY);
  }
}

/// The guardian's own verb: waits for its standard input to close, then kills the group.
pub fn guard(group: i32) -> io::Result<()> {
  if group <= 1 {
    return Err(io::Error::other(format!("{group} is not a process group")));
  }

  let _ = io::copy(&mut io::stdin(), &mut io::sink());
  signal_group(group, libc::SIGKILL);

  Ok(())
}
