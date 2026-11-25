use serde_json::{Value, json};

use agency_server::door::record::*;

fn assistant(text: Option<&str>, model: &str, sidechain: bool, usage: [i64; 4]) -> Value {
  let [read, made, input, output] = usage;
  let content: Vec<Value> = text.map(|text| json!({ "type": "text", "text": text })).into_iter().collect();

  json!({
    "type": "assistant",
    "isSidechain": sidechain,
    "effort": "high",
    "message": {
      "role": "assistant",
      "model": model,
      "content": content,
      "usage": { "input_tokens": input, "output_tokens": output, "cache_read_input_tokens": read, "cache_creation_input_tokens": made },
    },
  })
}

fn said(text: &str) -> Value {
  assistant(Some(text), "claude-opus-5", false, [0, 0, 0, 0])
}

fn tool(id: &str, name: &str, input: Value) -> Value {
  let mut line = assistant(None, "claude-opus-5", false, [0, 0, 0, 0]);

  line["message"]["content"] = json!([{ "type": "tool_use", "id": id, "name": name, "input": input }]);
  line
}

fn answered(id: &str) -> Value {
  json!({ "type": "user", "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": id, "content": "done" }] } })
}

fn launched_as(id: &str, text: &str) -> Value {
  json!({ "type": "user", "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": id, "content": [{ "type": "text", "text": text }] }] } })
}

fn ended_as(id: &str, status: &str) -> Value {
  json!({
    "type": "queue-operation",
    "operation": "enqueue",
    "content": format!("<task-notification>\n<task-id>t-{id}</task-id>\n<tool-use-id>{id}</tool-use-id>\n<status>{status}</status>\n<summary>done</summary>\n</task-notification>"),
  })
}

const WORKFLOW_LAUNCH: &str = "Workflow launched in background. Task ID: wqdscirps\nSummary: two answers";

fn workflow(name: &str) -> Value {
  tool(
    "w1",
    "Workflow",
    json!({ "script": format!("export const meta = {{ name: '{name}', description: 'x' }}") }),
  )
}

#[test]
fn a_response_split_into_several_lines_is_counted_once_and_answers_with_all_of_its_text() {
  let split = |block: Value| {
    let mut line = assistant(None, "claude-opus-5", false, [0, 53_544, 2, 100]);

    line["message"]["id"] = json!("msg_one");
    line["message"]["content"] = json!([block]);
    line
  };
  let lines = [
    said("an earlier answer"),
    split(json!({ "type": "thinking", "thinking": "…" })),
    split(json!({ "type": "text", "text": "Kept " })),
    assistant(Some("a co-worker's words"), "claude-opus-5", true, [0, 0, 0, 0]),
    split(json!({ "type": "text", "text": "warm." })),
  ];
  let turn = read_turn(&lines[1..], 0);

  assert_eq!(
    serde_json::to_value(&turn.spend).unwrap(),
    json!([{ "mode": "claude-code", "model": "claude-opus-5", "tokens": 53_646, "inputTokens": 2, "outputTokens": 100, "cacheReadTokens": 0, "cacheWriteTokens": 53_544, "cost": 0.0 }])
  );
  assert_eq!(turn.text, "Kept warm.");
  assert_eq!(
    answer_since(&lines, 1),
    turn.text,
    "the progress read answers what the whole reading does"
  );
}

#[test]
fn the_answer_is_the_last_thing_said_and_the_cost_every_line_the_turn_added() {
  assert_eq!(read_turn(&[said("thinking about it"), said("the answer")], 0).text, "the answer");

  let mut call = assistant(None, "claude-opus-5", false, [33_330, 31_080, 2, 122]);

  call["message"]["content"] = json!([{ "type": "tool_use", "id": "t1", "name": "Bash", "input": {} }]);

  let turn = read_turn(
    &[
      call,
      answered("t1"),
      assistant(Some("DONE"), "claude-opus-5", false, [64_410, 161, 2, 5]),
    ],
    0,
  );
  let spend = &turn.spend[0];

  assert_eq!(spend.tokens, 33_330 + 31_080 + 2 + 122 + 64_410 + 161 + 2 + 5);
  assert_eq!((spend.input_tokens, spend.output_tokens), (4, 127));
  assert_eq!((spend.cache_read_tokens, spend.cache_write_tokens), (33_330 + 64_410, 31_080 + 161));
}

#[test]
fn what_came_before_the_turn_is_not_the_turn_s_and_a_co_worker_s_lines_are_paid_for_but_not_the_answer() {
  let records = [
    assistant(Some("an earlier turn"), "claude-opus-5", false, [0, 0, 0, 500]),
    assistant(Some("this one"), "claude-opus-5", false, [0, 0, 0, 7]),
  ];
  let turn = read_turn(&records, 1);

  assert_eq!((turn.text.as_str(), turn.spend[0].output_tokens), ("this one", 7));

  let turn = read_turn(
    &[
      assistant(Some("the answer"), "claude-opus-5", false, [0, 0, 0, 10]),
      assistant(Some("a co-worker's report"), "claude-opus-5", true, [0, 0, 0, 4_000]),
      assistant(None, "claude-haiku-4-5", true, [0, 0, 0, 30]),
    ],
    0,
  );

  assert_eq!(turn.text, "the answer");
  assert_eq!(
    turn.spend.iter().map(|one| (one.model.as_str(), one.output_tokens)).collect::<Vec<_>>(),
    [("claude-opus-5", 4_010), ("claude-haiku-4-5", 30)]
  );
}

