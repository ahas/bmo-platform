//! The removal shapes a job's pane stops on, and the ones written the way the refusal asks for, which must
//! pass.

use agency_server::command_reading::removal::*;

// The removal shapes a job's pane stops on, and the ones written the way the refusal asks for, which must
#[test]
fn removal_refusal_reads_the_targets_the_cli_stops_on() {
  for command in [
    "rm -f $S/*.md",
    "S=/tmp/x; rm -rf $S/",
    r#"rm -f "$S"/a.md"#,
    "rm -f ${S}/a.md",
    "rm -f ${S:-/tmp}/a.md",
    "rm -f $P/$(cat f).jsonl",
    r#"S=work; echo "$(cat $S/a.md)"; rm -f $S/*.md"#,
    "cd /tmp && rm -rf `pwd`/x",
    "sudo rm -rf $HOME/x",
    "timeout 5 rm -f $1/x",
    "x=$(ls); rm $x",
    "bash -c 'rm -rf $D/x'",
    r#"zsh -c "rm -rf \$D/x""#,
    "eval 'rm -rf $D/x'",
    "echo $(rm -rf $D/x)",
    "if true; then rm -r $D; fi",
    "cat > run.sh <<'EOF'\nrm -f $S/x\nEOF",
    "rm -f -- $S",
  ] {
    assert!(
      removal_refusal(command).is_some_and(|refused| refused.contains("refused:")),
      "{command}"
    );
  }

  for command in [
    r#"rm -f "${S:?}"/*.md"#,
    "rm -f ${S:?}/x",
    r#"rm -rf "${S?scratchpad unset}/build""#,
    r#"S=work; echo "$(cat "$S"/a.md)"; rm -f "${S:?}"/*.md"#,
    "rm -f /tmp/a.md",
    "rm -rf node_modules dist",
    "echo '$S'; rm x",
    "rm -f 'a$b'",
    "git rm $f",
    "find . -name '*.tmp' -delete",
    "ls $S | xargs rm",
    "grep -rn 'rm -rf $S' docs",
    "echo rmdir $S",
    "bun test --parallel repo/repo/removal-guard.test.ts",
    "# rm -rf $S/\nls /tmp",
    "rm -f a.$$.tmp",
  ] {
    assert_eq!(removal_refusal(command), None, "{command}");
  }

  let reason = removal_refusal("rm -f $S/*.md").unwrap();

  assert!(reason.contains("$S/*.md"));
  assert!(reason.contains(r#""${S:?}"/*.md"#));
  assert!(reason.contains("~/.claude"));
}

#[test]
fn reads_a_substitution_and_an_unguarded_expansion_apart() {
  assert_eq!(unsafe_target("$(pwd)/x"), Some(Unsafe::Substitution));
  assert_eq!(unsafe_target(r#""$D"/x"#), Some(Unsafe::Expansion));
  assert_eq!(unsafe_target("'$D'/x"), None);
  assert_eq!(unsafe_target(r#""${D:?}"/x"#), None);
  assert_eq!(unsafe_target("${D:?}/${E}"), Some(Unsafe::Expansion));
}
