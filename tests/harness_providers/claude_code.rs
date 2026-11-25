//! Claude Code: its catalogue read off a login's directory, where its sessions' records are looked for, and its
//! history's prompts forgotten.

use std::path::Path;

use harness_providers::claude_code::*;
use harness_providers::{Machine, Provider};
use serde_json::{Value, json};

// A configuration directory of the test's own, with a catalogue file written under it.
fn write(dir: &Path, name: &str, text: &str) {
  let place = dir.join("cache/model-catalog");

  std::fs::create_dir_all(&place).unwrap();
  std::fs::write(place.join(name), text).unwrap();
}

fn file(fetched_at: i64, models: Value) -> String {
  json!({ "version": 2, "fetchedAt": fetched_at, "staleAt": fetched_at + 3_600_000, "catalog": { "surface": "cc", "config": { "id": "cc", "models": models } } })
    .to_string()
}

fn thinking() -> Value {
  json!({ "type": "effort", "effort_options": [{ "id": "low" }, { "id": "medium", "badge": { "message": "Default" } }, { "id": "high" }, { "id": "xhigh" }, { "id": "max" }] })
}

fn ids(dir: &Path) -> Vec<String> {
  catalogue(dir).unwrap().models.into_iter().map(|model| model.id).collect()
}

/// a model is read with its standing and the efforts its entry names
#[test]
fn a_model_is_read_with_its_standing_and_the_efforts_its_entry_names() {
  let dir = tempfile::tempdir().unwrap();

  write(
    dir.path(),
    "abc-cc.json",
    &file(
      1_000,
      json!([
        { "id": "claude-opus-5-5", "name": "Opus 5.5", "section": "main", "thinking": thinking() },
        { "id": "claude-haiku-4-5-20251001", "section": "main", "thinking": { "type": "none" } },
        { "id": "claude-opus-5", "section": "overflow", "thinking": thinking() },
      ]),
    ),
  );

  let read = catalogue(dir.path()).unwrap();
  let words: Vec<String> = ["low", "medium", "high", "xhigh", "max"].map(String::from).into();

  assert_eq!((read.fetched_at, read.stale_at), (1_000.0, 3_601_000.0));
  assert_eq!(
    read.models,
    [
      Model {
        id: "claude-opus-5-5".into(),
        in_service: true,
        efforts: words.clone()
      },
      // A model that does not think names no effort of its own.
      Model {
        id: "claude-haiku-4-5-20251001".into(),
        in_service: true,
        efforts: vec![]
      },
      // A superseded generation is read like any other; only its standing differs.
      Model {
        id: "claude-opus-5".into(),
        in_service: false,
        efforts: words
      },
    ]
  );
}

/// a model the reading has never heard of is read like any other
#[test]
fn a_model_the_reading_has_never_heard_of_is_read_like_any_other() {
  let dir = tempfile::tempdir().unwrap();

  write(
    dir.path(),
    "abc-cc.json",
    &file(1_000, json!([{ "id": "claude-opus-6", "section": "main", "thinking": thinking() }])),
  );

  assert_eq!(catalogue(dir.path()).unwrap().models[0].efforts.len(), 5);
}

/// the newest fetch answers where the directory holds more than one file
#[test]
fn the_newest_fetch_answers_where_the_directory_holds_more_than_one_file() {
  let dir = tempfile::tempdir().unwrap();

  write(
    dir.path(),
    "old-cc.json",
    &file(1_000, json!([{ "id": "claude-opus-5", "section": "main" }])),
  );
  write(
    dir.path(),
    "new-cc.json",
    &file(2_000, json!([{ "id": "claude-opus-5-5", "section": "main" }])),
  );
  write(dir.path(), "notes-other.json", &file(9_000, json!([{ "id": "elsewhere" }])));

  assert_eq!(ids(dir.path()), ["claude-opus-5-5"]);
}

/// an unreadable neighbour is passed over while a good file stands
#[test]
fn an_unreadable_neighbour_is_passed_over_while_a_good_file_stands() {
  let dir = tempfile::tempdir().unwrap();

  write(dir.path(), "broken-cc.json", "{ not json");
  write(
    dir.path(),
    "good-cc.json",
    &file(2_000, json!([{ "id": "claude-opus-5-5", "section": "main" }])),
  );

  assert_eq!(ids(dir.path()), ["claude-opus-5-5"]);
}

/// a directory that is not there refuses
#[test]
fn a_directory_that_is_not_there_refuses() {
  let dir = tempfile::tempdir().unwrap();

  assert!(catalogue(dir.path()).unwrap_err().starts_with("no model catalogue under"));
}

/// a directory holding no file of the surface refuses
#[test]
fn a_directory_holding_no_file_of_the_surface_refuses() {
  let dir = tempfile::tempdir().unwrap();

  write(dir.path(), "notes.txt", "");

  assert!(catalogue(dir.path()).unwrap_err().starts_with("no model catalogue file under"));
}

/// a file that does not parse refuses
#[test]
fn a_file_that_does_not_parse_refuses() {
  let dir = tempfile::tempdir().unwrap();

  write(dir.path(), "abc-cc.json", "{ not json");

  assert!(catalogue(dir.path()).unwrap_err().contains("could not be read"));
}

/// a catalogue naming no model refuses rather than answering an empty list
#[test]
fn a_catalogue_naming_no_model_refuses_rather_than_answering_an_empty_list() {
  let dir = tempfile::tempdir().unwrap();

  write(dir.path(), "abc-cc.json", &file(1_000, json!([])));

  assert!(catalogue(dir.path()).unwrap_err().ends_with("names no model"));
}

