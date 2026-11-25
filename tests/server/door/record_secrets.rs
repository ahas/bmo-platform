//! What an agent place's records are read for before a line is written (docs/agent-control.md, 「A record
//! holds no secret」): the saved values the data root's `env/` holds, the shared secret shapes, and agency's own
//! token shapes; a record's keys and the fields its readers take left whole; and a record written before
//! rewritten line by line.

use std::os::unix::fs::PermissionsExt as _;

use agency_server::door::place_run::{read_hooks, redact_record};
use agency_server::door::record_secrets::{Readings, Saved, Source, Tally, names_secret, saved_values};
use serde_json::json;

const BEARER: &str = "abcdefghijklmnopqrstuvwxyz0123";
const SAVED: &str = "saved-0123456789-abcdef";

#[test]
fn a_shape_takes_out_its_secret_part_and_leaves_what_it_spares_and_ordinary_words() {
  let readings = Readings::shapes_only();
  let long = "A".repeat(320);

  // A lowercase bearer is the secret shapes' alone: agency's own reads `Bearer`.
  assert_eq!(
    readings.text(&format!("curl -H 'authorization: bearer {BEARER}'")),
    "curl -H 'authorization: bearer [redacted]'"
  );
  assert_eq!(readings.text("x-api-key: key0123456789abcd"), "x-api-key: [redacted]");
  assert_eq!(readings.text(&format!("session {long}")), "session [redacted]");

  for kept in [
    format!("https://example.com/go?to={long}"),
    "https://example.com/docs/page?id=42".to_owned(),
    "890a6c3 the commit".to_owned(),
  ] {
    assert_eq!(readings.text(&kept), kept);
  }
}

#[test]
fn agency_s_token_shapes_read_after_the_secret_shapes_and_count_as_one_source() {
  let readings = Readings::shapes_only();
  let mut tally = Tally::default();

  // A key of sixteen characters after `sk-` is agency's own shape alone; a lowercase bearer is the secret
  // shapes'.
  assert_eq!(
    readings
      .text_tallied(&format!("key sk-0123456789abcdef and authorization: bearer {BEARER}"), &mut tally),
    "key [redacted] and authorization: bearer [redacted]"
  );
  assert_eq!(tally.lines(), ["secret shape bearer-token: 1", "agency token shape: 1"]);
}