#[test]
fn the_model_effort_and_context_come_off_the_last_line_and_an_empty_turn_says_so() {
  let turn = read_turn(&[assistant(Some("ok"), "claude-opus-5", false, [64_410, 161, 2, 0])], 0);

  assert_eq!(
    (turn.model.as_deref(), turn.effort.as_deref(), turn.context_tokens),
    (Some("claude-opus-5"), Some("high"), Some(64_573))
  );

  let none = read_turn(&[said("an earlier turn")], 1);

  assert_eq!(
    (none.text.as_str(), none.model, none.context_tokens, none.spend.len()),
    ("", None, None, 0)
  );

  let mut two = said("");

  two["message"]["content"] =
    json!([{ "type": "text", "text": "first line\n" }, { "type": "text", "text": "second line" }]);
  assert_eq!(read_turn(&[two], 0).text, "first line\nsecond line");
}

#[test]
fn only_a_person_s_prompt_after_the_point_counts_as_submitted() {
  let before = vec![json!({ "type": "user", "message": { "role": "user", "content": "an earlier turn" } })];
  let coming = vec![
    json!({ "type": "system", "subtype": "local_command" }),
    json!({ "type": "attachment" }),
    json!({ "type": "user", "isMeta": true, "message": { "role": "user", "content": "<local-command-caveat>" } }),
    json!({ "type": "user", "message": { "role": "user", "content": [{ "type": "tool_result", "content": "done" }] } }),
    json!({ "type": "user", "isSidechain": true, "message": { "role": "user", "content": "a co-worker's" } }),
  ];
  let mut records = [before.clone(), coming].concat();

  assert!(!prompted_since(&records, 1));
  records.push(json!({ "type": "user", "message": { "role": "user", "content": "the job" } }));
  assert!(prompted_since(&records, 1));
  assert!(prompted_since(
    &[
      before[0].clone(),
      json!({ "type": "user", "message": { "role": "user", "content": [{ "type": "text", "text": "the job" }] } })
    ],
    1
  ));
}

#[test]
fn the_work_a_turn_walked_away_from_is_its_own_launches_with_no_ending() {
  let left = read_turn(&[workflow("review-changes"), launched_as("w1", WORKFLOW_LAUNCH), said("going")], 0);

  assert_eq!(
    left.workflows,
    [RunWorkflow {
      call: "w1".into(),
      name: "review-changes".into(),
      status: None
    }]
  );

  let stopped = read_turn(
    &[workflow("stopped-one"), launched_as("w1", WORKFLOW_LAUNCH), ended_as("w1", "stopped")],
    0,
  );
  let taken_up = json!({ "type": "attachment", "attachment": { "type": "queued_command", "prompt": ended_as("w1", "completed")["content"] } });
  let finished = read_turn(&[workflow("done-one"), launched_as("w1", WORKFLOW_LAUNCH), taken_up], 0);

  assert_eq!(stopped.workflows[0].status.as_deref(), Some("stopped"));
  assert_eq!(finished.workflows[0].status.as_deref(), Some("completed"));
  assert!(
    read_turn(
      &[
        workflow("broken"),
        launched_as("w1", "<tool_use_error>Invalid workflow script</tool_use_error>")
      ],
      0
    )
    .workflows
    .is_empty()
  );

  let branch = tool("b1", "Task", json!({ "subagent_type": "scout", "description": "find the caller" }));

  assert_eq!(
    read_turn(std::slice::from_ref(&branch), 0).live_tasks,
    [LiveTask {
      id: "b1".into(),
      description: "find the caller".into()
    }]
  );
  assert!(read_turn(&[branch, answered("b1")], 0).live_tasks.is_empty());

  let launch = [
    tool("c1", "Bash", json!({ "command": "bun run test", "description": "seed the repository" })),
    launched_as(
      "c1",
      "Command running in background with ID: bz1925uj5. Output is being written to: /tmp/x.output.",
    ),
  ];

  assert_eq!(
    read_turn(&launch, 0).live_tasks,
    [LiveTask {
      id: "c1".into(),
      description: "seed the repository".into()
    }]
  );
  assert!(
    read_turn(&[launch[0].clone(), launch[1].clone(), ended_as("c1", "completed")], 0)
      .live_tasks
      .is_empty()
  );
  assert!(
    read_turn(&[workflow("left-behind"), launched_as("w1", WORKFLOW_LAUNCH), said("done")], 2)
      .workflows
      .is_empty()
  );

  let ordinary = read_turn(&[tool("t1", "Bash", json!({ "command": "ls" })), answered("t1")], 0);

  assert!(ordinary.live_tasks.is_empty() && ordinary.workflows.is_empty());
}

fn response(id: &str, text: &str) -> Value {
  json!({ "type": "assistant", "sessionId": "s1", "message": { "id": id, "model": "claude-sonnet-5", "content": [{ "type": "text", "text": text }], "usage": { "input_tokens": 10, "output_tokens": 5 } } })
}

fn step(id: &str) -> Value {
  json!({ "type": "assistant", "sessionId": "s1", "message": { "id": id, "model": "claude-sonnet-5", "content": [{ "type": "tool_use", "id": format!("t{id}"), "name": "Bash", "input": {} }], "usage": { "input_tokens": 3, "output_tokens": 2 } } })
}

fn end() -> Value {
  json!({ "type": "system", "subtype": "turn_duration" })
}

fn queue(operation: &str) -> Value {
  json!({ "type": "queue-operation", "operation": operation })
}

fn words(text: &str) -> Value {
  json!({ "type": "user", "message": { "content": text } })
}

