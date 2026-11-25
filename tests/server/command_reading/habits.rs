//! The shell habits that stall a job, each beside the way the refusal asks for it to be written, which must
//! pass, and the everyday commands that only look like one.

use agency_server::command_reading::ShellCall;
use agency_server::command_reading::habits::*;

fn plain() -> ShellCall {
  ShellCall::default()
}

fn habit(command: &str) -> Option<String> {
  habit_refusal(command, plain())
}

// The shell habits that stall a job, each beside the way the refusal asks for it to be written, which must
#[test]
fn a_reader_named_nothing_to_read_is_refused() {
  for command in [
    "ls",
    "cd /tmp && ls -la",
    "eza --icons=always",
    "ls -L 2",
    "cat",
    "cat -",
    "jq '.a'",
    "jq -r '.a[]'",
    "grep -n foo",
    "sed -n 1p",
    "python3",
    "python3 -",
    "head -n 5",
    "tr a b",
    "(cat)",
  ] {
    assert!(habit(command).is_some_and(|refused| refused.contains("standard input")), "{command}");
  }

  for command in [
    "ls -la /tmp",
    "ls .",
    "ls </dev/null",
    "ls -L 2 docs",
    "cat README.md",
    "echo a | cat",
    "git log | head -5",
    "jq '.a' file.json",
    "jq -n '1+1'",
    "jq -f prog.jq data.json",
    "grep -rn foo",
    "grep -e foo docs/a.md",
    "grep -n foo docs/a.md",
    "sed -n 1p a.md",
    "sed -e s/a/b/ a.md",
    "python3 script.py",
    "python3 -c 'print(1)'",
    "python3 -m http.server",
    "python3 - <<'EOF'\nprint(1)\nEOF",
    "cat <<'EOF' > a.txt\nhello\nEOF",
    "jq '.a' <<< '{}'",
    "while read -r line; do grep x; done < list.txt",
    "diff <(sort a) b",
    "cat <(ls /tmp)",
    "rg foo",
    "wc -l a b",
    "a || cat file",
  ] {
    assert_eq!(habit(command), None, "{command}");
  }
}

// A known long command is refused where the call names a wait shorter than it runs for; naming none waits until
// it ends, and so does a wait that covers it or the background.
#[test]
fn a_known_long_command_with_a_short_wait_asked_for() {
  let asked = |timeout_ms: Option<u64>, background: bool| ShellCall {
    timeout_ms,
    background,
  };

  for command in [
    "~/.local/share/bmo-testbed/deploy/bin/bmo check a.ts",
    "bmo check --since main",
    "bmo job close --then 'x'",
    "bmo-job job close --then 'x'",
    "bmo job ask --question 'x'",
    "bmo test suite --since main",
    "bmo test full",
    "bmo cargo test -p x",
    "bun test",
    "bun test --parallel",
    "bmo testbed run -- cargo test",
    "bmo-testbed run -- bun test a.test.ts b.test.ts",
    "bmo-testbed cargo test -p x",
    "git push",
    "git -C /tmp/x push origin main",
    "cd client && bunx nuxi typecheck",
    "cd client && bun run build",
    "bunx vue-tsc --noEmit",
  ] {
    assert!(
      habit_refusal(command, asked(Some(120_000), false))
        .is_some_and(|refused| refused.contains("runs for minutes")),
      "{command}"
    );
    assert_eq!(habit_refusal(command, asked(None, false)), None, "{command}");
    assert_eq!(habit_refusal(command, asked(Some(LONG_COMMAND_WAIT_MS), false)), None, "{command}");
    assert_eq!(habit_refusal(command, asked(Some(1_000), true)), None, "{command}");
  }

  for command in [
    "bun test --parallel tests/packages/a.test.ts",
    "git status",
    "git log --oneline -5",
    "bunx tsc --noEmit",
    "bmo testbed status",
    "bmo-testbed status --json",
    "bmo-testbed classify --json 'cargo test'",
    "bmo job --help",
  ] {
    assert_eq!(habit_refusal(command, asked(Some(1_000), false)), None, "{command}");
  }
}

#[test]
fn a_full_match_that_finds_its_own_shell() {
  for command in [
    "pkill -f voice-cer.py",
    "pkill -9 -f bench",
    "pgrep -f vite && echo up",
    "kill $(pgrep -f vite)",
    "until ! pgrep -f 'voice-cer.py' > /dev/null; do sleep 10; done",
  ] {
    assert!(habit(command).is_some_and(|refused| refused.contains("-f")), "{command}");
  }

  for command in [
    "pgrep -af vite",
    "pgrep -fl vite",
    "pkill vite",
    "pgrep vite",
    "systemctl --user stop bmo-web",
  ] {
    assert_eq!(habit(command), None, "{command}");
  }
}

#[test]
fn a_pattern_the_command_expands_for_itself_left_unquoted() {
  for command in [
    "grep -rn foo --include=*.ts .",
    "find . -name *.ts",
    "rg -g *.vue foo client",
    "find docs -path */lore/* -prune",
    "grep -r x --exclude-dir=node_* .",
  ] {
    assert!(habit(command).is_some_and(|refused| refused.contains("Quote it")), "{command}");
  }

  for command in [
    "grep -rn foo --include='*.ts' .",
    "find . -name '*.ts'",
    r#"rg -g "*.vue" foo client"#,
    "find . -name a.ts",
    "bun test --timeout=5000 a.test.ts",
    "echo --file=${S:?}",
  ] {
    assert_eq!(habit(command), None, "{command}");
  }

  assert!(unquoted_glob("*.ts"));
  assert!(!unquoted_glob("'*.ts'"));
  assert!(unquoted_glob(r#""a"*"#));
  assert!(!unquoted_glob("${S:?}"));
  assert!(!unquoted_glob("\\*.ts"));
}

#[test]
fn a_sleeping_wait_with_no_bound() {
  for command in [
    "until [ -f done ]; do sleep 5; done",
    "while true; do curl -s localhost:3000 && break; sleep 2; done",
    "bash -c 'while ! test -f x; do sleep 1; done'",
  ] {
    assert!(habit(command).is_some_and(|refused| refused.contains("bounded")), "{command}");
  }

  for command in [
    "timeout 300 zsh -c 'until [ -f done ]; do sleep 5; done'",
    "for i in {1..30}; do test -f done && break; sleep 2; done",
    "while read -r f; do echo $f; done < list.txt",
    "sleep 2 && ls /tmp",
    "wait $pid",
  ] {
    assert_eq!(habit(command), None, "{command}");
  }
}

#[test]
fn a_heredoc_body_is_text_not_commands() {
  assert_eq!(habit("cat > run.sh <<'EOF'\nls\npkill -f x\nEOF"), None);
}
