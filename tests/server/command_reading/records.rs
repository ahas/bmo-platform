use agency_server::command_reading::records::*;

#[test]
fn names_the_piece_of_the_command_it_was_read_in() {
  let records = CliRecords::of(&[], "/home/tester");

  assert_eq!(
    command_refusal("~/.local/bin/claude -p x", &records).as_deref(),
    Some("it starts the Claude Code CLI, read in \"~/.local/bin/claude\" with \"-p\"")
  );
  assert!(
    command_refusal("bash -c 'claude --resume x'", &records)
      .unwrap()
      .contains("inside the text \"claude --resume x\" given to bash")
  );
}
