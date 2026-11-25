//! A process standing in for a place's program: recorded as the place's running one, so a turn picked up
//! there reads its program alive for as long as the stand-in is held, however slowly the test goes on, and
//! taken down with its descendants when it is dropped.

#![allow(dead_code)]

use agency_server::lines::place::Place;
use agency_server::now_ms;
use serde_json::json;

/// The stand-in program, killed and reaped when it is dropped.
pub struct Program(std::process::Child);

impl Drop for Program {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

/// A stand-in program recorded as the place's running one.
pub fn running(place: &Place) -> Program {
  let program = Program(
    std::process::Command::new("sleep")
      .arg("infinity")
      .stdin(std::process::Stdio::null())
      .spawn()
      .unwrap(),
  );
  let identity = agent_host::identity::ProcessIdentity::of(program.0.id() as i32).unwrap();

  std::fs::create_dir_all(&place.path).unwrap();
  std::fs::write(
    place.path.join("running.json"),
    json!({ "identity": "one", "pid": identity.pid, "birth": identity.birth, "loop": identity,
            "starts": 1, "startedAt": now_ms(), "recent": [] })
    .to_string(),
  )
  .unwrap();

  program
}
