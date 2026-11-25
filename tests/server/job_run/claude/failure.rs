//! What a failed Claude turn was: a full window, a login gone, a run that died, and how each travels.

use agency_server::job_run::claude::failure::*;

#[test]
fn a_full_window_is_a_limit_with_the_reset_it_names() {
  assert_eq!(
    of_unanswered("Claude AI usage limit reached|1791430000", "job-worker"),
    Failure::Limit {
      resets_at: Some(1_791_430_000),
      account: None,
      silenced: false,
      named: None,
      model: None,
    }
  );
  assert_eq!(
    of_unanswered("You've hit your session limit · resets 3pm", "job-worker"),
    Failure::Limit {
      resets_at: None,
      account: None,
      silenced: false,
      named: Some("session".into()),
      model: None,
    }
  );
}

// The CLI names one model's limit by the model's family word, a word of its id; a window's name, a version
// number or no name at all names no model.
#[test]
fn a_limit_named_for_a_model_is_told_from_the_logins_own() {
  let fable =
    "You've reached your Fable limit. Run /usage-credits to continue or switch models with /model.";

  assert_eq!(limit_named(fable).as_deref(), Some("Fable"));
  assert_eq!(of_errored(fable, "job-worker"), Failure::limit(None, Some("Fable".into())));
  assert_eq!(limit_named("You've hit your limit · resets 7pm"), None);
  assert!(names_model("Fable", "claude-fable-5-2"));
  assert!(!names_model("Fable", "claude-opus-5-5"));
  assert!(!names_model("session", "claude-fable-5-2"));
  assert!(!names_model("5 hour", "claude-opus-5-5"));
}

#[test]
fn a_login_gone_is_read_off_the_last_line_and_named_by_the_key() {
  let failed = of_unanswered("banner\n\nInvalid API key · Please run /login\n", "job-worker");

  assert_eq!(
    failed,
    Failure::Account {
      failure: AccountFailure::Login,
      detail: "job-worker: banner\nInvalid API key · Please run /login".to_owned(),
    }
  );
  assert_eq!(failed.account_failure(), Some(AccountFailure::Login));

  // A long-lived token expired or revoked, in the words the CLI says it.
  for said in [
    "OAuth token has expired",
    "Failed to authenticate: OAuth token revoked.",
    "Invalid bearer token",
  ] {
    assert_eq!(
      of_unanswered(said, "job-worker").account_failure(),
      Some(AccountFailure::Login),
      "{said}"
    );
  }
}

#[test]
fn anything_else_died_with_the_tail_of_what_it_said() {
  let failed =
    of_unanswered("\x1b[31m────\x1b[0m\nNo conversation found with session ID x\n", "job-worker");

  assert_eq!(
    failed,
    Failure::Died {
      ending: RunEnding::Exit { code: 1 },
      said: Some("No conversation found with session ID x".to_owned()),
      message: "claude exited with code 1: No conversation found with session ID x".to_owned(),
    }
  );
  assert_eq!(failed.account_failure(), None);
}

#[test]
fn a_long_text_keeps_its_tail_from_a_whole_line() {
  let long = format!("{}\n{}", "a".repeat(10), "b".repeat(FAILURE_TEXT_LIMIT));
  let kept = failure_text(&format!("{long}\nlast"));

  assert!(kept.starts_with("…\n"));
  assert!(kept.ends_with("last"));
}

#[test]
fn a_failure_travels_by_its_kind() {
  assert_eq!(
    serde_json::to_value(Failure::Limit {
      resets_at: Some(5),
      account: None,
      silenced: false,
      named: None,
      model: None,
    })
    .unwrap(),
    serde_json::json!({ "kind": "limit", "resetsAt": 5 })
  );
  assert_eq!(
    serde_json::to_value(of_unanswered("boom", "k")).unwrap(),
    serde_json::json!({ "kind": "died", "ending": { "kind": "exit", "code": 1 }, "said": "boom", "message": "claude exited with code 1: boom" })
  );
}

