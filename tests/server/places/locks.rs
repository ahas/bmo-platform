//! The file locks the server holds: named as the TypeScript side names them, and held until let go.

use std::fs::File;
use std::io::Read;
use std::os::fd::{FromRawFd, OwnedFd};

use agency_server::places::locks::{held, lock_file_name, try_hold};

#[test]
fn a_lock_file_is_named_as_the_typescript_side_names_it() {
  assert_eq!(
    lock_file_name("/home/tester/repos/project"),
    "home-tester-repos-project-a40274a70664.lock"
  );
  assert_eq!(lock_file_name("/"), "root-8a5edab28263.lock");
  assert_eq!(lock_file_name("dev-pane:worker-w2"), "dev-pane-worker-w2-7fdd551d2021.lock");
  assert_eq!(
    lock_file_name(
      "job-hold:/home/tester/repos/a very/long/path/that/goes/on/and/on/past/sixty-four/characters/of/slug/text/here"
    ),
    "that-goes-on-and-on-past-sixty-four-characters-of-slug-text-here-28c916cfda36.lock"
  );
}

// A child forked while the lock is held keeps a copy of its descriptor, as one forked by any other thread
// of the process does until it execs; the lock still reads free the moment its holder lets it go.
#[test]
fn a_held_lock_reads_held_until_it_is_let_go_whatever_a_forked_child_keeps() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("locks").join("a.lock");

  assert!(!held(&path));

  let holding = try_hold(&path).unwrap();

  assert!(held(&path));
  assert!(try_hold(&path).is_none());

  let child = Forked::keeping_what_is_open();

  drop(holding);
  assert!(!held(&path), "the lock reads free while the forked child {} keeps its copy", child.0);
}

// A child forked from the test that holds a copy of every descriptor open at the fork and stands until the
// test kills it, once it has said it stands.
struct Forked(libc::pid_t);

impl Forked {
  fn keeping_what_is_open() -> Self {
    let mut ends = [0; 2];

    // SAFETY: pipe2 writes two descriptors into the array it is given.
    assert_eq!(unsafe { libc::pipe2(ends.as_mut_ptr(), libc::O_CLOEXEC) }, 0);

    // SAFETY: the child calls only write, pause and _exit, which are async-signal-safe.
    let pid = unsafe { libc::fork() };

    if pid == 0 {
      // SAFETY: the write end is the pipe's own; the child ends only by the test's kill.
      unsafe {
        libc::write(ends[1], b"s".as_ptr().cast(), 1);

        loop {
          libc::pause();
        }
      }
    }

    assert!(pid > 0, "the fork");

    // SAFETY: both ends are this process's own, each owned once from here.
    let (mut said, writing) =
      unsafe { (File::from(OwnedFd::from_raw_fd(ends[0])), OwnedFd::from_raw_fd(ends[1])) };
    let forked = Self(pid);
    let mut word = [0; 1];

    // With the parent's write end closed, the read ends with the child's word or with its end.
    drop(writing);
    assert_eq!(said.read(&mut word).unwrap(), 1, "the forked child stands");
    forked
  }
}

impl Drop for Forked {
  fn drop(&mut self) {
    // SAFETY: a signal to, and the reaping of, the child this test forked.
    unsafe {
      libc::kill(self.0, libc::SIGKILL);
      libc::waitpid(self.0, std::ptr::null_mut(), 0);
    }
  }
}
