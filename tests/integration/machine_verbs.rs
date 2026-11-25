//! The machine's verbs agency holds (docs/work-nodes.md): a node's pane drawn from the node as the node
//! service lists it, and an account's token for the work nodes made through `bmo-agency node claude-token`
//! over a scratch store and a scratch data root, never a real machine.

use crate::integration::common;
use crate::support::binary;

use std::path::Path;

use agency_server::peers::node::{NodeKind, NodeRow};
use agency_server::verbs::node_pane::node_pane_text;
use common::Scratch;
use policy::fill_marks;
use policy::verb_words::VERB_WORDS;

// "shows the node as the service lists it, how it is reached, its state with what the service says of it,
// and what Enter does": a login on a node reached over SSH, none on one behind a hub
#[test]
fn a_node_s_pane_shows_the_node_its_kind_its_state_and_what_enter_does() {
  let pane = &VERB_WORDS.node_pane;
  let refused = NodeRow {
    node: "pc-node".into(),
    label: Some("home pc".into()),
    kind: NodeKind::Ssh,
    state: "auth".into(),
    detail: Some("Permission denied (publickey,password).".into()),
  };
  let plain = NodeRow {
    node: "agent-a".into(),
    label: None,
    kind: NodeKind::Jake,
    state: "up".into(),
    detail: None,
  };
  let ssh = node_pane_text(&refused);
  let hub = node_pane_text(&plain);
  let detail = fill_marks(&pane.detail, &[("detail", "Permission denied (publickey,password).")]);

  assert!(ssh.starts_with("home pc (pc-node)"));
  assert!(ssh.contains(&fill_marks(&pane.kind, &[("kind", "ssh")])));
  assert!(hub.contains(&fill_marks(&pane.kind, &[("kind", "jake")])));
  assert!(ssh.contains(&fill_marks(&pane.state, &[("state", "auth"), ("detail", &detail)])));
  assert!(ssh.ends_with(&pane.ssh_enter));
  assert!(hub.starts_with("agent-a (agent-a)"));
  assert!(hub.contains(&fill_marks(&pane.state, &[("state", "up"), ("detail", "")])));
  assert!(hub.ends_with(&pane.no_login));
}

async fn second_login(scratch: &Scratch, login: &Path) {
  scratch
    .run(
      r#"INSERT INTO "claude_accounts" ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at")
         VALUES ('account', 'second', 1, 1, 'directory', $1, 1)"#,
      vec![login.to_string_lossy().into_owned().into()],
    )
    .await;
}

// `bmo-agency` run as a shell runs it, on the scratch store and data root, `input` on its standard input:
// its exit code, standard output and error stream.
fn agency(scratch: &Scratch, data_root: &Path, args: &[&str], input: &str) -> (i32, String, String) {
  use std::io::Write;

  let mut child = std::process::Command::new(binary::path("bmo-agency"))
    .args(args)
    .env("HOME", data_root)
    .env("BMO_DATA_DIR", data_root)
    .env("BMO_ARCHIVE_URL", scratch.address())
    .env_remove("BMO_HOME_BRIDGE")
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .unwrap();

  child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();

  let done = child.wait_with_output().unwrap();

  (
    done.status.code().unwrap_or(-1),
    String::from_utf8_lossy(&done.stdout).into_owned(),
    String::from_utf8_lossy(&done.stderr).into_owned(),
  )
}

// "is written into the account's file under the data root, readable by this user alone, and never printed"
#[tokio::test]
async fn a_token_for_the_work_nodes_is_written_readable_by_this_user_alone_and_never_printed() {
  use std::os::unix::fs::PermissionsExt;

  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let token = format!("sk-ant-oat01-{}", "x".repeat(60));

  second_login(&scratch, &data.path().join("second-login")).await;

  let (code, out, err) = agency(
    &scratch,
    data.path(),
    &["node", "claude-token", "second", "--paste"],
    &format!("{token}\n"),
  );
  let file = data.path().join("env/claude-tokens/account");

  assert_eq!(code, 0, "{err}");
  assert_eq!(std::fs::read_to_string(&file).unwrap(), format!("{token}\n"));
  assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
  assert!(!format!("{out}{err}").contains(&token));
  assert!(out.contains(&fill_marks(&VERB_WORDS.claude_token.kept, &[("file", &file.to_string_lossy())])));
}

// "refuses a text that is not such a token, and an account nobody registered, writing nothing"
#[tokio::test]
async fn a_text_that_is_no_token_and_an_account_nobody_registered_are_refused_writing_nothing() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let token = format!("sk-ant-oat01-{}", "x".repeat(60));
  let words = &VERB_WORDS.claude_token;

  second_login(&scratch, &data.path().join("second-login")).await;

  let (cut, _, said) = agency(
    &scratch,
    data.path(),
    &["node", "claude-token", "account", "--paste"],
    "sk-ant-oat01-short",
  );
  let (nobody, _, _) = agency(
    &scratch,
    data.path(),
    &["node", "claude-token", "nobody", "--paste"],
    &format!("{token}\n"),
  );

  assert_eq!(cut, 1);
  assert!(said.contains(&fill_marks(&words.refused, &[("fault", &words.cut)])), "{said}");
  assert_eq!(nobody, 1);
  assert!(!data.path().join("env/claude-tokens/account").exists());
}

// "is made by one command naming the account, quoted for the shell"
#[test]
fn the_token_is_made_by_one_command_naming_the_account_quoted_for_the_shell() {
  let command = &policy::providers::claude_code::CLAUDE_CODE.node_login.command;

  assert_eq!(
    command
      .replace("{deploy}", "/d")
      .replace("{name}", &"owner's login".replace('\'', r"'\''")),
    r"/d/bin/bmo-agency node claude-token 'owner'\''s login'"
  );
}
