//! The status-line command and the hooks of an agent's resident program as `bmo-agency agent-status`
//! answers them (docs/agent-control.md, 「What a resident turn reads」): what each call writes into the
//! place, the status line, the holds a stop is given while its background work runs or its answer uses a
//! banned word, a turn's end left open while the round owes a turn, and a person's typed line answered with
//! the service's composition, and the shell hook's answer to a command: its standard input closed, and a
//! Claude launch refused where the line asks. Each call is the verb's own function, run on a place of the
//! test's own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use agency_server::door::place_run::{
  BackgroundLine, begin_run, end_run, hand_run, person_submits, read_background, read_hooks, read_run,
  read_status,
};
use agency_server::verbs::agent_status::{
  Asked, CLOSED_INPUT, Ground, Refuses, agent_status, shell_guard, typed_hook_output, typed_submission,
};
use agency_server::wording::{WORDING_DOCUMENT, parse_glossary};
use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tokio::net::TcpListener;

const SESSION: &str = "7b357775-bf0e-4e3a-8b77-823abe359a4d";

// The wording document of the code tree a hook reads, holding one banned term a line can carry alone.
const GLOSSARY: &str = "# Wording\n\n## Banned terms\n\n- en · utilise → use\n";

struct Hook {
  place: tempfile::TempDir,
  config: tempfile::TempDir,
  tree: tempfile::TempDir,
  env: BTreeMap<String, String>,
}

impl Hook {
  fn new() -> Self {
    let tree = tempfile::tempdir().unwrap();

    std::fs::write(tree.path().join(WORDING_DOCUMENT), GLOSSARY).unwrap();

    Self {
      place: tempfile::tempdir().unwrap(),
      config: tempfile::tempdir().unwrap(),
      tree,
      env: BTreeMap::new(),
    }
  }

  fn path(&self) -> &Path {
    self.place.path()
  }

  // One call of the hook with its flags, handed `payload`, at the moment named.
  async fn call(&self, kind: &str, flags: &[&str], payload: &Value) -> Vec<String> {
    let asked = Asked {
      kind: kind.to_owned(),
      place: self.path().to_string_lossy().into_owned(),
      hold_tasks: flags.contains(&"--hold-tasks"),
      hold_text_tool_call: flags.contains(&"--hold-text-tool-call"),
      hold_wording: flags.contains(&"--hold-wording"),
      compose_typed: flags.contains(&"--compose-typed"),
      carry_background: flags.contains(&"--carry-background"),
      refuses: Refuses {
        claude: flags.contains(&"--refuse-claude"),
        job: flags.contains(&"--guard-job"),
      },
    };
    let ground = Ground {
      env: &self.env,
      code_tree: self.tree.path(),
      machine_config_dir: self.config.path(),
      now_ms: chrono::Utc::now().timestamp_millis(),
    };

    agent_status(&asked, &payload.to_string(), &ground).await
  }
}

fn now() -> i64 {
  chrono::Utc::now().timestamp_millis()
}

fn at(ms: i64) -> String {
  chrono::DateTime::from_timestamp_millis(ms).unwrap().to_rfc3339()
}

fn transcript(dir: &Path, lines: &[Value]) -> PathBuf {
  let path = dir.join("session.jsonl");

  std::fs::write(&path, lines.iter().map(|line| format!("{line}\n")).collect::<String>()).unwrap();
  path
}

/// the script records what it is handed and answers the status line
#[tokio::test]
async fn the_hook_records_what_it_is_handed_and_answers_the_status_line() {
  let hook = Hook::new();

  begin_run(hook.path(), "worker-job-7", "job seven", now(), BTreeMap::new()).unwrap();

  let said = hook
    .call("status", &[], &json!({ "session_id": "s", "cost": { "total_cost_usd": 0.1 } }))
    .await;

  assert_eq!(said.len(), 1);
  assert!(said[0].contains("worker-job-7") && said[0].contains("job seven"), "{said:?}");
  assert_eq!(read_status(hook.path(), 0).len(), 1);
  assert!(read_status(hook.path(), 0)[0].login.is_some());

  assert!(hook.call("stop", &[], &json!({})).await.is_empty());
  assert!(read_run(hook.path()).unwrap().ended_at.is_some());
}

