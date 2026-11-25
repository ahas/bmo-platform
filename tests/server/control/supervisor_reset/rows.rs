//! The rows of agency's store a supervisor's reset takes: the conversation range's own tables and
//! placeholders numbered as they are written.

use agency_server::control::supervisor_reset::rows::*;

fn scope(alone: Vec<i64>) -> Scope {
  Scope {
    agent: "room-supervisor".to_owned(),
    alone,
  }
}

#[test]
fn the_conversation_range_takes_agency_s_own_tables_and_the_person_range_none() {
  let both = scope(vec![-7]);
  let tables = |ranges: &[Range]| picks(&both, ranges).iter().map(|one| one.table).collect::<Vec<_>>();

  assert_eq!(
    tables(&[Range::Conversation]),
    ["agent_log", "runs", "room_session_turns", "room_sessions"]
  );
  assert!(tables(&[Range::Person]).is_empty());
}

#[test]
fn placeholders_are_numbered_as_written_and_no_room_is_a_list_nothing_is_in() {
  let picks = picks(&scope(vec![-7, 12]), &[Range::Conversation]);
  let of = |table: &str| picks.iter().find(|one| one.table == table).unwrap().condition.text.clone();

  assert_eq!(of("agent_log"), "agent = $1 OR chat_id IN ($2, $3)");
  assert_eq!(of("runs"), "agent = $1 OR chat_id IN ($2, $3)");
  assert_eq!(of("room_sessions"), "agent = $1");

  let none = super::supervisor_reset_picks(&scope(vec![]));

  assert_eq!(none, "agent = $1 OR chat_id IN (NULL)");
}
