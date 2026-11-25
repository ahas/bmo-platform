//! A Claude account signed in from the console (docs/cache-fallback.md, 「Signing an account in from the
//! console」): the sign-in's address read out of what the CLI prints, its hyperlink escapes and all; a refusal
//! said in the CLI's last words with the code taken out; and a new account's directory named by its name. A
//! person's sign-in on their node, as far as it is read without a node: the pane it runs in, the address read off
//! a screen that wrapped it, and the CLI's words after the code judging whether the login took. A person's new
//! login named past every account's names, and a name another account holds refused under its field; the owner's
//! new login named the same way, past a directory that already holds files.

use std::os::unix::process::CommandExt;
use std::path::Path;

use agency_server::control::claude_logins::{
  NodeOutcome, last_words, name_refusal, new_home, new_login_name, next_login_name, node_launch,
  node_outcome, said_since, says_missing, sign_in_address,
};
use policy::control_words::CONTROL_WORDS;

// What `claude auth login` prints with no browser to open: the address as a terminal hyperlink, its escapes
// around it, then the prompt for the code.
fn printed(address: &str) -> String {
  format!(
    "Opening browser to sign in…\nIf the browser didn't open, visit: \u{1b}]8;;{address}\u{7}{address}\u{1b}]8;;\u{7}\nPaste code here if prompted > "
  )
}

#[test]
fn the_address_is_read_out_of_the_hyperlink_the_cli_prints() {
  let address = "https://example.com/oauth/authorize?code=true&client_id=x&state=y";
  assert_eq!(sign_in_address(&printed(address)).as_deref(), Some(address));
  assert_eq!(sign_in_address("Opening browser to sign in…\n"), None);
  assert_eq!(sign_in_address("visit: http://example.com/plain"), None);
}

#[test]
fn a_refusal_says_the_last_words_with_the_code_taken_out() {
  let said = format!(
    "{}abc123-secret\n\u{1b}[31mOAuth error: invalid code abc123-secret\u{1b}[0m\n\n",
    printed("https://example.com/a")
  );
  let words = last_words(&said, Some("abc123-secret"));
  assert_eq!(words, "OAuth error: invalid code …");
  assert_eq!(last_words("", None), CONTROL_WORDS.logins.silent);
}

#[test]
fn a_new_account_s_directory_is_named_by_its_name_under_the_env_directory() {
  let root = Path::new("/data");
  assert_eq!(new_home(root, "claude-03"), Path::new("/data/env/claude-03"));
  assert_eq!(new_home(root, " Work Login! "), Path::new("/data/env/work-login"));
  assert_eq!(new_home(root, "../.."), Path::new("/data/env/"));
}

