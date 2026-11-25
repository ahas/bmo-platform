use agency_server::command_reading::reading::*;

#[test]
fn substitutions_are_read_as_commands_before_the_one_they_stand_in() {
  let read = read_command("echo $(cat x) > \"out file\"");

  assert_eq!(read.commands[0].words, ["cat", "x"]);
  assert_eq!(read.commands[1].words, ["echo", "$(cat x)"]);
  assert_eq!(read.commands[1].redirects, ["out file"]);
  assert!(!read.substituted_program);
  assert!(read_command("(`echo claude`) -p hi").substituted_program);
  assert!(!read_command("git commit -m \"node (`bot/claude`)\"").substituted_program);
  assert_eq!(read_command("git commit -m \"node (`bot/claude`)\"").commands.len(), 1);
}

#[test]
fn assigned_variables_are_put_in_where_they_are_used() {
  assert_eq!(expand_assigned("A=cla; B=ude; $A$B -p hi"), "A=cla; B=ude; claude -p hi");
  assert_eq!(expand_assigned("P=\"~/x\"; ls ${P}/y"), "P=\"~/x\"; ls ~/x/y");
}

#[test]
fn a_runner_and_its_value_are_looked_past() {
  let words: Vec<String> = ["timeout", "5", "bun", "x"].map(str::to_owned).to_vec();

  assert_eq!(program_of(&words).unwrap().program, "bun");
  assert_eq!(program_of(&["timeout".to_owned(), "claude".to_owned()]).unwrap().program, "claude");
  assert_eq!(shortly("a   b\nc"), "\"a b c\"");
}
