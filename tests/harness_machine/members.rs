//! The gateway's members: the file read, every member held to its rules, and a member of a road handed
//! over with the window, the answer ceiling and the thinking a run on it takes.

use harness_machine::members::{Member, for_road, read, validate};
use serde_json::json;

fn member(name: &str, road: Option<&str>) -> Member {
  Member {
    name: name.into(),
    road: road.map(str::to_owned),
    label: Some(name.into()),
    provider: "deepseek".into(),
    model: format!("{name}-model"),
    api_base: Some("https://api.deepseek.com/v1".into()),
    key_file: "deepseek.key".into(),
    context_tokens: Some(1_048_576),
    answer_tokens: Some(32_768),
    thinking: Some(true),
    free: None,
    account: None,
  }
}

#[test]
fn a_file_is_read_and_a_road_is_handed_its_members_with_what_a_run_is_sized_to() {
  let root = tempfile::tempdir().unwrap();
  let path = root.path().join("members.json");
  let text = json!([
    {
      "name": "deepseek-flash",
      "road": "deepseek",
      "label": "DeepSeek V4.1 Flash",
      "provider": "deepseek",
      "model": "deepseek-flash",
      "api_base": "https://api.deepseek.com/v1",
      "key_file": "deepseek.key",
      "context_tokens": 1048576,
      "answer_tokens": 32768,
      "thinking": true
    },
    {
      "name": "by-hand",
      "provider": "openrouter",
      "model": "deepseek/deepseek-v4.1-flash-1017",
      "key_file": "openrouter.key"
    }
  ]);
  std::fs::write(&path, text.to_string()).unwrap();

  let members = read(&path).unwrap();
  let deepseek = for_road(&members, "deepseek").unwrap();

  assert_eq!(members.len(), 2);
  assert_eq!(deepseek.len(), 1);
  assert_eq!(deepseek[0].member.name, "deepseek-flash");
  assert_eq!(
    (deepseek[0].context_tokens, deepseek[0].answer_tokens, deepseek[0].thinking),
    (1_048_576, 32_768, true)
  );
  assert!(for_road(&members, "openrouter").unwrap().is_empty());
}

#[test]
fn a_member_of_a_road_must_name_its_window_its_answer_and_whether_it_thinks() {
  for missing in ["context_tokens", "answer_tokens", "thinking"] {
    let mut one = member("deepseek-flash", Some("deepseek"));
    match missing {
      "context_tokens" => one.context_tokens = None,
      "answer_tokens" => one.answer_tokens = None,
      _ => one.thinking = None,
    }

    assert!(for_road(&[one], "deepseek").is_err(), "{missing} was not required");
  }
}

#[test]
fn a_file_that_does_not_read_or_a_member_that_breaks_a_rule_is_refused() {
  let root = tempfile::tempdir().unwrap();
  let path = root.path().join("members.json");

  std::fs::write(&path, "{").unwrap();
  assert!(read(&path).is_err());

  let mut bad_key = member("deepseek-flash", Some("deepseek"));
  bad_key.key_file = "../deepseek.key".into();
  assert!(validate(&[bad_key]).is_err());

  let mut bad_answer = member("deepseek-flash", Some("deepseek"));
  bad_answer.answer_tokens = Some(1_048_576);
  assert!(validate(&[bad_answer]).is_err());

  let twice = [member("deepseek-flash", Some("deepseek")), member("deepseek-flash", Some("deepseek"))];
  assert!(validate(&twice).is_err());

  let mut bad_provider = member("deepseek-flash", Some("deepseek"));
  bad_provider.provider = "nobody".into();
  assert!(validate(&[bad_provider]).is_err());
}

#[test]
fn a_file_that_is_not_there_is_no_members() {
  let root = tempfile::tempdir().unwrap();

  assert!(read(&root.path().join("members.json")).unwrap().is_empty());
}
