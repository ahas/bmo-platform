//! An agent's view file: the lines the host writes, and the file kept to its newest part.

use std::fs;

use agent_host::envelope::{Cause, Outcome};
use agent_host::layout::Layout;
use agent_host::view::*;

#[test]
fn lines_read_with_the_clock_and_the_run_they_name() {
  // 2026-10-05 06:30 UTC is 15:30 in Seoul.
  let at = 1_791_181_800_000;

  assert_eq!(began_line("review", "execution-1", at), "── 15:30 review (execution-1) started");
  assert_eq!(
    ended_line(
      "review",
      at,
      &Outcome {
        exit_code: 0,
        cause: Cause::Exited
      }
    ),
    "── 15:30 review ended"
  );
  assert_eq!(
    ended_line(
      "review",
      at,
      &Outcome {
        exit_code: 2,
        cause: Cause::Exited
      }
    ),
    "── 15:30 review ended · exit code 2"
  );
  assert_eq!(
    ended_line(
      "review",
      at,
      &Outcome {
        exit_code: 1,
        cause: Cause::LaunchError
      }
    ),
    "── 15:30 review launch-error"
  );
  assert_eq!(resting_line(Some(at)), "resting · since 15:30");
  assert_eq!(resting_line(None), "resting");
}

#[test]
fn a_failed_run_keeps_the_last_lines_it_said_on_its_error_stream() {
  let failed = Outcome {
    exit_code: 1,
    cause: Cause::Exited,
  };
  let said = "first\n\nsecond\nthird\nsearch run failed: permission denied\n\n";

  assert_eq!(
    failure_lines(&failed, said),
    ["   second", "   third", "   search run failed: permission denied"]
  );
  assert_eq!(
    failure_lines(&failed, &"y".repeat(FAILURE_WIDTH + 5)),
    [format!("   {}…", "y".repeat(FAILURE_WIDTH))]
  );
  assert!(failure_lines(&failed, "").is_empty());
  assert!(
    failure_lines(
      &Outcome {
        exit_code: 0,
        cause: Cause::Exited
      },
      said
    )
    .is_empty()
  );
}

#[test]
fn a_view_is_kept_to_its_newest_part() {
  let dir = tempfile::tempdir().unwrap();
  let layout = Layout::new(dir.path());

  fs::create_dir(&layout.executions).unwrap();

  let views = Views::new(&layout);
  let line = "x".repeat(1023);

  for _ in 0..600 {
    views.write("agent", std::slice::from_ref(&line)).unwrap();
  }

  let kept = fs::read_to_string(views.path("agent")).unwrap();

  assert!(kept.len() <= CEILING as usize);
  assert!(kept.len() >= KEPT - 1024);
  assert!(kept.lines().all(|one| one == line));
}
