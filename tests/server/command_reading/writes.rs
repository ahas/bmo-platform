//! The reading of what a job's command or file tool writes into a place the job guard keeps: another worker's
//! checkout, the statement document, and an umbrella job's own folder, a relative path read from where the shell
//! stands.

use agency_server::command_reading::writes::{
  Checkouts, ForeignCheckout, SealedTree, Standing, foreign_command_write, foreign_file_write,
  sealed_command_write, sealed_file_write, structure_command_write, structure_file_write,
};

const HOME: &str = "/home/tester";

fn checkout(worker: &str, cwd: &str) -> ForeignCheckout {
  ForeignCheckout {
    worker: worker.into(),
    repository: worker.into(),
    cwd: cwd.into(),
  }
}

// The job works in its own checkout; another worker's stands beside it, and a third holds both in its
// directory.
fn checkouts() -> Checkouts {
  Checkouts {
    own: vec!["/home/tester/repos/agency".into()],
    foreign: vec![checkout("web", "/home/tester/repos/web"), checkout("umbrella", "/home/tester/repos")],
    sealed: vec![],
  }
}

fn nowhere() -> Standing {
  Standing {
    at: None,
    home: HOME.into(),
  }
}

fn foreign(command: &str) -> Option<String> {
  foreign_command_write(command, &checkouts(), &nowhere())
    .map(|(checkout, why)| format!("{}: {why}", checkout.worker))
}

// An umbrella job: its own folder holds the checkout lent to it and another worker's, and tracks two files of its
// own; its shell stands in the folder.
fn umbrella() -> Checkouts {
  Checkouts {
    own: vec!["/home/tester/repos".into(), "/home/tester/repos/agency".into()],
    foreign: vec![checkout("web", "/home/tester/repos/web")],
    sealed: vec![tree()],
  }
}

fn tree() -> SealedTree {
  SealedTree {
    repository: "tree".into(),
    cwd: "/home/tester/repos".into(),
    tracked: vec!["README.md".into(), "docs/owner/plan.md".into()],
  }
}

fn in_folder() -> Standing {
  Standing {
    at: Some("/home/tester/repos".into()),
    home: HOME.into(),
  }
}

#[test]
fn an_umbrella_job_writes_none_of_its_folder_s_tracked_files_and_writes_in_the_checkouts_lent_to_it() {
  let known = umbrella();
  let sealed = |command: &str| sealed_command_write(command, &known, &in_folder()).map(|(_, why)| why);

  for command in [
    "echo x > README.md",
    "sed -i 's/a/b/' docs/owner/plan.md",
    "cd docs && echo x >> owner/plan.md",
    "echo x > /home/tester/repos/README.md",
    "cp notes.md ~/repos/README.md",
    "git commit -am x",
    "git -C /home/tester/repos add .",
    "cd agency && cd .. && git add README.md",
  ] {
    assert!(sealed(command).is_some(), "let through: {command}");
  }

  for command in [
    "echo x > agency/notes.md",
    "cd agency && git commit -am x",
    "git -C agency commit -am x",
    "echo x > new-notes.md",
    "cat README.md",
    "git log --oneline -5",
  ] {
    assert_eq!(sealed(command), None, "{command}");
  }

  // A relative path into a checkout below the folder that is no loan of the job's is that worker's.
  for command in ["echo x > web/notes.md", "cd web && git commit -am x"] {
    let whose =
      foreign_command_write(command, &known, &in_folder()).map(|(checkout, _)| checkout.worker.clone());

    assert_eq!(whose.as_deref(), Some("web"), "{command}");
  }

  assert!(sealed_file_write("/home/tester/repos/docs/owner/plan.md", &known).is_some());
  assert_eq!(sealed_file_write("/home/tester/repos/agency/README.md", &known), None);
  assert_eq!(sealed_file_write("/home/tester/repos/new-notes.md", &known), None);
}

