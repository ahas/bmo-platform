//! The signals that ask a process of this crate to end, noted rather than obeyed at once, so each
//! verb ends in its own way: a host lets go of its claim, a supervisor stops its child's group.

use std::sync::atomic::{AtomicBool, Ordering};

static ASKED: AtomicBool = AtomicBool::new(false);

extern "C" fn note(_: libc::c_int) {
  ASKED.store(true, Ordering::SeqCst);
}

/// SIGTERM, SIGINT and SIGHUP noted from now on.
pub fn listen() {
  for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
    // SAFETY: the handler only stores into an atomic, which is async-signal-safe.
    unsafe {
      libc::signal(signal, note as *const () as libc::sighandler_t);
    }
  }
}

/// Whether this process has been asked to end.
pub fn asked() -> bool {
  ASKED.load(Ordering::SeqCst)
}

/// A signal sent to every process in the group `group` leads; a group already gone is no fault.
pub fn signal_group(group: i32, signal: libc::c_int) {
  if group > 1 {
    // SAFETY: kill with a negative pid addresses a process group and touches no memory.
    unsafe {
      libc::kill(-group, signal);
    }
  }
}

/// A signal sent to one process.
pub fn signal_process(pid: i32, signal: libc::c_int) {
  if pid > 1 {
    // SAFETY: as above, for one process.
    unsafe {
      libc::kill(pid, signal);
    }
  }
}
