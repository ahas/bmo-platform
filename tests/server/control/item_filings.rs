//! An item a supervisor files (docs/api.md, 「assignments and jobs」): its room the turn's, `asked-by=owner` stamped
//! only where the turn is the owner's, any such label the call names let go, the repository it names written as
//! its `assigneeName`, where a launch reads it, and no item for an agent carrying no turn.

use agency_server::control::item_filings::{item_of, steps_check};
use agency_server::mcp::tools;
use agency_server::openapi::openapi;
use agency_server::turn_marks::{self, Mark};
use bmo_agency_client::ItemFilingAsk;
use serde_json::{Value, json};

use crate::support::scratch::Scratch;
use crate::support::standing;

fn ask(labels: &[(&str, &str)]) -> ItemFilingAsk {
  ItemFilingAsk {
    agent_name: "chat-agent".to_owned(),
    title: "Mend the guard".to_owned(),
    brief: "The guard refuses a good command.".to_owned(),
    queue_name: "todo".to_owned(),
    repository: None,
    priority: None,
    assignee_name: Some("helper".to_owned()),
    requester_name: Some("a member".to_owned()),
    labels: labels.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect(),
    steps: vec![],
    steps_check: None,
  }
}

#[test]
fn the_owner_s_turn_stamps_his_ask_and_the_room_is_the_turn_s() {
  let owner = item_of(
    &ask(&[("lane", "repair")]),
    Mark {
      chat_id: -9_000_000_000_003,
      privileged: true,
    },
  )
  .unwrap();

  assert_eq!(owner["scope"], "-9000000000003");
  assert_eq!(owner["labels"], json!({ "asked-by": "owner", "lane": "repair" }));
  assert_eq!((&owner["assigneeName"], &owner["queueName"]), (&json!("helper"), &json!("todo")));
  assert!(owner.get("priority").is_none(), "a field the call left out is left out");

  // Anyone else's turn files a draft: no stamp, and an `asked-by` the call names is let go.
  let other = item_of(
    &ask(&[("asked-by", "owner"), ("lane", "repair")]),
    Mark {
      chat_id: -5,
      privileged: false,
    },
  )
  .unwrap();

  assert_eq!(other["labels"], json!({ "lane": "repair" }));
  assert_eq!(other["scope"], "-5");
}

const TURN: Mark = Mark {
  chat_id: -5,
  privileged: false,
};

#[test]
fn an_item_naming_a_repository_goes_where_a_launch_reads_it_and_host_work_names_none() {
  let in_repository = ItemFilingAsk {
    repository: Some(" web ".to_owned()),
    assignee_name: None,
    labels: [("series".to_owned(), "rustsec-2026-0101".to_owned())].into(),
    ..ask(&[])
  };
  let item = item_of(&in_repository, TURN).unwrap();

  assert_eq!(item["assigneeName"], "web", "{item}");
  assert_eq!(item["labels"], json!({ "series": "rustsec-2026-0101" }));
  assert!(item.get("repository").is_none(), "the repository travels as the assignee alone: {item}");

  // Naming the same assignee beside it changes nothing.
  let same = ItemFilingAsk {
    assignee_name: Some("web".to_owned()),
    ..in_repository.clone()
  };

  assert_eq!(item_of(&same, TURN).unwrap()["assigneeName"], "web");

  // Host work names no repository, and its item no assignee.
  let host = ItemFilingAsk {
    assignee_name: None,
    ..ask(&[])
  };
  let item = item_of(&host, TURN).unwrap();

  assert!(item.get("assigneeName").is_none(), "{item}");

  // An assignee other than the repository it names is refused.
  let apart = ItemFilingAsk {
    assignee_name: Some("helper".to_owned()),
    ..in_repository
  };
  let refused = item_of(&apart, TURN).unwrap_err();

  assert!(refused.contains("helper") && refused.contains("web"), "{refused}");
}

#[test]
fn the_file_item_tool_takes_the_repository_and_says_how_work_across_repositories_is_filed() {
  let listed = tools(Some("chat-agent"));
  let file_item = listed.iter().find(|tool| tool.name == "file_item").expect("file_item is listed");
  let input = &openapi()["components"]["schemas"]["FileItem"];

  assert!(input["properties"]["repository"].is_object(), "{input}");
  assert!(
    file_item.description.contains("repository") && file_item.description.contains("repositories label"),
    "{}",
    file_item.description
  );
}