/// a line that asks for the hold has a stop with its background work running answered with a block
#[tokio::test]
async fn a_stop_with_its_background_work_running_is_answered_with_a_block_three_times_then_let_through() {
  let hook = Hook::new();
  let running = json!({ "background_tasks": [
    { "id": "b1", "type": "shell", "status": "running", "description": "dev server" },
    { "id": "b2", "type": "shell", "status": "completed" }
  ] });

  begin_run(hook.path(), "worker-job-8", "job eight", now() - 1_000, BTreeMap::new()).unwrap();

  for turn in 1..=3 {
    let said = hook.call("stop", &["--hold-tasks"], &running).await;
    let block: Value = serde_json::from_str(&said[0]).unwrap();

    assert_eq!(block["decision"], "block", "hold {turn}");
    assert!(block["reason"].as_str().unwrap().contains("b1 (dev server)"));
    assert!(!block["reason"].as_str().unwrap().contains("b2"));
    assert!(read_run(hook.path()).unwrap().ended_at.is_none());
  }

  assert_eq!(
    read_hooks(hook.path(), 0).iter().map(|hook| hook.kind.as_str()).collect::<Vec<_>>(),
    ["held"; 3]
  );
  assert!(hook.call("stop", &["--hold-tasks"], &running).await.is_empty());
  assert!(read_run(hook.path()).unwrap().ended_at.is_some());
}

// The first term the glossary sets aside that a line can carry alone, and what it is to be written as.
fn banned() -> (String, String) {
  parse_glossary(GLOSSARY)
    .unwrap()
    .into_iter()
    .find(|term| term.keeps.is_empty() && !term.term.contains(' '))
    .map(|term| (term.term, term.instead))
    .expect("a banned term in the glossary")
}

/// a line that holds its wording has a stop whose answer uses a banned word answered with a block
#[tokio::test]
async fn a_stop_whose_answer_uses_a_banned_word_is_answered_with_a_block_and_an_answer_written_again_passes()
 {
  let hook = Hook::new();
  let (term, instead) = banned();
  let started = now() - 1_000;

  begin_run(hook.path(), "worker-job-9", "job nine", started, BTreeMap::new()).unwrap();

  let handed = hook
    .call(
      "stop",
      &["--hold-tasks", "--hold-wording"],
      &json!({ "last_assistant_message": format!("{term}.") }),
    )
    .await;
  let block: Value = serde_json::from_str(&handed[0]).unwrap();

  assert_eq!(block["decision"], "block");
  assert!(block["reason"].as_str().unwrap().contains(&format!("\"{term}\" → {instead}")));

  // Without the program's own last message, the answer is read off the session record.
  let record = transcript(
    hook.place.path(),
    &[json!({ "type": "assistant", "timestamp": at(started + 10),
      "message": { "id": "m1", "content": [{ "type": "text", "text": format!("{term}.") }] } })],
  );
  let read = hook.call("stop", &["--hold-wording"], &json!({ "transcript_path": record })).await;

  assert!(
    serde_json::from_str::<Value>(&read[0]).unwrap()["reason"]
      .as_str()
      .unwrap()
      .contains(&term)
  );

  assert!(
    hook
      .call("stop", &["--hold-wording"], &json!({ "last_assistant_message": format!("{instead}.") }))
      .await
      .is_empty()
  );
  assert!(read_run(hook.path()).unwrap().ended_at.is_some());
}