#[test]
fn a_round_is_going_until_its_end_line_and_a_word_taken_inside_it_leaves_nothing_queued() {
  assert_eq!(
    read_round(&[json!({ "type": "user" }), step("a")], Notices::Joined).state,
    RoundState::Working
  );

  let done =
    read_round(&[json!({ "type": "user" }), step("a"), response("b", "answer"), end()], Notices::Joined);

  assert_eq!((done.state, done.text.as_str()), (RoundState::Ended, "answer"));
  assert_eq!(
    read_turn(&[json!({ "type": "user" }), step("a"), response("b", "answer"), end()], 0).spend[0]
      .input_tokens,
    13
  );

  let lines = [
    step("a"),
    queue("enqueue"),
    queue("remove"),
    json!({ "type": "attachment" }),
    response("b", "both"),
    end(),
  ];

  assert_eq!(turns_owed(&lines, Notices::Joined).queued, 0);
  assert_eq!(read_round(&lines, Notices::Joined).state, RoundState::Ended);
}

#[test]
fn a_word_pushed_after_the_last_tool_call_opens_a_turn_of_its_own_and_the_round_answers_with_the_last() {
  let lines = vec![step("a"), queue("enqueue"), response("b", "first answer"), end()];

  assert_eq!(turns_owed(&lines, Notices::Joined).queued, 1);
  assert_eq!(read_round(&lines, Notices::Joined).state, RoundState::Working);

  let after = [lines, vec![queue("dequeue"), words("WORD-X"), response("c", "PINEAPPLE"), end()]].concat();
  let round = read_round(&after, Notices::Joined);

  assert_eq!(turns_owed(&after, Notices::Joined).queued, 0);
  assert_eq!((round.state, round.text.as_str()), (RoundState::Ended, "PINEAPPLE"));
}

#[test]
fn a_round_hands_on_each_turns_answer_with_what_opened_the_turn() {
  let notice =
    json!({ "type": "user", "message": { "content": "<task-notification>done</task-notification>" } });
  let lines = [
    words("ROUND"),
    step("a"),
    queue("enqueue"),
    response("b", "<<SEND>> the reply"),
    end(),
    queue("dequeue"),
    words("MORE FOR THIS TURN."),
    response("c", "<<SKIP: nothing new>>"),
    end(),
    notice,
    response("d", "the task finished"),
    end(),
  ];
  let round = read_round(&lines, Notices::Joined);

  assert_eq!(round.text, "<<SKIP: nothing new>>\n\nthe task finished");
  assert_eq!(
    round.answers,
    [
      agent_session::TurnAnswer {
        text: "<<SEND>> the reply".into(),
        opened: agent_session::Opened::Handed,
      },
      agent_session::TurnAnswer {
        text: "<<SKIP: nothing new>>".into(),
        opened: agent_session::Opened::Handed,
      },
      agent_session::TurnAnswer {
        text: "the task finished".into(),
        opened: agent_session::Opened::Program,
      },
    ]
  );
}

#[test]
fn words_that_waited_together_open_one_turn_and_an_answer_written_again_goes_out_once() {
  let stop_summary = json!({ "type": "system", "subtype": "stop_hook_summary" });
  let lines = vec![
    words("ROUND"),
    step("a"),
    queue("enqueue"),
    queue("remove"),
    step("b"),
    queue("enqueue"),
    queue("enqueue"),
    response("c", "first answer"),
    queue("dequeue"),
    queue("dequeue"),
    stop_summary.clone(),
    end(),
  ];

  assert_eq!(turns_owed(&lines, Notices::Joined).queued, 0);
  assert_eq!(read_round(&lines, Notices::Joined).state, RoundState::Working);

  let after = [
    lines,
    vec![
      words("WORD-X"),
      json!({ "type": "attachment" }),
      words("WORD-Y"),
      response("d", "both words"),
      stop_summary.clone(),
      end(),
    ],
  ]
  .concat();
  let round = read_round(&after, Notices::Joined);

  assert_eq!((round.state, round.text.as_str()), (RoundState::Ended, "both words"));

  let draft = "Not written yet.\n\n| Gate | Accuracy |\n|---|---|\n| people-search | 0.878 |";
  let rewritten = format!("{draft}\n\nThe audio for B-1000 sentence 2 is uploaded again.");
  let lines = [
    words("ROUND"),
    step("a"),
    queue("enqueue"),
    response("b", draft),
    queue("dequeue"),
    stop_summary.clone(),
    end(),
    words("More for the answer you are making now: and upload the audio too so I can check it"),
    step("c"),
    step("d"),
    response("e", &rewritten),
    stop_summary,
    end(),
  ];
  let round = read_round(&lines, Notices::Joined);

  assert_eq!((round.state, round.text.as_str()), (RoundState::Ended, rewritten.as_str()));
}

#[test]
fn turns_owed_count_stretches_words_taken_straight_and_words_pulled_back() {
  let lines = vec![
    step("a"),
    queue("enqueue"),
    queue("enqueue"),
    response("b", "one"),
    queue("dequeue"),
    end(),
    words("WORD-X"),
    response("c", "two"),
    queue("dequeue"),
    end(),
    words("WORD-Y"),
  ];

  assert_eq!(turns_owed(&lines, Notices::Joined), Owed { queued: 0, owed: 3 });
  assert_eq!(read_round(&lines, Notices::Joined).state, RoundState::Working);
  assert_eq!(
    read_round(&[lines, vec![response("d", "three"), end()]].concat(), Notices::Joined).state,
    RoundState::Ended
  );

  let lines = vec![
    words("ROUND"),
    response("a", "first"),
    json!({ "type": "system", "subtype": "stop_hook_summary" }),
    end(),
    words("More for the answer you are making now: 2 new message(s)"),
  ];

  assert_eq!(turns_owed(&lines, Notices::Joined), Owed { queued: 0, owed: 2 });
  assert_eq!(read_round(&lines, Notices::Joined).state, RoundState::Working);

  let ended = read_round(&[lines, vec![response("b", "rewritten"), end()]].concat(), Notices::Joined);

  assert_eq!((ended.state, ended.text.as_str()), (RoundState::Ended, "rewritten"));

  let pulled = [
    step("a"),
    queue("enqueue"),
    queue("enqueue"),
    queue("popAll"),
    response("b", "x"),
    end(),
  ];

  assert_eq!(turns_owed(&pulled, Notices::Joined), Owed { queued: 0, owed: 1 });
  assert_eq!(read_round(&pulled, Notices::Joined).state, RoundState::Ended);
}

