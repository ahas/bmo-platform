//! What a command on a node that failed is said as (docs/person-nodes.md, 「The workplace on a PC」), and a look for a
//! tool the node did not answer told apart from a tool missing (docs/work-nodes.md, 「The workplace on a node」).

use agency_server::verbs::work_node_setup::{NO_CODE, SetupShell, ShellRun, looked};
use futures_util::future::BoxFuture;

#[test]
fn a_failed_command_says_its_code_and_its_errors_or_its_output_where_its_errors_hold_nothing() {
  let run = |code: i32, stdout: &str, stderr: &str| ShellRun {
    code,
    stdout: stdout.to_owned(),
    stderr: stderr.to_owned(),
  };

  assert_eq!(run(2, "out", "err\n").said(), "it ended with code 2: err");
  // WSL's launcher, failing before any command runs, writes why on its output alone.
  assert_eq!(
    run(-1073741819, "Catastrophic failure\n", "").said(),
    "it ended with code -1073741819: Catastrophic failure"
  );
  assert_eq!(
    run(1, " ", "").said(),
    "it ended with code 1: nothing was written on its output or its errors"
  );
  // A command that came to no code of its own is its errors alone.
  assert_eq!(run(NO_CODE, "", "the hub: no agent attached").said(), "the hub: no agent attached");
}

/// A node whose shell ends every command with the one code it was made with.
struct Ends(i32);

impl SetupShell for Ends {
  fn run<'a>(&'a self, _command: &'a str) -> BoxFuture<'a, ShellRun> {
    let code = self.0;

    Box::pin(async move {
      ShellRun {
        code,
        stdout: String::new(),
        stderr: if code == NO_CODE { "the step's unit did not start".to_owned() } else { String::new() },
      }
    })
  }
}

// "a tool is missing only where the look answers that it is not there, and a look the node did not answer says why"
#[tokio::test]
async fn a_tool_is_missing_only_where_the_look_answers_so_and_a_look_the_node_did_not_answer_says_why() {
  assert_eq!(looked(&Ends(0), "~/root", "git").await, Ok(true));
  assert_eq!(looked(&Ends(1), "~/root", "git").await, Ok(false));
  assert_eq!(
    looked(&Ends(NO_CODE), "~/root", "git").await,
    Err("the step's unit did not start".to_owned())
  );
  assert!(
    looked(&Ends(127), "~/root", "git").await.is_err(),
    "a shell that could not run the look"
  );
}
