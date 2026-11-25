//! A gate's places (docs/agent-control.md, 「A gate's places」): one set of places per program its ask
//! composes, gates whose asks compose one program sharing them; a place takes one turn at a time and a
//! program stands in up to its bound, every program's places together up to theirs, the one unused longest
//! making room, whatever its age; and the records a server leaves are taken over by the next.

use agency_server::places::gate_places::{
  Asked, Board, Bounds, Record, place_name, recorded, recorded_place,
};

const BOUNDS: Bounds = Bounds {
  places: 3,
  per_program: 2,
};

fn asked<'a>(system: &'a str, model: &'a str) -> Asked<'a> {
  Asked {
    agent: "gate",
    provider: "deepseek",
    model,
    effort: "high",
    thinking: false,
    system,
    cwd: "/tmp/gate-sessions",
  }
}

#[test]
fn a_program_is_keyed_by_everything_it_is_composed_of_and_kinds_with_one_program_share_it() {
  let rules = asked("Pick the room's rules.", "deepseek-flash").key();

  assert_eq!(rules, asked("Pick the room's rules.", "deepseek-flash").key());
  assert_ne!(rules, asked("Pick the library's documents.", "deepseek-flash").key());
  assert_ne!(rules, asked("Pick the room's rules.", "deepseek-v4-pro").key());
  assert_eq!(rules.len(), 10);
  assert_eq!(place_name("gate", &rules, 2), format!("gate-{rules}-2"));
}

#[test]
fn each_program_stays_in_its_own_place_and_takes_it_again_once_let_go() {
  let mut board = Board::default();
  let rules = board.take("gate", "rules", 0, BOUNDS).unwrap();
  let library = board.take("gate", "library", 0, BOUNDS).unwrap();

  assert_eq!((rules.name.as_str(), rules.fresh), ("gate-rules-1", true));
  assert_eq!((library.name.as_str(), library.fresh), ("gate-library-1", true));

  board.let_go("gate-rules-1", 10);

  let again = board.take("gate", "rules", 20, BOUNDS).unwrap();

  assert_eq!((again.name.as_str(), again.fresh, again.ended), ("gate-rules-1", false, None));
}

#[test]
fn turns_run_at_once_up_to_the_program_s_bound_and_every_program_s_together() {
  let mut board = Board::default();

  // Two overlapping turns of one program stand in two places; a third waits.
  assert_eq!(board.take("gate", "translate", 0, BOUNDS).unwrap().name, "gate-translate-1");
  assert_eq!(board.take("gate", "translate", 0, BOUNDS).unwrap().name, "gate-translate-2");
  assert!(board.take("gate", "translate", 0, BOUNDS).is_none());
  assert_eq!(board.busy(), 2);

  // Another program takes the last place; with every place at work, the next program waits.
  assert!(board.take("gate", "rules", 0, BOUNDS).is_some());
  assert!(board.take("gate", "library", 0, BOUNDS).is_none());

  // Once places are let go, the one of another program unused longest makes room.
  board.let_go("gate-translate-2", 5);
  board.let_go("gate-rules-1", 7);

  let library = board.take("gate", "library", 9, BOUNDS).unwrap();

  assert_eq!(library.ended.as_deref(), Some("gate-translate-2"));
  assert_eq!(board.standing(), ["gate-library-1", "gate-rules-1", "gate-translate-1"]);

  // A freed slot is the lowest one the program does not hold.
  board.let_go("gate-translate-1", 10);
  assert!(
    board
      .take("gate", "rules", 11, BOUNDS)
      .is_some_and(|seated| seated.name == "gate-rules-1")
  );
}

#[test]
fn a_place_unused_for_however_long_stands_and_is_taken_again() {
  let mut board = Board::default();

  board.take("gate", "rules", 0, BOUNDS).unwrap();
  board.let_go("gate-rules-1", 100);

  // A day unused ends nothing: the next turn of its program takes the same place, standing as it was.
  let again = board.take("gate", "rules", 100 + 24 * 3_600_000, BOUNDS).unwrap();

  assert_eq!((again.name.as_str(), again.fresh, again.ended), ("gate-rules-1", false, None));
}

#[test]
fn the_places_a_server_left_are_read_back_and_taken_over() {
  let data = tempfile::tempdir().unwrap();
  let place = data.path().join("agent-places").join("gate-rules-1");
  let record = Record {
    agent: "gate".into(),
    program: "rules".into(),
    slot: 1,
    used_at: 50,
  };

  std::fs::create_dir_all(&place).unwrap();
  std::fs::write(place.join("gate-place.json"), serde_json::to_vec(&record).unwrap()).unwrap();
  // A place under another name than its record's is none.
  std::fs::create_dir_all(data.path().join("agent-places").join("gate")).unwrap();
  std::fs::write(
    data.path().join("agent-places").join("gate").join("gate-place.json"),
    serde_json::to_vec(&record).unwrap(),
  )
  .unwrap();

  let read = recorded(data.path());

  assert_eq!(read.keys().collect::<Vec<_>>(), ["gate-rules-1"]);
  assert!(recorded_place(data.path(), "gate-rules-1"));
  assert!(!recorded_place(data.path(), "gate"));

  let mut board = Board::default();

  board.adopt("gate-rules-1", &read["gate-rules-1"]);

  let again = board.take("gate", "rules", 60, BOUNDS).unwrap();

  assert_eq!((again.name.as_str(), again.fresh), ("gate-rules-1", false));
  board.let_go("gate-rules-1", 60);
  assert_eq!(board.standing(), ["gate-rules-1"]);
}