#[test]
fn an_escape_is_an_interrupted_round_and_the_program_s_error_answers_nothing() {
  let escaped = [
    step("a"),
    json!({ "type": "user", "message": { "content": [{ "type": "text", "text": "[Request interrupted by user for tool use]" }] } }),
  ];

  assert_eq!(read_round(&escaped, Notices::Joined).state, RoundState::Interrupted);

  let failed = json!({ "type": "assistant", "isApiErrorMessage": true, "message": { "id": "e", "model": "<synthetic>", "content": [{ "type": "text", "text": "API Error: 529 Overloaded" }] } });

  assert_eq!(
    read_turn(&[failed.clone(), end()], 0).error.as_deref(),
    Some("API Error: 529 Overloaded")
  );
  // An error line that names no kind has none.
  assert_eq!(read_turn(&[failed.clone(), end()], 0).error_kind, None);
  assert_eq!(read_round(&[failed, end()], Notices::Joined).text, "");
}

// The 429 a full window answers with, as Claude Code writes it into the record before standing at its own
// choice of what to do next: the turn is over by the record, whatever the pane shows.
#[test]
fn a_turn_the_program_ended_on_a_full_window_is_over_by_its_record() {
  let full = json!({ "type": "assistant", "isApiErrorMessage": true, "error": "rate_limit", "apiErrorStatus": 429, "message": { "id": "r", "model": "<synthetic>", "content": [{ "type": "text", "text": "You've hit your weekly limit · resets Oct 16, 6am (Asia/Seoul)" }] } });
  let prompt = json!({ "type": "user", "message": { "content": "say hi" } });

  assert!(ended_on_error(&[prompt.clone(), full.clone(), end()], 0));
  assert_eq!(
    read_turn(&[prompt.clone(), full.clone(), end()], 0).error_kind.as_deref(),
    Some("rate_limit")
  );
  // Not before the line the turn ends with, and not past it.
  assert!(!ended_on_error(&[prompt.clone(), full.clone()], 0));
  assert!(!ended_on_error(&[prompt.clone(), full, end()], 3));
  assert!(!ended_on_error(&[prompt, assistant(Some("hi"), "m", false, [1, 1, 0, 0]), end()], 0));
}

// On a full window Claude Code writes its error in the answer's place and opens its own choice of what to do
// next, writing no closing line: a turn whose conversation ends on that error has ended on it.
#[test]
fn a_turn_whose_conversation_ends_on_the_program_s_error_rests_on_it() {
  let full = json!({ "type": "assistant", "isApiErrorMessage": true, "error": "rate_limit", "message": { "id": "r", "model": "<synthetic>", "content": [{ "type": "text", "text": "You've hit your weekly limit" }] } });
  let prompt = json!({ "type": "user", "message": { "content": "say hi" } });
  let notice = json!({ "type": "system", "subtype": "informational", "content": "Unknown command: /rate-limit-options" });

  assert!(rests_on_error(&[prompt.clone(), full.clone(), notice.clone()], 0));
  // Not where the turn went on past it, nor for an error before the turn began.
  assert!(!rests_on_error(&[prompt.clone(), full.clone(), said("hi")], 0));
  assert!(!rests_on_error(&[full.clone(), prompt.clone()], 0));
  assert!(!rests_on_error(&[full, notice, prompt], 2));
}

// A failed request's end the place recorded names the turn's error where the record holds no answer yet,
// and lends its kind to an error the record names none for; an answered turn is no error whatever the place
// recorded.
#[test]
fn a_turn_s_error_is_its_record_s_own_or_its_place_s_failure_where_the_record_holds_no_answer() {
  let prompt = json!({ "type": "user", "message": { "content": "say hi" } });
  let unnamed = json!({ "type": "assistant", "isApiErrorMessage": true, "message": { "id": "e", "model": "<synthetic>", "content": [{ "type": "text", "text": "API Error: 503 card busy" }] } });
  let failure = || Some(("API Error: 503 card busy".to_owned(), Some("server_error".to_owned())));
  let errored = read_turn(&[prompt.clone(), unnamed, end()], 0);
  let answered = read_turn(&[prompt.clone(), assistant(Some("hi"), "m", false, [1, 1, 0, 0]), end()], 0);
  let nothing = read_turn(&[prompt], 0);

  assert_eq!(
    turn_error(&errored, true, failure()),
    Some(("API Error: 503 card busy".to_owned(), Some("server_error".to_owned())))
  );
  assert_eq!(turn_error(&errored, true, None), Some(("API Error: 503 card busy".to_owned(), None)));
  assert_eq!(turn_error(&nothing, false, failure()), failure());
  assert_eq!(turn_error(&nothing, false, None), None);
  assert_eq!(turn_error(&answered, true, failure()), None);
}

fn status(session: &str, total: f64, limits: Option<Value>) -> Value {
  let mut reading = json!({ "session_id": session, "model": { "id": "claude-sonnet-5" }, "cost": { "total_cost_usd": total }, "context_window": { "used_percentage": 12 } });

  if let Some(limits) = limits {
    reading["rate_limits"] = limits;
  }

  reading
}