#[test]
fn a_person_s_sign_in_pane_runs_the_cli_s_own_words_under_the_node_s_workplace() {
  let launch = node_launch(
    "/home/someone/bmo",
    "/home/someone",
    "/home/someone/bmo/data/env/theirs",
    "C.UTF-8",
    false,
  );
  let policy = &policy::providers::claude_code::CLAUDE_CODE.console_login.command;

  // The CLI's words are the shell's arguments after its script, never spliced into it, the workplace's own Claude
  // Code in place of the word `claude`, its updater off.
  assert_eq!(&launch.command[..2], ["/bin/sh", "-c"]);
  assert_eq!(launch.command[4], "/home/someone/bmo/bin/bmo-claude");
  assert_eq!(&launch.command[5..], &policy[1..]);
  assert!(launch.command[2].contains(r#""$@""#));
  assert_eq!(launch.env["DISABLE_AUTOUPDATER"], "1");
  assert!(
    launch.command[2].contains("unset CLAUDE_CODE_OAUTH_TOKEN ANTHROPIC_API_KEY ANTHROPIC_AUTH_TOKEN")
  );
  assert_eq!(launch.env["CLAUDE_CONFIG_DIR"], "/home/someone/bmo/data/env/theirs");
  assert_eq!(launch.env["BROWSER"], "true");
  assert_eq!(launch.env["HOME"], "/home/someone");
  assert!(launch.env["PATH"].starts_with("/home/someone/bmo/bun/bin:"));
  assert_eq!(launch.cwd, "/home/someone");
}

// "a sign-in pane on a workplace with no Claude Code of its own says so and starts nothing"
#[test]
fn a_sign_in_pane_on_a_workplace_with_no_claude_code_of_its_own_says_so_and_starts_nothing() {
  let node = tempfile::tempdir().unwrap();
  let root = node.path().join("bmo");
  let config = root.join("data/env/theirs");
  let launch = node_launch(
    &root.to_string_lossy(),
    &node.path().to_string_lossy(),
    &config.to_string_lossy(),
    "C.UTF-8",
    true,
  );
  let line = run_pane(&launch, node.path(), "-HUP");

  assert!(says_missing(&line), "{line:?}");
  assert!(!says_missing("Paste code here if prompted > "));
}

// The shell of a launch, run as herdr runs a pane's, its first line read within a bound; then `close` sent to its
// whole group as herdr's closing the pane sends it, and the group taken down.
fn run_pane(launch: &agency_server::control::claude_logins::NodeLaunch, cwd: &Path, close: &str) -> String {
  let mut shell = std::process::Command::new(&launch.command[0]);

  shell
    .args(&launch.command[1..])
    .env_clear()
    .envs(&launch.env)
    .current_dir(cwd)
    .process_group(0)
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null());
  // The shell takes the hangup at its default, as a pane's shell under the node's herdr service does, whatever
  // the test run was handed: under `nohup` it would inherit the hangup ignored, which a non-interactive shell can
  // neither trap nor be ended by.
  // SAFETY: signal in the child before exec, an async-signal-safe call.
  unsafe {
    shell.pre_exec(|| {
      libc::signal(libc::SIGHUP, libc::SIG_DFL);
      Ok(())
    });
  }

  let mut shell = shell.spawn().unwrap();
  let mut out = std::io::BufReader::new(shell.stdout.take().unwrap());
  let (said, heard) = std::sync::mpsc::channel();

  std::thread::spawn(move || {
    let mut line = String::new();
    let _ = std::io::BufRead::read_line(&mut out, &mut line);
    let _ = said.send(line);
  });

  let line = heard.recv_timeout(std::time::Duration::from_secs(10)).unwrap_or_default();
  let group = format!("-{}", shell.id());
  let _ = std::process::Command::new("kill").args([close, "--", &group]).status();
  let ended = (0..100).any(|_| {
    std::thread::sleep(std::time::Duration::from_millis(50));
    matches!(shell.try_wait(), Ok(Some(_)))
  });
  let _ = std::process::Command::new("kill").args(["-KILL", "--", &group]).status();
  let _ = shell.wait();

  assert!(ended, "the pane's shell ends as its pane closes");
  line
}

// "a new login's pane closed with no login written leaves no directory on the node, and one written stays"
#[test]
fn a_new_login_s_pane_closed_with_no_login_written_leaves_no_directory_and_one_written_stays() {
  let node = tempfile::tempdir().unwrap();
  let root = node.path().join("bmo");
  let config = root.join("data/env/fresh");
  let launch = |fresh| {
    node_launch(
      &root.to_string_lossy(),
      &node.path().to_string_lossy(),
      &config.to_string_lossy(),
      "C.UTF-8",
      fresh,
    )
  };

  run_pane(&launch(true), node.path(), "-HUP");
  assert!(!config.exists(), "a new login given up leaves nothing");

  // A login the CLI wrote stays as the pane closes.
  std::fs::create_dir_all(&config).unwrap();
  std::fs::write(config.join(".credentials.json"), "{}").unwrap();
  run_pane(&launch(true), node.path(), "-HUP");
  assert!(config.join(".credentials.json").is_file());

  // An account signed in again keeps its directory whatever the sign-in came to.
  std::fs::remove_file(config.join(".credentials.json")).unwrap();
  run_pane(&launch(false), node.path(), "-HUP");
  assert!(config.is_dir());
}

#[test]
fn the_login_is_judged_by_the_cli_s_words_after_the_code() {
  let before = "Paste code here if prompted > ";

  assert_eq!(
    said_since(before, &format!("{before}abc\nLogin successful.\n")),
    "abc\nLogin successful.\n"
  );
  assert_eq!(
    node_outcome(&said_since(before, &format!("{before}abc\nlogin successful."))),
    NodeOutcome::Took
  );
  assert_eq!(node_outcome("abc\nOAuth error: invalid code"), NodeOutcome::Refused);
  assert_eq!(node_outcome("abc\n"), NodeOutcome::Pending);
}

#[test]
fn a_new_login_is_named_past_every_account_s_numbered_name() {
  assert_eq!(next_login_name(&[]), "claude-01");
  // The owner's own `claude-01` and `claude-02` stand beside a person who holds none of their own.
  assert_eq!(next_login_name(&["claude-01", "claude-02"]), "claude-03");
  assert_eq!(
    next_login_name(&["main", "claude-9", "claude-x", "claude-", " claude-11 "]),
    "claude-12"
  );
}

#[test]
fn a_name_another_account_holds_is_refused_under_the_name_field() {
  let taken = name_refusal("claude-01", &["claude-01"]).expect("a name held is refused");
  assert_eq!(
    (taken.status.as_u16(), taken.code(), taken.field.as_deref()),
    (409, "name-taken".to_owned(), Some("name"))
  );

  let long = name_refusal(&"x".repeat(500), &["claude-01"]).expect("a name too long is refused");
  assert_eq!((long.status.as_u16(), long.field.as_deref()), (400, Some("name")));

  assert!(name_refusal("claude-02", &["claude-01"]).is_none());
}

#[test]
fn a_new_login_named_by_agency_passes_every_name_and_every_directory_already_holding_files() {
  let data = tempfile::tempdir().unwrap();
  let names = ["claude-01", "claude-03"];

  // A person's login, whose directory stands on their node, is the next number past every account's names.
  assert_eq!(new_login_name(None, &names, None).unwrap(), "claude-04");

  // The owner's stands here: a directory left holding files passes its number on to the next.
  let left = new_home(data.path(), "claude-04");

  std::fs::create_dir_all(&left).unwrap();
  std::fs::write(left.join(".claude.json"), "{}").unwrap();
  std::fs::create_dir_all(new_home(data.path(), "claude-05")).unwrap();
  assert_eq!(new_login_name(None, &names, Some(data.path())).unwrap(), "claude-05");

  // A name given is taken as given where no account holds it, and refused where one does.
  assert_eq!(new_login_name(Some("mine"), &names, Some(data.path())).unwrap(), "mine");
  assert_eq!(new_login_name(Some("claude-03"), &names, None).unwrap_err().code(), "name-taken");
}