/// a line that holds a text-form tool call has such an answer answered with a block, and an ordinary report passes
#[tokio::test]
async fn a_stop_whose_answer_is_a_tool_call_written_as_text_is_answered_with_a_block_and_a_report_passes() {
  let hook = Hook::new();
  let raw = "<tool_call><function=Read><parameter=file_path>/tmp/one</parameter></function></tool_call>";

  begin_run(hook.path(), "worker-job-11", "job eleven", now() - 1_000, BTreeMap::new()).unwrap();

  let handed = hook
    .call(
      "stop",
      &["--hold-tasks", "--hold-text-tool-call"],
      &json!({ "last_assistant_message": raw }),
    )
    .await;
  let block: Value = serde_json::from_str(&handed[0]).unwrap();

  assert_eq!(block["decision"], "block");
  assert!(block["reason"].as_str().unwrap().contains("native tool call"), "{}", block["reason"]);
  assert!(read_run(hook.path()).unwrap().ended_at.is_none(), "a held stop ends nothing");

  assert!(
    hook
      .call(
        "stop",
        &["--hold-text-tool-call"],
        &json!({ "last_assistant_message": "The work is done; the report stands." }),
      )
      .await
      .is_empty()
  );
  assert!(read_run(hook.path()).unwrap().ended_at.is_some());
}

/// the hold lets through a stop with nothing running, and a person's turn
#[tokio::test]
async fn the_hold_lets_through_a_stop_with_nothing_running_and_a_persons_turn() {
  let hook = Hook::new();
  let let_through = |payload: Value| {
    let hook = &hook;

    async move { hook.call("stop", &["--hold-tasks"], &payload).await }
  };

  begin_run(hook.path(), "worker-job-10", "job ten", now() - 1_000, BTreeMap::new()).unwrap();
  assert!(let_through(json!({ "background_tasks": [] })).await.is_empty());
  assert!(read_run(hook.path()).unwrap().ended_at.is_some());

  hook.call("submit", &[], &json!({})).await;
  assert!(read_run(hook.path()).unwrap().person);
  assert!(
    let_through(json!({ "background_tasks": [{ "id": "b", "status": "running" }] }))
      .await
      .is_empty()
  );
  assert_eq!(read_hooks(hook.path(), 0).last().unwrap().kind, "stop");
}

/// a turn's end leaves the room's run open while the round still owes a turn
#[tokio::test]
async fn a_turns_end_leaves_the_bots_run_open_while_the_round_still_owes_a_turn() {
  let hook = Hook::new();
  let started = now() - 1_000;
  let record = transcript(
    hook.place.path(),
    &[
      json!({ "type": "user", "timestamp": at(started + 10), "message": { "content": "the work" } }),
      json!({ "type": "queue-operation", "operation": "enqueue", "timestamp": at(started + 20) }),
      json!({ "type": "queue-operation", "operation": "dequeue", "timestamp": at(started + 30) }),
      json!({ "type": "system", "subtype": "turn_duration", "timestamp": at(started + 40) }),
    ],
  );
  let stop = json!({ "session_id": "s1", "transcript_path": record });

  begin_run(hook.path(), "chat:9", "chat", started, BTreeMap::new()).unwrap();
  hook.call("stop", &[], &stop).await;
  assert!(read_run(hook.path()).unwrap().ended_at.is_none());

  hook.call("stop", &[], &stop).await;
  assert!(read_run(hook.path()).unwrap().ended_at.is_some());
}

/// a failed request's end is a turn's end: counted against what the round owes, never held, and closing the run
#[tokio::test]
async fn a_failed_requests_end_is_a_turns_end_counted_and_never_held() {
  let hook = Hook::new();
  let started = now() - 1_000;
  let record = transcript(
    hook.place.path(),
    &[
      json!({ "type": "user", "timestamp": at(started + 10), "message": { "content": "the words" } }),
      json!({ "type": "queue-operation", "operation": "enqueue", "timestamp": at(started + 20) }),
      json!({ "type": "queue-operation", "operation": "dequeue", "timestamp": at(started + 30) }),
      json!({ "type": "system", "subtype": "turn_duration", "timestamp": at(started + 40) }),
    ],
  );
  let failed = json!({
    "session_id": "s1",
    "transcript_path": record,
    "hook_event_name": "StopFailure",
    "error": "server_error",
    "last_assistant_message": "API Error: 503 card busy",
    "background_tasks": [{ "id": "b", "status": "running" }],
  });

  begin_run(hook.path(), "chat:9", "chat", started, BTreeMap::new()).unwrap();

  // The first of the round's two turns failed: the run stays open for the second.
  assert!(hook.call("failure", &[], &failed).await.is_empty());
  assert!(read_run(hook.path()).unwrap().ended_at.is_none());

  // The second failed too, with work still running, and is let through whatever the hook was asked to hold.
  assert!(hook.call("failure", &["--hold-tasks"], &failed).await.is_empty());
  assert!(read_run(hook.path()).unwrap().ended_at.is_some());
  assert_eq!(read_hooks(hook.path(), 0).last().unwrap().kind, "failure");
}