#[test]
fn a_turn_s_mark_stands_until_that_turn_ends_and_a_newer_one_stands_past_an_older_end() {
  let mark = Mark {
    chat_id: 7,
    privileged: true,
  };

  turn_marks::carrying("marked-agent", "run-1", mark);
  assert_eq!(turn_marks::of("marked-agent"), Some(mark));

  turn_marks::carrying(
    "marked-agent",
    "run-2",
    Mark {
      privileged: false,
      ..mark
    },
  );
  turn_marks::ended("marked-agent", "run-1");
  assert_eq!(
    turn_marks::of("marked-agent"),
    Some(Mark {
      privileged: false,
      ..mark
    })
  );

  turn_marks::ended("marked-agent", "run-2");
  assert_eq!(turn_marks::of("marked-agent"), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_carrying_no_turn_files_no_item() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let answer = reqwest::Client::new()
    .post(format!("http://127.0.0.1:{}/item-filings", server.port))
    .bearer_auth(token)
    .json(&ask(&[]))
    .send()
    .await
    .unwrap();
  let status = answer.status().as_u16();
  let body: Value = answer.json().await.unwrap_or(Value::Null);

  assert_eq!((status, body["code"].as_str()), (409, Some("no-turn")), "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_filing_s_steps_are_checked_against_the_agent_it_runs_on_and_its_mark_holds_while_they_move() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = crate::support::grounds::ground_on(scratch.db().clone(), home.path());

  crate::support::rows::seats(scratch.db(), "coder", &[("claude-code", "claude-opus-5")]).await;

  // A filing that names no agent, or `auto`, is answered with the agents of the role it would run as, for the
  // supervisor to name one; one the agency does not register is refused (docs/repositories.md, 「An agent an item
  // names」).
  for labels in [&[("role", "coder")][..], &[("role", "coder"), ("agent", "auto")][..]] {
    let unnamed = steps_check(&ground, &ask(labels)).await.unwrap_err();
    let said = &unnamed.error;

    assert_eq!(
      (unnamed.status.as_u16(), unnamed.code.as_deref()),
      (409, Some("agent-unnamed")),
      "{said}"
    );
    assert!(
      said.contains("The agents of the coder role:") && said.contains("- coder-01: "),
      "{said}"
    );
  }

  let unknown = steps_check(&ground, &ask(&[("agent", "nobody-07")])).await.unwrap_err();

  assert_eq!(
    (unknown.status.as_u16(), unknown.code.as_deref()),
    (422, Some("unknown-agent")),
    "{}",
    unknown.error
  );

  let filing = ItemFilingAsk {
    steps: vec!["Investigate the guard".to_owned()],
    ..ask(&[("agent", "coder-01")])
  };
  let (mark, words) = steps_check(&ground, &filing).await.unwrap();

  assert!(words.contains("the coder role") && words.contains(&mark), "{words}");
  assert!(
    words.contains("Investigate") && words.contains("Implement") && words.contains("Verify"),
    "{words}"
  );

  // The mark is the item's and its agent's, so the steps may move between the two filings.
  let moved = ItemFilingAsk {
    steps: vec!["Investigate".to_owned(), "Implement".to_owned(), "Verify".to_owned()],
    ..filing.clone()
  };

  assert_eq!(steps_check(&ground, &moved).await.unwrap().0, mark);

  let another = ItemFilingAsk {
    title: "Mend another guard".to_owned(),
    ..filing
  };

  assert_ne!(steps_check(&ground, &another).await.unwrap().0, mark);
}

// A filing naming a model and a role is checked against the first agent of the role on that model and names them
// all; one naming a model no agent of the role runs is refused with the role's agents (docs/repositories.md, 「An
// agent an item names」).
#[tokio::test(flavor = "multi_thread")]
async fn a_filing_naming_a_model_is_checked_against_its_candidates_and_refused_where_the_role_has_none() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = crate::support::grounds::ground_on(scratch.db().clone(), home.path());

  crate::support::rows::seats(
    scratch.db(),
    "coder",
    &[
      ("claude-code", "claude-opus-5"),
      ("deepseek", "deepseek-flash"),
      ("claude-code", "claude-opus-5"),
    ],
  )
  .await;

  let (mark, words) = steps_check(&ground, &ask(&[("model", "claude-opus-5"), ("role", "coder")]))
    .await
    .unwrap();

  assert!(
    words.contains("model=claude-opus-5 (coder-01, coder-03)") && words.contains(&mark),
    "{words}"
  );

  let lacking = steps_check(&ground, &ask(&[("model", "gpt-5.6-sol"), ("role", "coder")]))
    .await
    .unwrap_err();
  let said = &lacking.error;

  assert_eq!(
    (lacking.status.as_u16(), lacking.code.as_deref()),
    (422, Some("no-candidate")),
    "{said}"
  );
  assert!(
    said.contains("model=gpt-5.6-sol") && said.contains("- coder-02: deepseek/deepseek-flash"),
    "{said}"
  );
}