#[test]
fn the_money_is_the_running_total_s_difference_within_one_session_and_nothing_across_two() {
  let same = status_figures(Some(&status("s1", 0.25, None)), Some(&status("s1", 0.3, None)));

  assert!((same.money.unwrap() - 0.05).abs() < 1e-10);
  assert_eq!(
    status_figures(Some(&status("s1", 0.25, None)), Some(&status("s2", 0.01, None))).money,
    None
  );
  assert_eq!(status_figures(None, Some(&status("s1", 0.3, None))).money, None);
}

#[test]
fn windows_come_from_the_reading_after_the_turn_as_fractions_and_are_absent_before_a_request() {
  let limits = json!({ "five_hour": { "used_percentage": 22, "resets_at": 1_789_962_600 }, "seven_day": { "used_percentage": 10, "resets_at": 1_790_528_400 } });
  let read = status_figures(None, Some(&status("s1", 0.1, Some(limits))));
  let none = status_figures(None, Some(&status("s1", 0.1, None)));

  assert_eq!(
    read.session_usage,
    Some(SessionUsage {
      utilization: 0.22,
      resets_at: 1_789_962_600.0,
      weekly_utilization: Some(0.1),
      weekly_resets_at: Some(1_790_528_400.0)
    })
  );
  assert_eq!(
    read.windows.iter().map(|window| window.name.as_str()).collect::<Vec<_>>(),
    ["five_hour", "seven_day"]
  );
  assert!((read.context_ratio.unwrap() - 0.12).abs() < 1e-10);
  assert_eq!(read.session_id.as_deref(), Some("s1"));
  assert_eq!((none.session_usage, none.windows.len()), (None, 0));
}

#[test]
fn the_last_activity_names_the_tool_called_with_what_it_was_given() {
  let mut call = tool("t1", "Bash", json!({ "command": "bun   test\n  server", "description": "x" }));

  call["timestamp"] = json!("2026-10-08T01:02:03.000Z");

  let activity = last_activity(&[said("before"), call], 0);

  assert_eq!(activity.what.as_deref(), Some("called Bash (bun test server)"));
  assert_eq!(
    activity.at,
    Some(
      chrono::DateTime::parse_from_rfc3339("2026-10-08T01:02:03.000Z")
        .unwrap()
        .timestamp_millis()
    )
  );
  assert_eq!(last_activity(&[answered("t1")], 0).what.as_deref(), Some("a tool's answer came back"));
  assert_eq!(last_activity(&[words("x")], 0).what, None);
  assert_eq!(
    last_activity(
      &[json!({ "type": "user", "message": { "content": [{ "type": "text", "text": "x" }] } })],
      0
    )
    .what
    .as_deref(),
    Some("was handed words")
  );
  assert_eq!(
    last_activity(&[], 0),
    Activity {
      at: None,
      what: None
    }
  );
}

#[test]
fn a_record_is_read_as_it_grows_and_a_last_line_still_being_written_counts_only_once_whole() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("s.jsonl");
  let mut tail = Tail::new(&path);

  assert!(tail.look().is_empty());
  std::fs::write(&path, "{\"a\":1}\nnot json\n{\"b\":").unwrap();
  assert_eq!(tail.look(), [json!({ "a": 1 })]);
  std::fs::write(&path, "{\"a\":1}\nnot json\n{\"b\":2}").unwrap();
  assert_eq!(tail.look(), [json!({ "a": 1 }), json!({ "b": 2 })]);
  std::fs::write(&path, "{\"c\":3}\n").unwrap();
  assert_eq!(tail.look(), [json!({ "c": 3 })]);
}

fn appended(path: &std::path::Path, lines: &[Value]) {
  use std::io::Write;

  let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();

  for line in lines {
    writeln!(file, "{line}").unwrap();
  }
}

fn account(tokens: i64) -> Value {
  json!({ "type": "cost-state", "modelUsage": { "claude-sonnet-5": { "inputTokens": tokens, "costUSD": 0.5 } } })
}

// A long session's history: many rounds, each a prompt, a long answer and its end.
fn history(rounds: usize) -> Vec<Value> {
  let long = "x".repeat(2000);

  (0..rounds)
    .flat_map(|at| [words(&format!("prompt {at}")), response(&format!("r{at}"), &long), end()])
    .collect()
}

#[test]
fn a_reader_from_the_end_holds_only_what_the_record_gained_and_what_a_whole_reading_turns_on() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("s.jsonl");

  appended(&path, &history(2000));
  appended(&path, &[account(100), queue("enqueue"), words("still being answered")]);

  let mut tail = Tail::at_end(&path);
  let from = tail.count();

  assert_eq!(from, 6003, "every line keeps its number from the record's start");
  assert_eq!(
    tail.held(),
    Held {
      records: 4,
      raw_bytes: 0
    },
    "before the floor: the last end, the account line, the queue operation and the last line of the conversation"
  );

  appended(&path, &[queue("dequeue"), response("new", "the turn's answer"), account(130), end()]);

  let mut whole = Tail::new(&path);
  let records = tail.look();

  assert_eq!(records.len(), from + 4);
  assert_eq!(read_turn(records, from).text, "the turn's answer");
  assert_eq!(
    agency_server::door::turn::spend_since(records, from).unwrap()[0].input_tokens,
    30,
    "the spend is measured from the account line before the turn"
  );
  assert_eq!(rests_apart(records), rests_apart(whole.look()));
  assert_eq!(ends_on_turn_end(records), ends_on_turn_end(whole.look()));
  assert_eq!(
    turns_owed(records, Notices::Apart).queued,
    turns_owed(whole.look(), Notices::Apart).queued
  );
  assert_eq!(tail.held().records, 8);
}