// A service answering `/turn-contexts` with a composition, writing down what it was asked and with what token.
async fn composing() -> (u16, Arc<Mutex<Vec<(Option<String>, Value)>>>) {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let asked = Arc::new(Mutex::new(vec![]));
  let heard = asked.clone();
  let app = Router::new().fallback(move |request: Request| {
    let heard = heard.clone();

    async move {
      let token = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
      let body = request.into_body().collect().await.unwrap().to_bytes();

      heard.lock().unwrap().push((token, serde_json::from_slice(&body).unwrap()));
      axum::Json(json!({ "context": "<round of the owner's chat>" }))
    }
  });

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (port, asked)
}

// A service standing only where a composition is answered: an ask put anywhere else is 404, as a route that
// is not there answers it, and its path is written down with the ask.
async fn composing_only(at: &str) -> (u16, Arc<Mutex<Vec<(String, Value)>>>) {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let heard = Arc::new(Mutex::new(vec![]));
  let seen = heard.clone();
  let wanted = at.to_owned();

  let app = Router::new().fallback(move |request: Request| {
    let seen = seen.clone();
    let wanted = wanted.clone();

    async move {
      let path = request.uri().path().to_string();
      let body = request.into_body().collect().await.unwrap().to_bytes();
      let answering = path == wanted;

      seen.lock().unwrap().push((path, serde_json::from_slice(&body).unwrap()));

      if answering {
        (StatusCode::OK, axum::Json(json!({ "context": "<round of the owner's chat>" })))
      } else {
        (StatusCode::NOT_FOUND, axum::Json(json!({ "error": "not-found" })))
      }
    }
  });

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (port, heard)
}

/// the submit hook asks for a typed line at the route that answers compositions, not at the point's name
#[tokio::test]
async fn a_typed_line_asked_at_the_route_answers_with_the_composition_and_not_the_missing_note() {
  let (port, heard) = composing_only("/turn-contexts").await;
  let mut hook = Hook::new();

  hook.env.insert("AGENCY_CONTROL_PORT".into(), port.to_string());
  hook.env.insert("AGENCY_CONTROL_TOKEN".into(), "typed-token".into());

  let line = json!({ "prompt": "sort out today's plan", "session_id": SESSION });

  begin_run(hook.path(), "chat:9", "chat", now() - 1_000, BTreeMap::new()).unwrap();
  assert!(hook.call("submit", &["--compose-typed"], &line).await.is_empty());
  end_run(hook.path(), now()).unwrap();

  let said = hook.call("submit", &["--compose-typed"], &line).await;

  assert_eq!(
    serde_json::from_str::<Value>(&said[0]).unwrap()["hookSpecificOutput"]["additionalContext"],
    json!("<round of the owner's chat>")
  );
  assert_eq!(
    heard.lock().unwrap().iter().map(|(path, _)| path.to_owned()).collect::<Vec<String>>(),
    ["/turn-contexts".to_owned()],
    "the ask reaches the route that answers compositions: nothing else was asked"
  );
}

