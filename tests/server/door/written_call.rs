//! Whether a final answer is a tool call written as text (docs/agent-control.md, 「What a resident turn
//! reads」): broad over the shapes a broken call takes, and blind to what an answer quotes as code.

use agency_server::door::written_call::text_tool_call;

// The final answers two jobs on a registered model ended on: a `<tool_call>` block holding a template's
// control token where the call's name belongs, and a bare `<invoke>` naming no real tool.
const TOOL_CALL_BLOCK: &str = "<tool_call>\n<|im_start|>function>\n</function>\n</tool_call>";
const BARE_INVOKE: &str = "<invoke name=\"function\">\n</function>";

#[test]
fn the_answers_jobs_ended_on_are_calls_written_as_text() {
  assert!(text_tool_call(TOOL_CALL_BLOCK));
  assert!(text_tool_call(&format!("\n\n{TOOL_CALL_BLOCK}")));
  assert!(text_tool_call(BARE_INVOKE));
  assert!(text_tool_call(&format!("Reading the file now.\n\n{BARE_INVOKE}")));
}

#[test]
fn any_call_markup_or_control_token_outside_quoted_code_makes_a_call_written_as_text() {
  for answer in [
    "<function=Read>\n<parameter=file_path>/tmp/one</parameter>\n</function>",
    "<function_calls>\n<invoke name=\"Bash\">\n<parameter name=\"command\">ls</parameter>\n</invoke>",
    "Done.</invoke>",
    "<invoke/>",
    "<tool_calls>[]</tool_calls>",
    "Then the call:\n<parameter name=\"x\">1",
    "The report.<|im_end|>",
    "<\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}>",
    "    <tool_call>\n    {\"name\": \"Read\"}",
    "{\"name\": \"Read\", \"arguments\": {\"file_path\": \"/tmp/one\"}}",
    "{\"type\": \"function\", \"function\": {\"name\": \"Read\", \"parameters\": {}}}",
  ] {
    assert!(text_tool_call(answer), "{answer}");
  }
}

#[test]
fn a_report_quoting_call_markup_as_code_or_naming_it_in_words_is_no_call() {
  for answer in [
    format!("The job ended on this answer:\n\n```\n{TOOL_CALL_BLOCK}\n```\n\nThe detector now holds it."),
    format!("~~~xml\n{BARE_INVOKE}\n~~~"),
    "The canary ended on `<invoke name=\"function\">` and `<|im_start|>`, which the hold now catches."
      .to_owned(),
    "The function returns early, and a functional test covers the <functional> path.".to_owned(),
    "{\"name\": \"report\", \"status\": \"done\"}".to_owned(),
    "The work is done.".to_owned(),
  ] {
    assert!(!text_tool_call(&answer), "{answer}");
  }
}