#[test]
fn another_job_writes_none_of_an_umbrella_s_tracked_files_and_anywhere_else_below_its_folder() {
  // A job in a checkout below the folder, and a repository below it that no row names.
  let known = Checkouts {
    own: vec!["/home/tester/repos/agency".into()],
    foreign: vec![checkout("web", "/home/tester/repos/web")],
    sealed: vec![tree()],
  };
  let standing = Standing {
    at: Some("/home/tester/repos/agency".into()),
    home: HOME.into(),
  };
  let sealed = |command: &str| sealed_command_write(command, &known, &standing).map(|(_, why)| why);

  for command in [
    "echo x > ../README.md",
    "cd .. && git commit -am x",
    "cd ~/repos/docs && echo x > owner/plan.md",
  ] {
    assert!(sealed(command).is_some(), "let through: {command}");
  }

  for command in [
    "echo x > notes.md",
    "git commit -am x",
    "echo x > ../agency-scratch/notes.md",
    "cd ../agency-scratch && git commit -am x",
  ] {
    assert_eq!(sealed(command), None, "{command}");
    assert_eq!(foreign_command_write(command, &known, &standing), None, "{command}");
  }

  assert!(sealed_file_write("/home/tester/repos/README.md", &known).is_some());
  assert_eq!(sealed_file_write("/home/tester/repos/agency-scratch/a.md", &known), None);
}

#[test]
fn a_write_into_another_checkout_is_read_however_the_command_writes_it() {
  for command in [
    "echo hi > /home/tester/repos/web/notes.md",
    "cp a.txt ~/repos/web/",
    "sed -i 's/a/b/' $HOME/repos/web/src/main.ts",
    "git -C /home/tester/repos/web commit -m x",
    "cd /home/tester/repos/web && git add . && git commit -m x",
    "cd ~/repos/web && echo x >> notes.md",
    "python3 -c \"open('/home/tester/repos/web/a.py', 'w').write('x')\"",
    "python3 - <<'EOF'\nfrom pathlib import Path\nPath('/home/tester/repos/web/a.py').write_text('x')\nEOF",
    "bash -c 'touch /home/tester/repos/web/new.txt'",
    "D=/home/tester/repos/web; rm -f \"$D/old.txt\"",
  ] {
    assert!(foreign(command).is_some_and(|why| why.starts_with("web: ")), "let through: {command}");
  }
}

#[test]
fn a_read_of_another_checkout_and_a_write_beside_it_are_not_writes_into_it() {
  for command in [
    "cat /home/tester/repos/web/README.md",
    "rg -n route /home/tester/repos/web/src",
    "git -C /home/tester/repos/web log --oneline -5",
    "cp /home/tester/repos/web/a.txt ./a.txt",
    "echo hi > /home/tester/repos/agency/notes.md",
    "python3 -c \"print(open('/home/tester/repos/web/a.py').read())\"",
    "cd /home/tester/repos/web && cd /tmp && echo x > notes.md",
  ] {
    assert_eq!(foreign(command), None, "{command}");
  }
}

#[test]
fn a_file_tool_writes_into_the_deepest_checkout_its_path_lies_inside() {
  let known = checkouts();
  let whose = |path: &str| foreign_file_write(path, &known).map(|checkout| checkout.worker.clone());

  assert_eq!(whose("/home/tester/repos/web/src/a.ts").as_deref(), Some("web"));
  assert_eq!(whose("/home/tester/repos/web").as_deref(), Some("web"));
  assert_eq!(whose("/home/tester/repos/web-w2/src/a.ts").as_deref(), Some("umbrella"));
  assert_eq!(whose("/home/tester/repos/agency/src/a.rs"), None);
  assert_eq!(whose("/home/tester/notes/a.md"), None);
}

#[test]
fn a_command_writing_inside_a_checkout_nested_in_another_s_directory_is_that_checkout_s() {
  assert_eq!(foreign("echo x > /home/tester/repos/agency/a.md"), None);
  assert_eq!(foreign("cd ~/repos/agency && git commit -m x"), None);
  assert!(
    foreign("echo x > /home/tester/repos/STRUCTURE.md").is_some_and(|why| why.starts_with("umbrella: "))
  );
}

#[test]
fn a_change_to_the_statement_document_is_read_in_any_directory_and_a_read_of_it_is_not() {
  for command in [
    "echo x >> STRUCTURE.md",
    "sed -i 's/a/b/' docs/../STRUCTURE.md",
    "git checkout -- STRUCTURE.md",
    "mv /tmp/new.md /home/tester/repos/agency/STRUCTURE.md",
  ] {
    assert!(structure_command_write(command).is_some(), "let through: {command}");
  }

  for command in ["cat STRUCTURE.md", "rg -n Accounts STRUCTURE.md", "git diff STRUCTURE.md"] {
    assert_eq!(structure_command_write(command), None, "{command}");
  }

  assert!(structure_file_write("/home/tester/repos/agency/STRUCTURE.md").is_some());
  assert!(structure_file_write("STRUCTURE.md").is_some());
  assert!(structure_file_write("/home/tester/repos/agency/docs/STRUCTURE.md.bak").is_none());
}