/// a line that composes typed lines hands a person's line the service's composition, and nothing else
#[tokio::test]
async fn a_persons_typed_line_is_handed_the_services_composition_and_nothing_else_is() {
  let (port, asked) = composing().await;
  let mut hook = Hook::new();

  hook.env.insert("AGENCY_CONTROL_PORT".into(), port.to_string());
  hook.env.insert("AGENCY_CONTROL_TOKEN".into(), "typed-token".into());

  let line = json!({ "prompt": "sort out today's plan", "session_id": SESSION });

  begin_run(hook.path(), "chat:9", "chat", now() - 1_000, BTreeMap::new()).unwrap();
  assert!(hook.call("submit", &["--compose-typed"], &line).await.is_empty());
  end_run(hook.path(), now()).unwrap();

  assert!(
    hook
      .call("submit", &["--compose-typed"], &json!({ "prompt": "/compact", "session_id": SESSION }))
      .await
      .is_empty()
  );
  hook.call("stop", &[], &json!({})).await;

  let said = hook.call("submit", &["--compose-typed"], &line).await;

  assert_eq!(
    serde_json::from_str::<Value>(&said[0]).unwrap(),
    json!({ "hookSpecificOutput": { "hookEventName": "UserPromptSubmit", "additionalContext": "<round of the owner's chat>" } })
  );
  assert_eq!(
    *asked.lock().unwrap(),
    [(
      Some("Bearer typed-token".to_owned()),
      json!({ "text": "sort out today's plan", "sessionId": SESSION })
    )]
  );
  assert!(read_run(hook.path()).unwrap().person);
}

/// a word the service pushed that lands after the turn's end is the round's, not a typed line
#[tokio::test]
async fn a_word_the_service_pushed_that_lands_after_the_turns_end_is_the_rounds_not_a_typed_line() {
  let (port, asked) = composing().await;
  let mut hook = Hook::new();

  hook.env.insert("AGENCY_CONTROL_PORT".into(), port.to_string());
  hook.env.insert("AGENCY_CONTROL_TOKEN".into(), "typed-token".into());

  let env = BTreeMap::from([("BMO_RUN_NAME".to_owned(), "chat:-9:1:174".to_owned())]);

  begin_run(hook.path(), "chat:-9:1:174", "chat", now() - 1_000, env).unwrap();
  hook.call("stop", &["--compose-typed"], &json!({ "session_id": SESSION })).await;
  assert!(read_run(hook.path()).unwrap().ended_at.is_some());

  hand_run(hook.path(), true);
  assert!(
    hook
      .call(
        "submit",
        &["--compose-typed"],
        &json!({ "prompt": "More for the answer you are making now", "session_id": SESSION })
      )
      .await
      .is_empty()
  );
  assert!(asked.lock().unwrap().is_empty());

  let run = read_run(hook.path()).unwrap();

  assert_eq!((run.name.as_deref(), run.person, run.ended_at), (Some("chat:-9:1:174"), false, None));

  // The submit took the mark away, so once that turn has ended the next line is judged afresh.
  hook.call("stop", &["--compose-typed"], &json!({ "session_id": SESSION })).await;
  assert!(person_submits(hook.path()));
}

/// is handed over with its session, and an empty line, a slash command or no session is left alone
#[test]
fn a_typed_line_is_handed_over_with_its_session_and_an_empty_line_a_slash_command_or_no_session_is_left_alone()
 {
  assert_eq!(
    typed_submission(&json!({ "prompt": "sort it out", "session_id": SESSION })),
    Some(("sort it out".to_owned(), SESSION.to_owned()))
  );
  assert_eq!(typed_submission(&json!({ "prompt": "  ", "session_id": SESSION })), None);
  assert_eq!(typed_submission(&json!({ "prompt": " /compact", "session_id": SESSION })), None);
  assert_eq!(typed_submission(&json!({ "prompt": "plan", "session_id": "" })), None);
  assert_eq!(typed_submission(&json!("not json")), None);
}

// What a supervisor's program has its shell hook refuse.
const SUPERVISOR_S: Refuses = Refuses {
  claude: true,
  job: false,
};

// The shell hook's answer to a Bash call on a supervisor's program, which refuses a Claude launch: the
// reason it denies one, or none for a call let through.
fn shell_refusal(command: &str) -> Option<String> {
  let answer = shell_guard(&bash_call(command), SUPERVISOR_S).unwrap();
  let answer: Value = serde_json::from_str(&answer).unwrap();

  answer["hookSpecificOutput"]["permissionDecisionReason"].as_str().map(str::to_owned)
}

