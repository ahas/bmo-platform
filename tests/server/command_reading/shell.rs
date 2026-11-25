use agency_server::command_reading::shell::*;

fn words(text: &str) -> Vec<Vec<String>> {
  split(text).commands.into_iter().map(|command| command.words).collect()
}

#[test]
fn words_keep_their_quoting_and_operators_end_a_command() {
  assert_eq!(
    words(r#"git commit -m "x" && rm -rf "${S:?}"/*.md; A=1 cmd"#),
    [
      vec!["git", "commit", "-m", "\"x\""],
      vec!["rm", "-rf", "\"${S:?}\"/*.md"],
      vec!["A=1", "cmd"]
    ]
  );
}

#[test]
fn a_descriptor_belongs_to_its_redirect_and_a_duplicate_names_no_file() {
  let read = split("echo hi > out.txt 2>&1 </dev/null >> log");
  let command = &read.commands[0];

  assert_eq!(command.words, ["echo", "hi"]);
  assert_eq!(command.redirects, ["out.txt", "log"]);
  assert_eq!(command.appended, ["log"]);
  assert_eq!(command.inputs, ["/dev/null"]);
  assert!(command.stdin_given);
}

#[test]
fn a_heredoc_body_is_text_beside_the_line_that_opened_it() {
  let read = split("cat > x <<'EOF' | wc -l\nls\npkill -f x\nEOF\necho after");

  assert_eq!(
    read.commands.iter().map(|command| command.words.clone()).collect::<Vec<_>>(),
    [vec!["cat"], vec!["wc", "-l"], vec!["echo", "after"]]
  );
  assert_eq!(
    read.heredocs,
    [Heredoc {
      opener: "cat > x <<'EOF' | wc -l".into(),
      text: "ls\npkill -f x".into()
    }]
  );
  assert!(read.commands[1].stdin_given);
}

#[test]
fn substitutions_come_out_with_whether_they_stood_quoted() {
  let read = split("echo $(echo one) \"a $(two) `three`\" <(ls four)");

  assert_eq!(
    read.inner.iter().map(|inner| (inner.text.as_str(), inner.quoted)).collect::<Vec<_>>(),
    [("echo one", false), ("two", true), ("three", true), ("ls four", false)]
  );
  assert_eq!(read.commands[0].words.last().unwrap(), "<(ls four)");
}

#[test]
fn heredoc_bodies_come_out_and_quoted_text_is_blanked() {
  let text = "bun scripts/edit.ts docs/x.md --stdin <<'EOF'\n@@from\nfor each file\nEOF\necho done";

  assert_eq!(without_heredoc_bodies(text), "bun scripts/edit.ts docs/x.md --stdin <<'EOF'\necho done");
  assert_eq!(bare(text), "bun scripts/edit.ts docs/x.md --stdin << '' \n echo done");
  assert_eq!(bare("x --replace 'waits for the run' \"a $b\"c"), "x --replace '' ''c");
}

#[test]
fn a_line_the_tokenizer_cannot_read_reads_as_nothing() {
  assert_eq!(split("echo 'open"), Split::default());
}

#[test]
fn a_value_takes_the_quoting_off_and_leaves_expansions_as_written() {
  assert_eq!(value(r#""$HOME/.claude"/x"#), "$HOME/.claude/x");
  assert_eq!(value("'a b'c\\ d"), "a bc d");
  assert_eq!(value("$(echo claude)"), "$(echo claude)");
  assert_eq!(unquoted(r#""rm -rf \$D/x""#), "rm -rf $D/x");
}

#[test]
fn the_program_is_found_past_assignments_runners_and_grammar() {
  let run =
    program_of(&["FOO=1".into(), "timeout".into(), "30".into(), "claude".into(), "-p".into()]).unwrap();

  assert_eq!(
    (run.program.as_str(), run.args.as_slice(), run.runners.as_slice()),
    ("claude", &["-p".to_owned()][..], &["timeout".to_owned()][..])
  );
  assert_eq!(program_of(&["if".into(), "!".into()]), None);
}