// A connection refused — the model gateway restarting with a deploy among them — is the service not answering,
// as a server error is; a request refused for its shape is the work's own.
#[test]
fn a_refused_connection_is_an_outage_and_a_request_refused_for_its_shape_is_the_work_s() {
  let refused = "API Error: Connection refused — a firewall or proxy may be blocking it (ECONNREFUSED)";

  assert_eq!(of_errored(refused, "bmo").account_failure(), Some(AccountFailure::Outage));
  assert!(!request_refused(refused, Some("server_error")));
  assert!(!request_refused("API Error: 503 Service is too busy", Some("server_error")));
  assert!(request_refused("API Error: 400 messages must alternate", None));
  assert!(request_refused("API Error: 422 unprocessable", None));
  assert!(request_refused("anything", Some("invalid_request")));
  assert!(request_refused("anything", Some("max_output_tokens")));
  assert!(!request_refused("API Error: 429 too many requests", Some("rate_limit")));
}

// The endpoint answers in HTTP status codes rather than in a subscription's words: a refused key or a
// spent balance waits for a person, and too many requests at once is a short limit. The text is what
// DeepSeek answered with, in the line Claude Code 2.1.304 writes around it.
#[test]
fn an_endpoints_refusals_are_read_as_its_accounts_and_never_as_the_works() {
  let refused = "API Error: 401 Authentication Fails, Your api key: ****0000 is invalid (request_id: cf3dd8cd-e802-4102-bd4d-e573aa73c839)";

  // The interactive line the resident program leaves, and the line a print run leaves.
  for said in [format!("Please run /login · {refused}"), format!("Failed to authenticate. {refused}")] {
    for failed in [of_unanswered(&said, "bmo"), of_errored(&said, "bmo")] {
      assert_eq!(failed.account_failure(), Some(AccountFailure::Login), "{said}");
    }
  }

  // The balance spent, as the endpoint's documented error shape names it, rendered and raw.
  for said in [
    "API Error: 402 Insufficient Balance",
    r#"API Error: {"error":{"message":"Insufficient Balance","type":"unknown_error","param":null,"code":"invalid_request_error"}}"#,
  ] {
    assert_eq!(of_errored(said, "bmo").account_failure(), Some(AccountFailure::Login), "{said}");
  }

  // A body read without a status in front is still its key refused.
  assert_eq!(
    of_errored(
      r#"API Error: {"error":{"message":"Authentication Fails, Your api key: ****0000 is invalid","type":"authentication_error","param":null,"code":"invalid_request_error"}}"#,
      "bmo"
    )
    .account_failure(),
    Some(AccountFailure::Login)
  );

  // Too many requests at once is a short rest, never a work failure.
  assert_eq!(
    of_errored("API Error: Request rejected (429) · this may be a temporary capacity issue.", "bmo"),
    Failure::Limit {
      resets_at: None,
      account: None,
      silenced: false,
      named: None,
      model: None,
    }
  );
  assert_eq!(
    of_unanswered("API Error: Request rejected (429) · Rate limit reached", "bmo").account_failure(),
    Some(AccountFailure::Limit)
  );
}

// A 403 is the account's own refusal, the firewall in front of the API answering it among them, read so
// whether the CLI asks for `/login` beside it or not, and in its own words.
#[test]
fn a_403_is_the_account_s_refusal_whatever_stands_beside_it() {
  // The resident program's line, and the line a print run of the CLI leaves.
  for said in [
    "Please run /login · API Error: 403 Attention Required! | Cloudflare",
    "Failed to authenticate. API Error: 403 Attention Required! | Cloudflare",
  ] {
    for failed in [of_unanswered(said, "coder"), of_errored(said, "coder")] {
      assert_eq!(failed.account_failure(), Some(AccountFailure::Login), "{said}");
      assert_eq!(failed.message(), format!("coder: {said}"));
    }
  }
}

#[test]
fn a_server_error_is_an_outage_and_every_failure_names_its_cause_in_english() {
  assert_eq!(
    of_errored("API Error: 529 overloaded", "k").account_failure(),
    Some(AccountFailure::Outage)
  );

  let untrusted = Failure::Untrusted {
    folder: "/work".into(),
    config: "/home/tester/.claude.json".into(),
  };

  assert!(untrusted.cause().starts_with("JobFolderUntrusted: Claude Code did not get past"));
  assert!(
    untrusted.message().contains("/work") && untrusted.message().contains("/home/tester/.claude.json")
  );
  assert!(Failure::Exhausted { attempted: vec![] }.message().is_ascii());
}