/// a catalogue whose models are not a list refuses too
#[test]
fn a_catalogue_whose_models_are_not_a_list_refuses_too() {
  let dir = tempfile::tempdir().unwrap();

  write(
    dir.path(),
    "abc-cc.json",
    r#"{"version":2,"fetchedAt":1000,"staleAt":2000,"catalog":{"surface":"cc","config":{}}}"#,
  );

  assert!(catalogue(dir.path()).unwrap_err().ends_with("names no model"));
}

/// a read past the moment the CLI set for fetching again is stale, and one before it is not
#[test]
fn a_read_past_the_moment_the_cli_set_for_fetching_again_is_stale_and_one_before_it_is_not() {
  let dir = tempfile::tempdir().unwrap();

  write(
    dir.path(),
    "abc-cc.json",
    &file(1_000, json!([{ "id": "claude-opus-5-5", "section": "main" }])),
  );

  let read = catalogue(dir.path()).unwrap();

  assert!(!read.stale(3_600_000.0));
  assert!(read.stale(3_601_000.0));
}

/// a file carrying no fetch time is read as the oldest and always stale
#[test]
fn a_file_carrying_no_fetch_time_is_read_as_the_oldest_and_always_stale() {
  let dir = tempfile::tempdir().unwrap();

  write(
    dir.path(),
    "abc-cc.json",
    r#"{"version":2,"catalog":{"surface":"cc","config":{"models":[{"id":"claude-opus-5-5","section":"main"}]}}}"#,
  );

  let read = catalogue(dir.path()).unwrap();

  assert_eq!(read.fetched_at, 0.0);
  assert!(read.stale(1.0));
}

#[test]
fn the_prompts_of_a_folder_leave_the_history_and_every_other_line_stays_as_it_stood() {
  let dir = tempfile::tempdir().unwrap();
  let home = Path::new("/data/supervisor-homes/pal");
  let line = |project: &str, session: &str| {
    format!(
      "{}\n",
      json!({ "display": "words", "pastedContents": {}, "project": project, "sessionId": session })
    )
  };
  let other = line("/data/supervisor-homes/pal-2", "s1");
  let history = [
    line("/data/supervisor-homes/pal", "s1"),
    other.clone(),
    line("/data/supervisor-homes/pal/rooms", "s2"),
    "not a line of the history\n".to_owned(),
    line("/data/supervisor-homes/pal", "s3"),
  ];

  std::fs::write(dir.path().join(HISTORY), history.concat()).unwrap();

  assert_eq!(ClaudeCode::forget_prompts_under(dir.path(), home, Some("s3")).unwrap(), 1);
  assert_eq!(std::fs::read_to_string(dir.path().join(HISTORY)).unwrap(), history[..4].concat());
  assert_eq!(ClaudeCode::forget_prompts_under(dir.path(), home, None).unwrap(), 2);
  assert_eq!(
    std::fs::read_to_string(dir.path().join(HISTORY)).unwrap(),
    format!("{other}not a line of the history\n")
  );
  // The lock goes with the rewrite, and a configuration directory keeping no history has nothing to forget.
  assert!(!dir.path().join(format!("{HISTORY}.lock")).exists());
  assert_eq!(ClaudeCode::forget_prompts_under(&dir.path().join("none"), home, None).unwrap(), 0);
}

#[test]
fn a_working_directory_that_is_a_link_keeps_its_sessions_under_its_real_path() {
  let home = tempfile::tempdir().unwrap();
  let release = home.path().join("releases").join("abc123");
  let link = home.path().join("deploy");

  std::fs::create_dir_all(&release).unwrap();
  std::os::unix::fs::symlink(&release, &link).unwrap();

  let config = Path::new("/config");
  let named = |path: &Path| -> String {
    let real = std::fs::canonicalize(path).unwrap();

    real
      .to_string_lossy()
      .chars()
      .map(|one| if one.is_ascii_alphanumeric() { one } else { '-' })
      .collect()
  };

  assert_eq!(ClaudeCode::project_folder(config, &link), config.join("projects").join(named(&release)));
  assert_eq!(ClaudeCode::project_folder(config, &link), ClaudeCode::project_folder(config, &release));
  assert_eq!(
    ClaudeCode::project_folder(config, Path::new("/no/such/place")),
    config.join("projects").join("-no-such-place"),
    "a path that does not stand is named as given"
  );
}

#[test]
fn a_session_and_its_prompts_are_looked_for_under_the_machines_own_configuration_and_every_logins_home() {
  let dir = tempfile::tempdir().unwrap();
  let own = dir.path().join("own");
  let login = dir.path().join("login");
  let machine =
    Machine::with_env([("CLAUDE_CONFIG_DIR".to_owned(), own.display().to_string())], dir.path());
  let home = Path::new("/data/supervisor-homes/pal");
  let logins = [login.clone(), own.clone()];

  assert_eq!(ClaudeCode::config_dirs(&machine, &logins), [own.clone(), login.clone()]);
  assert_eq!(
    ClaudeCode.session_records(&machine, &logins, home, "s1", "m"),
    [
      ClaudeCode::records_under(&own, home, "s1"),
      ClaudeCode::records_under(&login, home, "s1")
    ]
    .concat()
  );

  std::fs::create_dir_all(&login).unwrap();
  std::fs::write(
    login.join(HISTORY),
    format!(
      "{}\n",
      json!({ "display": "words", "project": "/data/supervisor-homes/pal", "sessionId": "s1" })
    ),
  )
  .unwrap();

  assert_eq!(ClaudeCode.forget_prompts(&machine, &logins, home, Some("s1")).unwrap(), 1);
  assert_eq!(std::fs::read_to_string(login.join(HISTORY)).unwrap(), "");
}
