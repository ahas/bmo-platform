use agency_server::door::endpoint::*;
use agency_server::door::record::Reply;

#[test]
fn the_session_is_the_one_the_command_opens_or_resumes() {
  let words = |args: &[&str]| args.iter().map(|one| (*one).to_owned()).collect::<Vec<_>>();

  assert_eq!(session_named(&words(&["claude", "--session-id", "s1"])), Some("s1"));
  assert_eq!(session_named(&words(&["claude", "--resume", "s2", "--model", "m"])), Some("s2"));
  assert_eq!(session_named(&words(&["claude", "--model", "m"])), None);
}

#[test]
fn a_road_reply_is_the_roads_with_no_money_and_no_login() {
  let reply = Reply {
    mode: "claude-code".into(),
    model: Some("m".into()),
    effort: None,
    text: "done".into(),
    session_id: None,
    context_ratio: None,
    context_tokens: None,
    session_usage: None,
    windows: vec![],
    spend: vec![agency_server::door::record::ModelSpend {
      mode: "claude-code".into(),
      model: "m".into(),
      tokens: 3,
      input_tokens: 2,
      output_tokens: 1,
      cache_read_tokens: 0,
      cache_write_tokens: 0,
      cost: 0.5,
    }],
    live_tasks: vec![],
    workflows: vec![],
    account: Some("first".into()),
    last_request_usage: None,
    request_usages: None,
    answers: Vec::new(),
  };
  let road = road_reply("registered", reply);

  assert_eq!((road.mode.as_str(), road.account), ("registered", None));
  assert_eq!(
    road.spend.iter().map(|one| (one.mode.as_str(), one.cost)).collect::<Vec<_>>(),
    vec![("registered", 0.0)]
  );
}