// A Bash call as the program hands it to the hook.
fn bash_call(command: &str) -> String {
  json!({ "tool_name": "Bash", "tool_input": {
    "command": command, "description": "what it does", "timeout": 600_000, "run_in_background": false,
  } })
  .to_string()
}

// The call the shell hook hands the program back in place of the one it was handed.
fn rewritten(command: &str, refuse_claude: bool) -> Value {
  let refuses = Refuses {
    claude: refuse_claude,
    job: false,
  };
  let answer: Value = serde_json::from_str(&shell_guard(&bash_call(command), refuses).unwrap()).unwrap();

  assert_eq!(answer["hookSpecificOutput"]["hookEventName"], "PreToolUse");
  assert!(answer["hookSpecificOutput"].get("permissionDecision").is_none(), "{answer}");

  answer["hookSpecificOutput"]["updatedInput"].clone()
}

/// hands every shell call back whole with its command behind the line that closes the shell's standard
/// input, on every program, and a Claude launch too where the line does not refuse one
#[test]
fn the_shell_hook_closes_every_command_s_standard_input_and_keeps_the_rest_of_the_call() {
  let command = r#"rg -n '"<tool_call>"|<function=' -g '*.rs' | head"#;
  let call = rewritten(command, false);

  assert_eq!(call["command"], format!("{CLOSED_INPUT}\n{command}"));
  assert_eq!(call["description"], "what it does");
  assert_eq!(call["timeout"], 600_000);
  assert_eq!(call["run_in_background"], false);
  assert_eq!(
    rewritten(&format!("{CLOSED_INPUT}\nls ."), false)["command"],
    format!("{CLOSED_INPUT}\nls .")
  );
  assert_eq!(rewritten("claude -p hi", false)["command"], format!("{CLOSED_INPUT}\nclaude -p hi"));
  assert_eq!(rewritten("ls -la .", true)["command"], format!("{CLOSED_INPUT}\nls -la ."));
}

/// a reader given nothing to read ends at once under the rewritten command, though the shell itself stands
/// on an input nothing ever writes to or closes, as the program's shell does where it reads a redirection
#[test]
fn a_reader_named_nothing_ends_under_the_rewritten_command_while_its_shell_s_input_stays_open() {
  let command = rewritten("cat; echo ended", false)["command"].as_str().unwrap().to_owned();
  let mut shell = std::process::Command::new("bash")
    .args(["-c", &command])
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .spawn()
    .unwrap();
  // Held, never written to and never closed, until the shell has ended.
  let input = shell.stdin.take();
  let ended = (0..200).find_map(|_| {
    let done = shell.try_wait().unwrap();

    if done.is_none() {
      std::thread::sleep(std::time::Duration::from_millis(10));
    }

    done
  });

  if ended.is_none() {
    let _ = shell.kill();
  }

  let output = shell.wait_with_output().unwrap();

  drop(input);
  assert!(ended.is_some_and(|status| status.success()), "the reader waited on the open input");
  assert_eq!(String::from_utf8_lossy(&output.stdout), "ended\n");
}

/// refuses a shell command that starts Claude Code, and lets every other through
#[test]
fn the_shell_guard_refuses_a_shell_command_that_starts_claude_code_and_lets_every_other_through() {
  for command in ["claude -p hi", "env A=1 claude", "bash -c 'claude --resume x'"] {
    let reason = shell_refusal(command).unwrap_or_else(|| panic!("let through: {command}"));

    assert!(reason.starts_with("refused: it starts the Claude Code CLI"), "{reason}");
    assert!(
      reason.contains("file it as an item with file_item and launch it with assign_worker and run_job"),
      "{reason}"
    );
  }

  for command in ["ls ~/.claude", "echo claude", "git log"] {
    assert_eq!(shell_refusal(command), None, "{command}");
  }

  assert_eq!(
    shell_guard(
      &json!({ "tool_name": "Read", "tool_input": { "file_path": "/x" } }).to_string(),
      SUPERVISOR_S
    ),
    None
  );
  assert_eq!(shell_guard("not json", SUPERVISOR_S), None);
}

