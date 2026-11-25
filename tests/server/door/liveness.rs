//! The life of a turn put into a program's pane (docs/agent-control.md, 「Telling that a turn's program is
//! gone」): its program named by its process — a pane's own by the session its command line carries, a
//! standing one by its place — and each look's judgement of it: a program gone is lost, a record that closed
//! the turn is the record's to read, a pane herdr says is gone is herdr's ending, and with no program to name
//! a pane at rest, blocked or holding none is the other end gone once a second look in a row finds nothing
//! the program writes moved. The turn's row is opened, written, closed with its ending, and a row a process
//! left open is closed as `restarted` as the next server starts.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use agency_server::door::liveness::{
  Judge, Liveness, Lost, Peer, Phase, Program, RESTARTED, Signals, carries_session, close_left_open,
  left_open, own_peer, program_of_session, standing_peer,
};
use agency_server::door::looks::Look;
use agency_server::lines::place::Place;
use agent_host::identity::ProcessIdentity;
use serde_json::json;

use crate::support::scratch::Scratch;

// A process whose command line carries a session the way a program of a run's own does, handed back once
// it has written its first line. A spawn returns while the kernel may still be laying out the new program,
// so its command line can read empty until then; a line it writes is past that. It waits on its input
// rather than on a command of its own, so ending it leaves nothing running.
fn carrying(session: &str) -> Child {
  let mut child = Command::new("sh")
    .args(["-c", "echo; read line", "claude", "--session-id", session])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()
    .unwrap();
  let mut ready = String::new();

  BufReader::new(child.stdout.as_mut().unwrap()).read_line(&mut ready).unwrap();
  assert_eq!(ready, "\n", "the process carrying the session wrote its first line");

  child
}

fn program_of(child: &Child) -> Program {
  let identity = ProcessIdentity::of(child.id() as i32).unwrap();

  Program {
    pid: identity.pid,
    birth: Some(identity.birth),
  }
}

fn ended(mut child: Child) {
  child.kill().unwrap();
  child.wait().unwrap();
}

fn status(word: &str) -> Look {
  Look::Status(word.to_owned())
}

fn signals(size: u64) -> Signals {
  Signals {
    record_size: size,
    status_line_at: None,
    hook_at: None,
  }
}

fn write_running(place: &Path, program: &Program) {
  std::fs::create_dir_all(place).unwrap();
  std::fs::write(
    place.join("running.json"),
    json!({ "identity": "a", "pid": program.pid, "birth": program.birth,
            "loop": { "pid": program.pid, "birth": program.birth },
            "starts": 1, "startedAt": 1, "paneId": "pane-1", "recent": [] })
    .to_string(),
  )
  .unwrap();
}

#[test]
fn a_session_is_carried_right_after_the_flag_that_opens_or_resumes_it() {
  let words = |line: &[&str]| line.iter().map(|word| (*word).to_owned()).collect::<Vec<_>>();

  assert!(carries_session(&words(&["claude", "--session-id", "s-1"]), "s-1"));
  assert!(carries_session(&words(&["claude", "--resume", "s-1", "--model", "m"]), "s-1"));
  assert!(!carries_session(&words(&["claude", "--model", "s-1"]), "s-1"));
  assert!(!carries_session(&words(&["claude", "--session-id", "s-2"]), "s-1"));
}

#[test]
fn a_pane_s_own_program_is_found_by_its_session_and_read_gone_once_its_process_ends() {
  let session = format!("liveness-{}-{}", std::process::id(), agency_server::now_ms());

  assert_eq!(program_of_session(&session), None);
  assert_eq!(own_peer(None), Peer::Unnamed);

  let child = carrying(&session);
  let found = program_of_session(&session).expect("the process carrying the session");

  assert_eq!(found, program_of(&child));
  assert_eq!(own_peer(Some(&found)), Peer::Alive(found.clone()));

  ended(child);

  assert_eq!(own_peer(Some(&found)), Peer::Gone(found.clone()));
}

#[test]
fn a_standing_program_is_gone_once_its_process_ends_or_its_place_stands_another() {
  let folder = tempfile::tempdir().unwrap();
  let place = Place::of(folder.path(), "agent-a");
  let first = Command::new("sleep").arg("30").spawn().unwrap();
  let named = program_of(&first);

  write_running(&place.path, &named);

  assert_eq!(standing_peer(&place, None), Peer::Unnamed);
  assert_eq!(standing_peer(&place, Some(&named)), Peer::Alive(named.clone()));

  // Its loop stood another program in its stead: the one the turn was put into is gone.
  let second = Command::new("sleep").arg("30").spawn().unwrap();

  write_running(&place.path, &program_of(&second));

  assert_eq!(standing_peer(&place, Some(&named)), Peer::Gone(named.clone()));

  // The place still names it, and its process has ended.
  write_running(&place.path, &named);
  ended(first);

  assert_eq!(standing_peer(&place, Some(&named)), Peer::Gone(named.clone()));

  ended(second);
}

#[test]
fn a_program_gone_is_lost_at_once_whatever_herdr_reads() {
  let gone = Peer::Gone(Program {
    pid: 4_000_000,
    birth: Some("1".to_owned()),
  });

  for herdr in [status("working"), status("done"), Look::Silent("no answer".to_owned())] {
    assert_eq!(Judge::default().look(&gone, false, &herdr, &signals(10)), Some(Lost::Program));
  }

  assert_eq!(Lost::Program.word(), "program-lost");
}

