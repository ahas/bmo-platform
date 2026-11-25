//! Agency's own server as a test stands it: started on the store and a data root of the test's own, under a
//! home of the test's own so it reaches no herdr of the machine's, its control port and its quota source
//! each on a port the machine picks, once a seat of the test server's connections is free for its pool;
//! and its control port once it listens. The machine's own Claude login stands signed in under that home,
//! where the variables name no directory of their own. It is stopped when the guard goes, and its seat given
//! back.

#![allow(dead_code)]

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use crate::support::binary;
use crate::support::connections::Seat;
use crate::support::grounds::signed_in;

/// A server standing, and the port its control routes listen on.
pub struct Standing {
  child: Child,
  pub port: u16,
  // Given back after the drop has ended the child, and its connections with it.
  _seat: Seat,
}

impl Standing {
  /// The process the server runs as, for a test that reads what its environment came to.
  pub fn pid(&self) -> u32 {
    self.child.id()
  }

  /// Asked to stop as the service manager asks it, by `SIGTERM`, and waited on until it has ended: how it
  /// ended, or none where it stood past `bound`.
  pub fn stop(&mut self, bound: std::time::Duration) -> Option<std::process::ExitStatus> {
    unsafe {
      libc::kill(self.child.id() as i32, libc::SIGTERM);
    }

    let until = std::time::Instant::now() + bound;

    loop {
      if let Some(ended) = self.child.try_wait().unwrap() {
        return Some(ended);
      }

      if std::time::Instant::now() >= until {
        return None;
      }

      std::thread::sleep(std::time::Duration::from_millis(20));
    }
  }
}

impl Drop for Standing {
  fn drop(&mut self) {
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

/// The server started under `home` on the data root `data` and the store `store`, under `token`.
pub fn stand_server(home: &Path, data: &Path, store: &str, token: &str) -> Standing {
  stand_server_with(home, data, store, token, &[])
}

/// The server started as `stand_server` starts it, with these variables beside its own.
pub fn stand_server_with(
  home: &Path,
  data: &Path,
  store: &str,
  token: &str,
  variables: &[(&str, &str)],
) -> Standing {
  // The test server's environment is built here, not inherited: the worker runs the suite with the
  // deployment's own peer addresses and tokens in its environment, and a test must reach none of them. A
  // test that needs a stand-in names it in `variables`; the marker keeps even that from being called.
  let path = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned());
  let program = binary::path("agency-server");

  if !variables.iter().any(|(name, _)| *name == "CLAUDE_CONFIG_DIR") {
    signed_in(&home.join(".claude"));
  }

  let seat = Seat::take();
  let mut child = Command::new(program)
    .env_clear()
    .env("PATH", path)
    .arg("--data-root")
    .arg(data)
    .current_dir(home)
    .env("HOME", home)
    .env("XDG_CONFIG_HOME", home.join(".config"))
    .env("LANG", "C.UTF-8")
    .env("BMO_DEPLOY_DIR", env!("CARGO_MANIFEST_DIR"))
    .env("BMO_ARCHIVE_URL", store)
    .env("AGENCY_CONTROL_PORT", "0")
    .env("AGENCY_QUOTA_PORT", "0")
    .env("AGENCY_CONTROL_TOKEN", token)
    .envs(variables.iter().copied())
    .env("BMO_TEST_GROUND", "1")
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .unwrap();
  let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
  let port = loop {
    let line = lines.next().expect("the server said where its control routes are").unwrap();

    if let Some(address) = line.strip_prefix("agency: control routes on http://") {
      break address.rsplit(':').next().unwrap().trim().parse().unwrap();
    }
  };

  std::thread::spawn(move || for _ in lines {});

  Standing {
    child,
    port,
    _seat: seat,
  }
}