#[test]
fn a_reader_from_the_end_of_a_record_not_written_yet_holds_all_it_gains() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("s.jsonl");
  let mut tail = Tail::at_end(&path);

  assert_eq!(tail.count(), 0);
  appended(&path, &[words("the first prompt"), response("a", "the first answer")]);
  assert_eq!(read_turn(tail.look(), 0).text, "the first answer");
  assert_eq!(tail.held().records, 2);
}

#[test]
fn a_line_still_being_written_stays_above_a_floor_set_at_the_end() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("s.jsonl");

  std::fs::write(&path, format!("{}\n{}", words("before"), response("a", "being written"))).unwrap();

  let mut tail = Tail::at_end(&path);

  assert_eq!(tail.count(), 2);
  std::fs::write(&path, format!("{}\n{}\n", words("before"), response("a", "being written"))).unwrap();
  assert_eq!(read_turn(tail.look(), 1).text, "being written");
}

#[test]
fn a_raised_floor_lets_go_of_the_lines_before_it_and_a_filed_line_of_its_words_as_written() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("s.jsonl");

  appended(&path, &history(10));

  let mut tail = Tail::new(&path);

  tail.look();
  assert_eq!(tail.held().records, 30);
  tail.forget_raw_before(30);
  assert_eq!(tail.held().raw_bytes, 0);
  assert_eq!(tail.raw().len(), 30, "a line as written keeps its place once let go");
  tail.forget_before(27);
  assert_eq!(
    tail.held().records,
    2 + 3,
    "the last line of the conversation and the last end, then the last round"
  );
  assert_eq!(read_turn(tail.look(), 27).text, "x".repeat(2000));
}

#[test]
fn a_child_is_asked_for_by_its_call_and_ended_by_its_notice() {
  let mut reader = ChildReader::default();
  let asked = tool("toolu_1", "Agent", json!({}));
  let said = reader.read(&asked, |_| false);

  assert_eq!(
    said,
    [ChildEvidence {
      id: "toolu_1".into(),
      state: "requested",
      evidence: "claude.tool_use".into()
    }]
  );

  let launched = json!({ "type": "user", "toolUseResult": { "status": "async_launched", "agentId": "a1" }, "message": { "content": [{ "type": "tool_result", "tool_use_id": "toolu_1" }] } });

  assert_eq!(reader.read(&launched, |id| id == "toolu_1")[0].state, "started");

  let notice =
    json!({ "type": "system", "subtype": "task_notification", "task_id": "a1", "status": "completed" });
  let ended = reader.read(&notice, |id| id == "toolu_1");

  assert_eq!(
    ended,
    [ChildEvidence {
      id: "toolu_1".into(),
      state: "completed",
      evidence: "claude.task_notification:completed".into()
    }]
  );
}

fn spent(mode: &str, model: &str, tokens: i64) -> ModelSpend {
  ModelSpend {
    mode: mode.into(),
    model: model.into(),
    tokens,
    input_tokens: tokens,
    output_tokens: 0,
    cache_read_tokens: 0,
    cache_write_tokens: 0,
    cost: 0.5,
  }
}

#[test]
fn what_a_run_spent_is_published_row_by_row_under_each_rows_login_and_nothing_where_it_spent_nothing() {
  let spend = [
    spent("claude-code", "model-a", 10),
    spent("codex", "model-b", 4),
    spent("claude-code", "model-c", 0),
  ];
  let published = UsageSpent::of("worker", Some("run-1"), 100, &spend, |one| {
    if one.mode == "codex" { String::new() } else { "acct".into() }
  })
  .unwrap();

  assert_eq!(published.rows.len(), 2, "a model that spent nothing is no row");
  assert_eq!(
    serde_json::to_value(&published).unwrap(),
    json!({
      "agent": "worker",
      "run": "run-1",
      "at": 100,
      "rows": [
        { "account": "acct", "mode": "claude-code", "model": "model-a", "tokens": 10, "inputTokens": 10,
          "outputTokens": 0, "cacheReadTokens": 0, "cacheWriteTokens": 0, "cost": 0.5 },
        { "account": "", "mode": "codex", "model": "model-b", "tokens": 4, "inputTokens": 4,
          "outputTokens": 0, "cacheReadTokens": 0, "cacheWriteTokens": 0, "cost": 0.5 },
      ],
    })
  );
  assert_eq!(UsageSpent::of("worker", None, 100, &spend[2..], |_| String::new()), None);
}

// The lines a command of the program's own writes, as Claude Code 2.1.318 was measured to write them.
#[test]
fn a_command_s_words_are_read_off_its_user_or_system_line_and_its_error_apart() {
  let lines = |said: Value| vec![json!({ "type": "user", "message": { "content": "/compact" } }), said];
  let compacted = lines(json!({ "type": "user", "message": { "content":
    "<local-command-stdout>\u{1b}[2mCompacted (ctrl+o to see full summary)\u{1b}[22m\n\u{1b}[2mPreCompact completed\u{1b}[22m</local-command-stdout>" } }));
  let thin = lines(json!({ "type": "system", "subtype": "local_command",
    "content": "<local-command-stdout>Not enough messages to compact.</local-command-stdout>" }));
  let failed = lines(json!({ "type": "system", "subtype": "local_command",
    "content": "<local-command-stderr>Error during compaction: API Error</local-command-stderr>" }));

  assert_eq!(
    command_words(&compacted, 0),
    Some(CommandWords {
      text: "Compacted (ctrl+o to see full summary)\nPreCompact completed".to_owned(),
      error: false
    })
  );
  assert_eq!(
    command_words(&thin, 0),
    Some(CommandWords {
      text: "Not enough messages to compact.".to_owned(),
      error: false
    })
  );
  assert_eq!(
    command_words(&failed, 0),
    Some(CommandWords {
      text: "Error during compaction: API Error".to_owned(),
      error: true
    })
  );
  assert_eq!(command_words(&failed, 2), None, "only what follows the point named");
  assert_eq!(
    command_words(
      &[
        json!({ "type": "user", "message": { "content": "a person quoting <local-command-stdout>x</local-command-stdout>" } })
      ],
      0
    ),
    None,
    "only a line that is nothing but the command's words"
  );
}