#[test]
fn the_saved_values_are_the_secret_ones_under_env_a_login_home_and_a_large_file_aside() {
  let env = tempfile::tempdir().unwrap();
  let at = |name: &str| env.path().join(name);

  std::fs::write(at("one.token"), format!("# the console's\n{SAVED}\n")).unwrap();
  std::fs::write(
    at("service.env"),
    concat!(
      "SERVICE_API_KEY=\"quoted-0123456789-abcdef\"\n",
      "SERVICE_PASSWORD=pw-0123456789\n",
      "SERVICE_URL=https://example.com/service\n",
      "SHORT_TOKEN=abc\n",
      "CACHE_TOKEN=/var/lib/service/token-file\n",
    ),
  )
  .unwrap();
  std::fs::write(
    at("bot.toml"),
    "name = \"a-bot-with-a-long-name\"\n[bot]\nbot_token = \"bot-0123456789-abcdef\"\n",
  )
  .unwrap();
  std::fs::create_dir_all(at("hooks/nested")).unwrap();
  std::fs::write(at("hooks/supply.token"), "supply-0123456789-abcdef\n").unwrap();
  std::fs::write(
    at("hooks/nested/shell.env"),
    "export NODE_TOKEN=node-0123456789-abcdef\nexport PATH=/usr/bin\n",
  )
  .unwrap();
  std::fs::write(
    at("hooks/nested/login.json"),
    r#"{"name":"a-bot-with-a-long-name","oauth":{"accessToken":"access-0123456789-abcdef","scopes":["user:inference"]}}"#,
  )
  .unwrap();
  std::fs::write(at("hooks/whole.json"), r#""whole-0123456789-abcdef""#).unwrap();
  // A file of one field is read as the field: a path under a plain name gives nothing, and a base64 value
  // ending on its padding is one value.
  std::fs::write(at("hooks/one-field.env"), "EVENT_HOOK=/srv/service/event-hook\n").unwrap();
  std::fs::write(at("hooks/padded.token"), "cGFkZGVkLXZhbHVlLWhlcmU==\n").unwrap();
  std::fs::write(at("hooks/large.token"), "l".repeat(70_000)).unwrap();
  std::fs::create_dir(at("login-home")).unwrap();
  std::fs::write(at("login-home/.claude.json"), "{}").unwrap();
  std::fs::write(at("login-home/inner.token"), "inner-0123456789-abcdef").unwrap();

  let mut saved = saved_values(env.path());

  saved.sort_by(|one, other| one.value.cmp(&other.value));
  assert_eq!(
    saved,
    [
      Saved::new("\"quoted-0123456789-abcdef\"", "service.env"),
      Saved::new("access-0123456789-abcdef", "hooks/nested/login.json"),
      Saved::new("bot-0123456789-abcdef", "bot.toml"),
      Saved::new("cGFkZGVkLXZhbHVlLWhlcmU==", "hooks/padded.token"),
      Saved::new("node-0123456789-abcdef", "hooks/nested/shell.env"),
      Saved::new("pw-0123456789", "service.env"),
      Saved::new("quoted-0123456789-abcdef", "service.env"),
      Saved::new(SAVED, "one.token"),
      Saved::new("supply-0123456789-abcdef", "hooks/supply.token"),
      Saved::new("whole-0123456789-abcdef", "hooks/whole.json"),
    ]
  );

  for named in [
    "BOT_TOKEN",
    "api_key",
    "service.apikey",
    "DB-PASSWORD",
    "Secret",
    "accessToken",
    "apiKey",
  ] {
    assert!(names_secret(named), "{named}");
  }

  for unnamed in ["KEY_FILE", "MONKEY", "apiary", "PATH", "tokenFile", "Monkey"] {
    assert!(!names_secret(unnamed), "{unnamed}");
  }
}

#[test]
fn a_record_keeps_its_keys_and_the_fields_its_readers_take() {
  let readings = Readings::of(vec![Saved::new(SAVED, "one.token")]);
  let session = "5f27643e-ae81-4401-83a3-bc4d195ba48b";
  let transcript = format!("/data/claude/projects/-data-agent-places-worker/{session}.jsonl");
  let mut record = json!({
    "at": 1,
    "kind": "stop",
    "payload": {
      "session_id": session,
      "transcript_path": transcript,
      "background_tasks": [{
        "id": "b7x2k9",
        "status": "running",
        "description": "Fetch the page",
        "command": format!("curl -H 'Authorization: Bearer {BEARER}' -d token={SAVED} https://example.com/docs"),
      }],
      "api_key": "kept as a key",
    },
  });

  assert!(readings.value(&mut record));
  assert_eq!(
    record,
    json!({
      "at": 1,
      "kind": "stop",
      "payload": {
        "session_id": session,
        "transcript_path": transcript,
        "background_tasks": [{
          "id": "b7x2k9",
          "status": "running",
          "description": "Fetch the page",
          "command": "curl -H 'Authorization: Bearer [redacted]' -d token=[redacted] https://example.com/docs",
        }],
        "api_key": "kept as a key",
      },
    })
  );
  assert!(!readings.value(&mut record), "words read twice lose nothing more");
}

#[test]
fn a_record_written_before_is_rewritten_where_a_line_changed_and_kept_byte_for_byte_elsewhere() {
  let place = tempfile::tempdir().unwrap();
  let hooks = place.path().join("hooks.jsonl");
  let readings = Readings::of(vec![Saved::new(SAVED, "one.token"), Saved::new(SAVED, "copy.env")]);
  let ordinary =
    r#"{"at":1,  "kind":"submit","payload":{"session_id":"s","prompt":"https://example.com/a?b=c"}}"#;
  let secret = format!(
    r#"{{"at":2,"kind":"stop","payload":{{"session_id":"s","last_assistant_message":"{SAVED}"}}}}"#
  );
  let unread = format!("not json: Bearer {BEARER} and x-api-key: key0123456789abcd");
  let mut tally = Tally::default();

  std::fs::write(&hooks, format!("{ordinary}\n{secret}\n{unread}\n")).unwrap();
  std::fs::set_permissions(&hooks, std::fs::Permissions::from_mode(0o600)).unwrap();

  assert_eq!(redact_record(&hooks, &readings, &mut tally).unwrap(), 2);
  // Each run is counted to the source that took it, a saved value by every file it stands in.
  assert_eq!(
    tally.lines(),
    [
      "saved value in one.token, copy.env: 1",
      "secret shape bearer-token: 1",
      "secret shape named-value: 1",
    ]
  );
  assert_eq!(tally.of(&Source::Agency), 0);

  let text = std::fs::read_to_string(&hooks).unwrap();

  assert_eq!(
    text,
    format!(
      "{ordinary}\n{}\nnot json: Bearer [redacted] and x-api-key: [redacted]\n",
      r#"{"at":2,"kind":"stop","payload":{"session_id":"s","last_assistant_message":"[redacted]"}}"#
    )
  );
  assert_eq!(std::fs::metadata(&hooks).unwrap().permissions().mode() & 0o777, 0o600);
  assert_eq!(read_hooks(place.path(), 0).len(), 2);

  // A record holding nothing more is left as it is, and nothing is left beside it.
  assert_eq!(redact_record(&hooks, &readings, &mut Tally::default()).unwrap(), 0);
  assert_eq!(std::fs::read_to_string(&hooks).unwrap(), text);
  assert_eq!(std::fs::read_dir(place.path()).unwrap().count(), 1);
}
