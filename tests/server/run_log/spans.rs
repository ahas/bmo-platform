//! Where a run's time went: a call timed to its answer and summed by its tool, and a co-worker named by
//! its kind and its errand.

use agency_server::run_log::json::Json;
use agency_server::run_log::spans::{SpanTally, coworker_name};

#[test]
fn a_call_is_timed_to_its_answer_and_summed_by_its_tool() {
  let mut tally = SpanTally::default();
  let call = |id: &str| {
    Json::parse(&format!(
      r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"{id}","name":"Bash","input":{{}}}}]}}}}"#
    ))
    .unwrap()
  };
  let answer = |id: &str| {
    Json::parse(&format!(
      r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"{id}"}}]}}}}"#
    ))
    .unwrap()
  };

  tally.see(&call("a"));
  tally.see(&answer("a"));
  tally.see(&call("b"));
  tally.see(&answer("b"));

  let spans = tally.spans();

  assert_eq!(spans.len(), 1);
  assert_eq!((spans[0].kind.as_str(), spans[0].name.as_str(), spans[0].times), ("tool", "Bash", 2));
}

#[test]
fn a_co_worker_is_named_by_its_kind_and_its_errand() {
  let input = Json::parse(r#"{"subagent_type":"scout","description":"find  the\nlock"}"#).unwrap();

  assert_eq!(coworker_name("Agent", &input), "Agent(scout) · find the lock");
}