#[test]
fn a_command_s_closing_and_the_compacted_marker_are_read_after_the_point_named() {
  let caveat = json!({ "type": "user", "isMeta": true, "message": { "content": "<local-command-caveat>Run directly.</local-command-caveat>" } });
  let boundary =
    json!({ "type": "system", "subtype": "compact_boundary", "content": "Conversation compacted" });
  let word = json!({ "type": "user", "message": { "content": "/compact" } });

  assert!(!command_closing_since(std::slice::from_ref(&word), 0));
  assert!(command_closing_since(&[word.clone(), caveat.clone()], 0));
  assert!(!command_closing_since(&[caveat, word.clone()], 1));
  assert!(compacted_since(&[word.clone(), boundary.clone()], 0));
  assert!(!compacted_since(&[boundary, word], 1));
}

const NOTICE: &str = "<task-notification>\n<task-id>a60</task-id>\n<tool-use-id>toolu_1</tool-use-id>\n<output-file>/tmp/s/tasks/a60.output</output-file>\n<status>completed</status>\n<summary>Agent \"Split\" finished</summary>\n<result>Six sets made.\nAll owned.</result>\n</task-notification>";

fn queued(operation: &str, content: &str) -> Value {
  json!({ "type": "queue-operation", "operation": operation, "content": content })
}

/// read with notices apart, a round ends at its own turn's end, a notice queued after it is no word it handed
/// over, and the turn the program opens on that notice is no round's: it owes it nothing and takes nothing from
/// it; read joined, the same notice holds the round and adds its answer, as a job's turn reads it
#[test]
fn a_round_read_with_notices_apart_ends_at_its_own_turn_and_takes_nothing_from_a_notice_s_turn() {
  let answered = vec![words("[chat:1] the round"), response("a", "the answer"), end()];
  let queued_after = [answered.clone(), vec![queued("enqueue", NOTICE)]].concat();

  assert_eq!(read_round(&queued_after, Notices::Apart).state, RoundState::Ended);
  assert_eq!(turns_owed(&queued_after, Notices::Apart).queued, 0);
  assert_eq!(read_round(&queued_after, Notices::Joined).state, RoundState::Working);

  let notice_turn = [
    queued_after.clone(),
    vec![queued("dequeue", ""), words(NOTICE), response("b", "noted"), end()],
  ]
  .concat();
  let apart = read_round(&notice_turn, Notices::Apart);
  let joined = read_round(&notice_turn, Notices::Joined);

  assert_eq!(
    (apart.state, apart.text.as_str(), apart.answers.len()),
    (RoundState::Ended, "the answer", 1)
  );
  assert_eq!((joined.state, joined.text.as_str()), (RoundState::Ended, "the answer\n\nnoted"));
}

/// read with notices apart, a round whose words went in while a notice's turn worked waits for the turn its
/// words were taken into, and answers with that turn alone
#[test]
fn a_round_whose_words_wait_behind_a_notice_s_turn_ends_with_the_turn_they_went_into() {
  let behind = vec![queued("enqueue", "[chat:2] the round"), words(NOTICE), response("a", "noted"), end()];

  assert_eq!(turns_owed(&behind, Notices::Apart).queued, 1);
  assert_eq!(read_round(&behind, Notices::Apart).state, RoundState::Working);

  let taken = [
    behind,
    vec![queued("dequeue", ""), words("[chat:2] the round"), response("b", "the answer"), end()],
  ]
  .concat();
  let round = read_round(&taken, Notices::Apart);

  assert_eq!((round.state, round.text.as_str()), (RoundState::Ended, "the answer"));
}

/// the foreground rests, read off the record, where its last line of the conversation is a turn's end, a notice
/// queued or taken up inside a turn passed over, and no word handed over waits in the queue
#[test]
fn the_foreground_rests_on_a_turn_s_end_whatever_notices_come_after_it() {
  let ended = vec![words("the round"), response("a", "the answer"), end()];

  assert!(rests_apart(&ended));
  assert!(rests_apart(
    &[ended.clone(), vec![queued("enqueue", NOTICE), queued("remove", NOTICE)]].concat()
  ));
  assert!(!rests_apart(&[ended.clone(), vec![queued("enqueue", "more words")]].concat()));
  assert!(!rests_apart(&[ended, vec![words(NOTICE), step("b")]].concat()));
}

/// reads what a background task's notice says, and nothing from a prompt that is no notice
#[test]
fn a_task_notice_is_read_for_its_task_status_result_and_output() {
  let notice = task_notice(NOTICE).unwrap();

  assert_eq!(notice.task, "a60");
  assert_eq!(notice.call.as_deref(), Some("toolu_1"));
  assert_eq!(notice.status.as_deref(), Some("completed"));
  assert_eq!(notice.result.as_deref(), Some("Six sets made.\nAll owned."));
  assert_eq!(notice.output.as_deref(), Some("/tmp/s/tasks/a60.output"));
  assert!(notice.ended());

  let resumed = task_notice(
    "<task-notification>\n<task-id>a60</task-id>\n<summary>resumed</summary>\n</task-notification>",
  );

  assert!(!resumed.unwrap().ended());
  assert!(!is_notice("the round") && task_notice("the round").is_none());
}