// A call of `tool` as the program hands it to the hook, asked to run in the background or not.
fn call_of(tool: &str, background: bool) -> String {
  let input = match tool {
    "Bash" => json!({ "command": "sleep 900", "description": "wait", "run_in_background": background }),
    _ => {
      json!({
        "description": "find it", "prompt": "find it", "subagent_type": "general-purpose",
        "run_in_background": background,
      })
    }
  };

  json!({ "tool_name": tool, "tool_input": input }).to_string()
}

/// lets a supervisor's program put a shell call or a subagent in its own background: the shell's command is
/// handed back with its input closed as any other, and a subagent goes ahead untouched
#[test]
fn a_supervisor_s_background_shell_or_subagent_goes_ahead() {
  let shell: Value =
    serde_json::from_str(&shell_guard(&call_of("Bash", true), SUPERVISOR_S).unwrap()).unwrap();

  assert!(shell["hookSpecificOutput"].get("permissionDecision").is_none(), "{shell}");
  assert_eq!(shell["hookSpecificOutput"]["updatedInput"]["run_in_background"], true);
  assert_eq!(
    shell["hookSpecificOutput"]["updatedInput"]["command"],
    format!("{CLOSED_INPUT}\nsleep 900")
  );

  for tool in ["Agent", "Task"] {
    assert_eq!(shell_guard(&call_of(tool, true), SUPERVISOR_S), None, "{tool}");
  }
}

// A service that refuses every line.
async fn refusing() -> u16 {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let app = Router::new().fallback(|| async {
    (
      StatusCode::NOT_IMPLEMENTED,
      axum::Json(json!({ "error": "no consumer composes a typed line" })),
    )
  });

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  port
}