#[test]
fn a_record_that_closed_the_turn_is_the_record_s_to_read_even_with_its_program_gone() {
  let gone = Peer::Gone(Program {
    pid: 4_000_000,
    birth: None,
  });
  let mut judge = Judge::default();

  assert_eq!(judge.look(&gone, true, &status("done"), &signals(10)), None);
  assert_eq!(judge.look(&Peer::Unnamed, true, &status("unknown"), &signals(10)), None);
  assert_eq!(judge.look(&Peer::Unnamed, true, &status("unknown"), &signals(10)), None);
}

#[test]
fn a_pane_herdr_says_is_gone_is_herdr_s_ending_and_never_a_silent_peer() {
  let mut judge = Judge::default();

  assert_eq!(judge.look(&Peer::Unnamed, false, &Look::Gone, &signals(10)), None);
  assert_eq!(judge.look(&Peer::Unnamed, false, &Look::Gone, &signals(10)), None);
}

#[test]
fn a_pane_at_rest_with_no_turn_end_and_no_program_is_a_silent_peer_once_nothing_moved_between_two_looks() {
  let mut judge = Judge::default();

  // The first look only marks what the program's signals were.
  assert_eq!(judge.look(&Peer::Unnamed, false, &status("idle"), &signals(10)), None);
  // Its record grew: the program spoke, and the mark starts again.
  assert_eq!(judge.look(&Peer::Unnamed, false, &status("idle"), &signals(12)), None);
  assert_eq!(judge.look(&Peer::Unnamed, false, &status("done"), &signals(12)), Some(Lost::SilentPeer));
  assert_eq!(Lost::SilentPeer.word(), "silent-peer");
}

#[test]
fn a_pane_blocked_or_holding_no_program_with_none_to_name_is_a_silent_peer_once_nothing_moved() {
  for word in ["blocked", "unknown"] {
    let mut judge = Judge::default();
    let quiet = Signals {
      record_size: 10,
      status_line_at: Some(5),
      hook_at: Some(6),
    };

    assert_eq!(judge.look(&Peer::Unnamed, false, &status(word), &quiet), None);

    // A hook event is the program speaking.
    let spoke = Signals {
      hook_at: Some(7),
      ..quiet.clone()
    };

    assert_eq!(judge.look(&Peer::Unnamed, false, &status(word), &spoke), None);
    assert_eq!(judge.look(&Peer::Unnamed, false, &status(word), &spoke), Some(Lost::SilentPeer));
  }
}

#[test]
fn a_program_alive_or_a_pane_working_is_waited_on_however_long() {
  let alive = Peer::Alive(Program {
    pid: 1,
    birth: None,
  });
  let mut judge = Judge::default();

  for _ in 0..5 {
    assert_eq!(judge.look(&alive, false, &status("done"), &signals(10)), None);
    assert_eq!(judge.look(&alive, false, &status("unknown"), &signals(10)), None);
  }

  // A pane working breaks the run of quiet looks, as herdr not answering does.
  assert_eq!(judge.look(&Peer::Unnamed, false, &status("unknown"), &signals(10)), None);
  assert_eq!(judge.look(&Peer::Unnamed, false, &status("working"), &signals(10)), None);
  assert_eq!(judge.look(&Peer::Unnamed, false, &status("unknown"), &signals(10)), None);
  assert_eq!(
    judge.look(&Peer::Unnamed, false, &Look::Silent("no answer".to_owned()), &signals(10)),
    None
  );
  assert_eq!(judge.look(&Peer::Unnamed, false, &status("unknown"), &signals(10)), None);
}

#[tokio::test]
async fn a_turn_s_row_is_written_as_it_changes_closed_with_its_ending_and_one_left_open_closed_as_restarted()
 {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let child = Command::new("sleep").arg("30").spawn().unwrap();
  let program = program_of(&child);
  let mut turn = Liveness::new("exec-1", "search-1", "search", "s-1", vec!["/records/s-1.jsonl".into()]);

  turn.write(db).await;
  turn.pane("pane-1");
  turn.phase(Phase::Working);
  turn.program(Some(&program));
  turn.herdr(&status("working"));
  turn.signals(&signals(40));
  turn.write(db).await;

  let open = left_open(db).await.unwrap();

  assert_eq!(open.len(), 1);
  assert_eq!(
    (
      open[0].phase.as_str(),
      open[0].pane.as_deref(),
      open[0].program.as_ref(),
      open[0].herdr.as_deref()
    ),
    ("working", Some("pane-1"), Some(&program), Some("working"))
  );

  turn.close(db, "program-lost", "program-lost: the program is gone").await;

  assert!(left_open(db).await.unwrap().is_empty());
  assert_eq!(
    scratch
      .count(
        r#"SELECT count(*) FROM "turn_liveness" WHERE "execution" = 'exec-1' AND "ending" = 'program-lost'"#,
        vec![]
      )
      .await,
    1
  );

  // A turn the process before this one left open is closed with what is found of its program now.
  let mut left = Liveness::new("exec-2", "sources-2", "search", "s-2", vec![]);

  left.program(Some(&program));
  left.write(db).await;
  ended(child);

  assert_eq!(close_left_open(db).await.unwrap(), 1);
  assert!(left_open(db).await.unwrap().is_empty());
  assert_eq!(
    scratch
      .count(
        r#"SELECT count(*) FROM "turn_liveness" WHERE "execution" = 'exec-2' AND "ending" = $1
             AND "detail" LIKE '%gone%'"#,
        vec![RESTARTED.into()]
      )
      .await,
    1
  );
}