/// reads the shells a subagent's own record put in the background that no notice in it has ended
#[test]
fn a_subagent_s_background_shells_are_those_no_notice_ended() {
  let call = |id: &str, command: &str| {
    json!({ "type": "assistant", "isSidechain": true, "timestamp": "2026-11-17T13:30:00Z", "message": { "content": [
      { "type": "tool_use", "id": id, "name": "Bash", "input": { "command": command, "run_in_background": true } }
    ] } })
  };
  let launched = |id: &str, task: &str| {
    json!({ "type": "user", "isSidechain": true, "message": { "content": [
      { "type": "tool_result", "tool_use_id": id, "content": format!("Command running in background with ID: {task}") }
    ] } })
  };
  let left = shells_left(&[
    call("t1", "find / -name x"),
    launched("t1", "bfind"),
    call("t2", "while pgrep -f find; do sleep 5; done"),
    launched("t2", "bwait"),
    queued(
      "enqueue",
      "<task-notification>\n<task-id>bfind</task-id>\n<status>completed</status>\n</task-notification>",
    ),
  ]);

  assert_eq!(left.len(), 1);
  assert_eq!(
    (left[0].task.as_str(), left[0].command.as_str()),
    ("bwait", "while pgrep -f find; do sleep 5; done")
  );
}

// What a reader holds of a record's lines before `from`, written out: the marks it keeps there.
fn held_before(lines: &[Value], from: usize) -> usize {
  lines[..from]
    .iter()
    .filter(|line| !line.is_null())
    .map(|line| line.to_string().len())
    .sum()
}

#[test]
fn a_reader_for_each_of_many_turns_on_a_growing_session_holds_its_turn_and_small_marks_before_it() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("s.jsonl");
  let asked = "w".repeat(4000);
  let long = "x".repeat(2000);
  let notice = format!("<task-notification><task-id>t1</task-id>{asked}</task-notification>");

  // A long supervisor session: every round its words queued and taken, a long answer and its end.
  for round in 0..300 {
    appended(
      &path,
      &[
        queued("enqueue", &asked),
        queued("dequeue", ""),
        words(&format!("prompt {round}")),
        response(&format!("r{round}"), &long),
        end(),
        account(100 + round as i64),
      ],
    );
  }

  // A notice queued before the turns, which keeps one entry standing in the queue through all of them.
  appended(&path, &[queued("enqueue", &notice)]);

  let mut first_marks = 0;

  for turn in 0..30 {
    let mut tail = Tail::at_end(&path);
    let from = tail.count();
    let on_disk = std::fs::metadata(&path).unwrap().len() as usize;
    let marks = held_before(tail.lines(), from);

    if turn == 0 {
      first_marks = marks;
    }

    assert!(
      marks * 20 < on_disk,
      "turn {turn}: {marks} bytes held before the turn of {on_disk} written"
    );
    assert!(
      marks <= first_marks + turn * 200,
      "turn {turn}: what is held before the turn grows by the marks of the turns since, no more"
    );

    appended(
      &path,
      &[
        queued("enqueue", &asked),
        queued("dequeue", ""),
        words(&format!("turn {turn}")),
        response(&format!("n{turn}"), &format!("answer {turn}")),
        account(5000 + turn as i64),
        end(),
      ],
    );

    let records = tail.look();
    let count = records.len();

    assert_eq!(count, from + 6);
    assert_eq!(read_round(&records[from..], Notices::Apart).text, format!("answer {turn}"));
    assert_eq!(turns_owed(records, Notices::Joined).queued, 1, "one entry still stands queued");

    // What the whole record says is read the same off what the reader keeps, at the first turn and the last.
    if turn == 0 || turn == 29 {
      let mut whole = Tail::new(&path);

      assert_eq!(turns_owed(whole.look(), Notices::Joined).queued, 1);
      assert_eq!(
        turns_owed(records, Notices::Apart).queued,
        turns_owed(whole.look(), Notices::Apart).queued,
        "a queued notice is told from queued words off its mark alone"
      );
      assert_eq!(rests_apart(records), rests_apart(whole.look()));
      assert_eq!(ends_on_turn_end(records), ends_on_turn_end(whole.look()));
    }

    tail.forget_raw_before(count);
    assert_eq!(tail.held().raw_bytes, 0);
    assert!(tail.held().records <= 6 + 4 + 2 * (300 + turn) + 1);
  }
}

#[test]
fn a_reader_from_a_line_found_a_line_at_a_time_holds_only_the_lines_from_there_across_many_looks() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("s.jsonl");

  appended(&path, &history(300));
  appended(&path, &[words("the turn picked up")]);

  // Where the turn begins is found as the pickup finds it, a line at a time.
  let mut begins = None;

  each_line(&path, |at, record| {
    if record.pointer("/message/content").and_then(Value::as_str) == Some("the turn picked up") {
      begins = Some(at);
    }
  });

  let begins = begins.unwrap();
  let mut tail = Tail::from_line(&path, begins);

  assert_eq!(begins, 900);
  assert_eq!(tail.count(), 901);
  assert!(tail.held().records <= 2 + 1, "the last end before the turn, and the turn's own line");

  for look in 0..200 {
    appended(&path, &[step(&format!("s{look}"))]);

    let records = tail.look();

    assert_eq!(records.len(), 902 + look);
    assert!(prompted_since(records, begins));
    tail.forget_raw_before(902 + look);
    assert!(tail.held().records <= 2 + 1 + look + 1);
    assert_eq!(tail.held().raw_bytes, 0);
  }
}