/// goes on with a line saying why where the service cannot compose it
#[tokio::test]
async fn a_typed_line_goes_on_with_a_line_saying_why_where_the_service_cannot_compose_it() {
  let context = |printed: String| {
    let said: Value = serde_json::from_str(&printed).unwrap();

    assert_eq!(said["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit");
    said["hookSpecificOutput"]["additionalContext"].as_str().unwrap().to_owned()
  };
  let refused = refusing().await;
  // A port nothing listens on: one bound and let go.
  let gone = TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap().port();

  assert!(
    context(typed_hook_output("plan", SESSION, gone, None).await)
      .contains("could not be composed (the program has no AGENCY_CONTROL_TOKEN to ask agency with)")
  );
  assert!(
    context(typed_hook_output("plan", SESSION, refused, Some("t")).await)
      .contains("(agency answered 501: no consumer composes a typed line)")
  );
  assert!(
    context(typed_hook_output("plan", SESSION, gone, Some("t")).await).ends_with("so it goes on as typed.")
  );
}

/// lets through a shell command that reaches a unit's port, its store or its token files: the machine is the
/// owner's and the agent acts on it with his hands (docs/agents.md, 「A supervisor's shell reaches the whole
/// machine」)
#[test]
fn the_shell_guard_lets_a_command_reach_a_unit_s_port_its_store_and_its_token_files() {
  for command in [
    r#"psql "dbname=bmo_brain" -c "select 1""#,
    "sqlite3 /tmp/x.db .tables",
    "curl -s http://127.0.0.1:13104/agents",
    "nc localhost 5432",
    "cat $BMO_DATA_DIR/env/agency.token",
    "cat ~/.local/share/bmo/data/env/job.token",
    "bmo use bmo/brain_list --subject person:90000001",
    "ls /tmp && echo done",
  ] {
    assert_eq!(shell_refusal(command), None, "{command}");
  }
}

/// a conversational agent's dsh shell asks the same guard in its start-up file and never runs a refused
/// command (docs/agent-control.md, the shell guard in dsh's shell start-up)
#[test]
fn a_dsh_shell_refuses_before_it_runs_and_runs_every_other_command() {
  let data = tempfile::tempdir().unwrap();
  let script = agency_server::lines::place::carry_script(data.path()).unwrap();
  let guard = crate::support::binary::path("bmo-agency");
  let marker = data.path().join("ran");
  let run = |command: &str| {
    std::process::Command::new("bash")
      .args(["-c", command])
      .env_clear()
      .env("PATH", "/usr/bin:/bin")
      .env("BASH_ENV", &script)
      .env("BMO_DATA_DIR", data.path())
      .env(agency_server::lines::place::SHELL_GUARD_VARIABLE, &guard)
      .stdin(std::process::Stdio::null())
      .output()
      .unwrap()
  };

  let refused = run(&format!("touch {} ; claude -p hi", marker.display()));

  assert_eq!(refused.status.code(), Some(126));
  assert!(String::from_utf8_lossy(&refused.stderr).contains("refused: it starts the Claude Code CLI"));
  assert!(!marker.exists(), "nothing of a refused command runs");

  let allowed = run("echo fine");

  assert_eq!(String::from_utf8_lossy(&allowed.stdout), "fine\n");
  assert_eq!(allowed.status.code(), Some(0));
}

const NOTICE: &str = "<task-notification>\n<task-id>a60</task-id>\n<status>completed</status>\n<result>Six sets.</result>\n</task-notification>";

/// on a line that carries its background apart, a stop writes each task it names running under the room of the
/// run standing in the place, once; the notice's submit is written beside it and is the program's own turn,
/// opening no person's turn and composing nothing; and the run closes at the stop with the work still running
#[tokio::test]
async fn a_supervisor_s_stop_writes_its_background_under_the_room_and_the_notice_opens_no_person_s_turn() {
  let hook = Hook::new();
  let carry = ["--carry-background", "--compose-typed"];
  let env = BTreeMap::from([
    ("BMO_TURN_CHAT".to_owned(), "-500".to_owned()),
    ("BMO_TURN_AGENT".to_owned(), "bmo".to_owned()),
  ]);

  begin_run(hook.path(), "chat:-500:1", "a turn in a room", now(), env).unwrap();

  let stop = json!({
    "session_id": "s", "transcript_path": "/records/s.jsonl",
    "background_tasks": [
      { "id": "a60", "type": "subagent", "status": "running", "description": "Split the stickers" },
      { "id": "b1", "type": "shell", "status": "completed", "description": "done before", "command": "ls" },
    ],
  });

  assert!(
    hook.call("stop", &carry, &stop).await.is_empty(),
    "a stop with work running is not held"
  );
  assert!(read_run(hook.path()).unwrap().ended_at.is_some(), "the run closes at its stop");
  hook.call("stop", &carry, &stop).await;

  let notice = json!({ "session_id": "s", "prompt": NOTICE });

  assert!(hook.call("submit", &carry, &notice).await.is_empty(), "a notice is composed nothing");

  let run = read_run(hook.path()).unwrap();

  assert!(
    !run.person && run.name.as_deref() == Some("chat:-500:1"),
    "the notice opened no person's turn: {run:?}"
  );

  let lines = read_background(hook.path());

  assert_eq!(lines.len(), 2, "{lines:?}");

  let BackgroundLine::Launched(launch) = &lines[0] else { panic!("{lines:?}") };

  assert_eq!(
    (
      launch.task.as_str(),
      launch.kind.as_str(),
      launch.room.as_deref(),
      launch.agent.as_deref()
    ),
    ("a60", "subagent", Some("-500"), Some("bmo"))
  );
  assert_eq!(launch.transcript.as_deref(), Some("/records/s.jsonl"));
  assert!(
    matches!(&lines[1], BackgroundLine::Notice { notice, .. } if notice.task == "a60" && notice.ended())
  );
}

/// a job's line carries nothing apart: neither its stop nor a notice's submit writes a background record
#[tokio::test]
async fn a_job_s_stop_writes_no_background_record() {
  let hook = Hook::new();

  begin_run(hook.path(), "worker-job-9", "a job", now(), BTreeMap::new()).unwrap();
  hook
    .call(
      "stop",
      &[],
      &json!({ "background_tasks": [{ "id": "a60", "type": "subagent", "status": "running" }] }),
    )
    .await;
  hook.call("submit", &[], &json!({ "session_id": "s", "prompt": NOTICE })).await;

  assert!(read_background(hook.path()).is_empty());
}
