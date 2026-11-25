//! The gateway's table written from the registry, and the digest that decides whether a running child
//! serves it.

use std::collections::{BTreeMap, HashMap};

use llm_gateway::supply::PersonalKey;
use llm_gateway::table::{
  HOOKS, Member, Owner, Table, gateway_table, gateway_table_with_members, gateway_table_with_personal,
  laid_instances, owners, personal_members, table_digest,
};
use serde_json::{Value, json};
use store::registry::{RegisteredAddress, Registration};

fn registration(name: &str, addresses: Vec<RegisteredAddress>) -> Registration {
  Registration {
    name: name.into(),
    label: name.into(),
    model: "served-model".into(),
    context_tokens: 32_768,
    answer_tokens: 4_096,
    thinking: false,
    walked: false,
    keeps_research: false,
    card_service: None,
    role_files: false,
    account: None,
    addresses,
  }
}

fn address(node: &str, url: &str, key_file: Option<&str>) -> RegisteredAddress {
  RegisteredAddress {
    node: node.into(),
    url: url.into(),
    key_file: key_file.map(str::to_owned),
    key_owned: false,
    placed_at: None,
    role_files: false,
    reach_port: None,
  }
}

fn nodes() -> Registration {
  Registration {
    model: "RVN-model".into(),
    ..registration(
      "nodes",
      vec![
        address("alpha", "http://127.0.0.1:18021/v1/chat/completions", Some("alpha-llm.token")),
        address("bravo", "http://127.0.0.1:18023/v1/chat/completions", None),
      ],
    )
  }
}

fn single() -> Registration {
  registration(
    "single",
    vec![address(
      "d-vex",
      "https://llm.example.com/v1/chat/completions",
      Some("registered/single-d-vex.key"),
    )],
  )
}

fn table() -> Table {
  gateway_table(&[nodes(), single(), registration("addressless", vec![])])
}

#[test]
fn each_registration_is_served_under_its_name_and_each_node_of_several_on_its_own() {
  let config: Value = serde_json::from_str(&table().text).unwrap();
  let served: Vec<(String, String)> = config["model_list"]
    .as_array()
    .unwrap()
    .iter()
    .map(|entry| {
      (
        entry["model_name"].as_str().unwrap().to_owned(),
        entry["litellm_params"]["api_base"].as_str().unwrap().to_owned(),
      )
    })
    .collect();
  let named = |name: &str, base: &str| (name.to_owned(), base.to_owned());
  assert_eq!(
    served,
    [
      named("nodes", "http://127.0.0.1:18021/v1"),
      named("nodes@alpha", "http://127.0.0.1:18021/v1"),
      named("nodes", "http://127.0.0.1:18023/v1"),
      named("nodes@bravo", "http://127.0.0.1:18023/v1"),
      named("single", "https://llm.example.com/v1"),
    ]
  );
}

// "a person's local AI is asked at its port of this machine's loopback, its path kept, never at the host it names"
#[test]
fn a_local_ai_reached_through_its_owner_s_pc_is_asked_at_its_loopback_port() {
  let mut reached = address("pc", "http://localhost:11434/v1/chat/completions", Some("users/7/ollama-key"));

  reached.reach_port = Some(47_300);

  let config: Value =
    serde_json::from_str(&gateway_table(&[registration("u7-ollama", vec![reached])]).text).unwrap();
  let entry = &config["model_list"][0];

  assert_eq!(entry["model_name"], "u7-ollama");
  assert_eq!(entry["litellm_params"]["api_base"], "http://127.0.0.1:47300/v1");
}

#[test]
fn each_address_is_asked_for_the_model_with_no_wait_and_thinking_off_and_keys_named_by_variable() {
  let table = table();
  let config: Value = serde_json::from_str(&table.text).unwrap();
  assert_eq!(
    config["model_list"][0]["litellm_params"],
    json!({
      "model": "openai/RVN-model",
      "api_base": "http://127.0.0.1:18021/v1",
      "api_key": "os.environ/BMO_KEY_ALPHA_LLM_TOKEN",
      "chat_template_kwargs": { "enable_thinking": false },
    })
  );
  assert_eq!(config["model_list"][2]["litellm_params"]["api_key"], "no-key");
  assert_eq!(
    table.key_files,
    BTreeMap::from([
      ("BMO_KEY_ALPHA_LLM_TOKEN".to_owned(), "alpha-llm.token".to_owned()),
      (
        "BMO_KEY_REGISTERED_SINGLE_D_VEX_KEY".to_owned(),
        "registered/single-d-vex.key".to_owned()
      ),
    ])
  );
  assert_eq!(
    config["litellm_settings"],
    json!({
      "drop_params": true,
      "use_chat_completions_url_for_anthropic_messages": true,
      "callbacks": ["thinking.handler", "sampling.handler", "reminders.handler", "long_tools.handler", "roles.handler", "unclocked.handler", "costs.handler"],
      "num_retries": 0,
    })
  );
  assert_eq!(config["general_settings"], json!({ "master_key": "os.environ/LLM_GATEWAY_KEY" }));
}

#[test]
fn the_table_is_written_in_its_fixed_form_byte_for_byte() {
  // The fixture is the table these registrations give, held byte for byte: its key order, its
  // indentation and its closing line break are what LiteLLM is started on.
  assert_eq!(table().text, include_str!("fixtures/table.json"));
}

#[test]
fn a_key_given_again_is_a_new_table_and_the_same_table_otherwise() {
  let table = table();
  let mut keys = HashMap::from([("alpha-llm.token", "first"), ("registered/single-d-vex.key", "k")]);
  let digest = |keys: &HashMap<&str, &str>, table: &Table| {
    table_digest(table, |file| keys.get(file).map(|key| (*key).to_owned()), &[])
  };
  let before = digest(&keys, &table);
  assert_eq!(digest(&keys, &table), before);
  assert_eq!(digest(&keys, &gateway_table(&[nodes(), single()])), before);
  keys.insert("alpha-llm.token", "second");
  assert_ne!(digest(&keys, &table), before);
  assert!(!table.text.contains("first"));
}

#[test]
fn a_changed_hook_is_a_new_table() {
  let table = table();
  let hooks = |reminders: &str| -> Vec<(String, Vec<u8>)> {
    HOOKS
      .iter()
      .map(|hook| {
        (
          (*hook).to_owned(),
          if *hook == "reminders" { reminders } else { "same" }.as_bytes().to_vec(),
        )
      })
      .collect()
  };
  let digest = |hooks: &[(String, Vec<u8>)]| table_digest(&table, |_| None, hooks);
  assert_eq!(digest(&hooks("first")), digest(&hooks("first")));
  assert_ne!(digest(&hooks("first")), digest(&hooks("second")));
}

#[test]
fn an_address_that_keeps_role_files_says_so_on_every_entry_it_stands_behind_and_no_other_does() {
  let mut kept = address("alpha", "http://127.0.0.1:18021/v1/chat/completions", None);

  kept.role_files = true;

  let table = gateway_table(&[registration(
    "nodes",
    vec![kept, address("bravo", "http://127.0.0.1:18023/v1/chat/completions", None)],
  )]);
  let read: serde_json::Value = serde_json::from_str(&table.text).unwrap();
  let infos: Vec<(String, serde_json::Value)> = read["model_list"]
    .as_array()
    .unwrap()
    .iter()
    .map(|entry| (entry["model_name"].as_str().unwrap().to_owned(), entry["model_info"].clone()))
    .collect();
  let on = serde_json::json!({ "role_files": true });

  assert_eq!(
    infos,
    [
      ("nodes".to_owned(), on.clone()),
      ("nodes@alpha".to_owned(), on),
      ("nodes".to_owned(), serde_json::Value::Null),
      ("nodes@bravo".to_owned(), serde_json::Value::Null),
    ]
  );
  assert!(
    read["litellm_settings"]["callbacks"]
      .as_array()
      .unwrap()
      .contains(&"roles.handler".into())
  );
}

#[test]
fn members_are_served_under_their_own_names_on_their_providers() {
  let members = [
    Member {
      name: "deepseek-flash".into(),
      road: Some("deepseek".into()),
      label: Some("DeepSeek V4.1 Flash".into()),
      provider: "deepseek".into(),
      model: "deepseek-flash".into(),
      api_base: Some("https://api.deepseek.com/v1".into()),
      key_file: "deepseek.key".into(),
      context_tokens: Some(1_048_576),
      answer_tokens: Some(32_768),
      thinking: Some(true),
      free: None,
      account: None,
    },
    Member {
      name: "openrouter-deepseek-flash".into(),
      road: Some("openrouter".into()),
      label: Some("DeepSeek V4.1 Flash on OpenRouter".into()),
      provider: "openrouter".into(),
      model: "deepseek/deepseek-v4.1-flash-1017".into(),
      api_base: Some("https://openrouter.ai/api/v1".into()),
      key_file: "openrouter.key".into(),
      context_tokens: Some(1_048_576),
      answer_tokens: Some(32_768),
      thinking: Some(true),
      free: None,
      account: None,
    },
  ];
  let table = gateway_table_with_members(&[], &members).unwrap();
  let config: Value = serde_json::from_str(&table.text).unwrap();
  let entries = config["model_list"].as_array().unwrap();

  assert_eq!(entries.len(), 2);
  assert_eq!(entries[0]["model_name"], "deepseek-flash");
  assert_eq!(entries[0]["litellm_params"]["model"], "deepseek/deepseek-flash");
  assert_eq!(entries[1]["model_name"], "openrouter-deepseek-flash");
  assert_eq!(entries[1]["litellm_params"]["model"], "openrouter/deepseek/deepseek-v4.1-flash-1017");
  assert_eq!(entries[1]["litellm_params"]["api_key"], "os.environ/BMO_KEY_OPENROUTER_KEY");
  assert_eq!(
    table.key_files.get("BMO_KEY_OPENROUTER_KEY").map(String::as_str),
    Some("openrouter.key")
  );
}

#[test]
fn a_member_naming_a_registration_refuses_the_table() {
  let member = Member {
    name: "taken".into(),
    road: None,
    label: None,
    provider: "deepseek".into(),
    model: "taken-model".into(),
    api_base: None,
    key_file: "deepseek.key".into(),
    context_tokens: None,
    answer_tokens: None,
    thinking: None,
    free: None,
    account: None,
  };
  let refused = gateway_table_with_members(&[registration("taken", vec![])], &[member]);

  assert!(refused.is_err());
}

#[test]
fn a_registration_naming_a_card_service_is_served_by_the_master_s_standing_instances_each_by_its_node() {
  let carded = Registration {
    card_service: Some("arena".into()),
    role_files: true,
    ..registration("flash", vec![])
  };
  let picture = json!({
    "services": [{
      "serviceName": "arena", "nodeNames": ["n3", "n4"],
      "instances": [{ "nodeName": "n3", "address": "http://127.0.0.1:18021/v1/chat/completions", "keyFile": "/keys/arena.key" }],
    }],
    "nodes": [],
  });
  let laid = laid_instances(vec![carded.clone()], Some(&picture));

  assert_eq!(laid[0].addresses.len(), 1);
  assert!(laid[0].addresses[0].role_files, "an instance keeps role files as its registration says");

  let table = gateway_table(&laid);
  let read: Value = serde_json::from_str(&table.text).unwrap();
  let names: Vec<&str> = read["model_list"]
    .as_array()
    .unwrap()
    .iter()
    .map(|entry| entry["model_name"].as_str().unwrap())
    .collect();

  assert_eq!(names, vec!["flash", "flash@n3"], "one standing instance is still served under its node");
  assert_eq!(read["model_list"][1]["model_info"]["role_files"], true);

  for entry in read["model_list"].as_array().unwrap() {
    let params = &entry["litellm_params"];

    assert!(
      params.get("timeout").is_none() && params.get("stream_timeout").is_none(),
      "no entry names a wait: {entry}"
    );
  }

  assert!(
    read["litellm_settings"]["callbacks"]
      .as_array()
      .unwrap()
      .contains(&"unclocked.handler".into())
  );
  assert_eq!(table.key_files.values().next().map(String::as_str), Some("/keys/arena.key"));

  for nothing in [None, Some(json!({ "services": [] }))] {
    let laid = laid_instances(vec![carded.clone()], nothing.as_ref());

    assert!(laid[0].addresses.is_empty(), "with no picture of it, it is served nowhere");
  }
}

fn road_member(name: &str, road: Option<&str>) -> Member {
  Member {
    name: name.into(),
    road: road.map(str::to_owned),
    label: Some(format!("{name} label")),
    provider: "openrouter".into(),
    model: format!("{name}-model"),
    api_base: Some("https://openrouter.ai/api/v1".into()),
    key_file: format!("{name}.key"),
    context_tokens: Some(131_072),
    answer_tokens: Some(8_192),
    thinking: Some(false),
    free: None,
    account: None,
  }
}

fn personal_key(id: i64, user_id: i64, road: &str, secret: &str, place: i64) -> PersonalKey {
  PersonalKey {
    id,
    user_id: Some(user_id),
    unit_id: None,
    road: road.into(),
    secret: secret.into(),
    place,
    enabled: true,
    refused_at: None,
  }
}

#[test]
fn a_persons_lowest_open_key_of_a_road_lays_one_personal_member_beside_each_member_of_that_road() {
  let members = [
    road_member("router-a", Some("openrouter")),
    road_member("router-b", Some("openrouter")),
    road_member("flash", Some("deepseek")),
    road_member("by-hand", None),
  ];
  let keys = [
    personal_key(1, 7, "openrouter", "router-second", 2),
    personal_key(2, 7, "openrouter", "router-first", 1),
    PersonalKey {
      enabled: false,
      ..personal_key(3, 7, "openrouter", "router-off", 0)
    },
    PersonalKey {
      refused_at: Some(1_000),
      ..personal_key(4, 7, "deepseek", "flash-refused", 0)
    },
    personal_key(5, 7, "deepseek", "../outside", 1),
    personal_key(6, 9, "deepseek", "flash-mine", 4),
  ];
  let personal = personal_members(&members, &keys);
  let laid: Vec<(&str, &str, i64)> = personal
    .iter()
    .map(|one| (one.member.name.as_str(), one.member.key_file.as_str(), owner_number(one.owner)))
    .collect();

  assert_eq!(
    laid,
    [
      ("router-a@u7", "users/7/router-first", 7),
      ("router-b@u7", "users/7/router-first", 7),
      ("flash@u9", "users/9/flash-mine", 9),
    ],
    "a key turned off, found refused or outside the person's folder lays none"
  );
  assert_eq!(
    personal[0].member,
    Member {
      name: "router-a@u7".into(),
      road: None,
      key_file: "users/7/router-first".into(),
      account: Some(7),
      ..members[0].clone()
    },
    "a personal member is its road member's copy on the person's key, of no road"
  );

  let table = gateway_table_with_personal(&[], &members, &personal).unwrap();
  let config: Value = serde_json::from_str(&table.text).unwrap();

  assert_eq!(config["model_list"][4]["model_name"], "router-a@u7");
  assert_eq!(
    config["model_list"][4]["litellm_params"]["api_key"],
    "os.environ/BMO_KEY_USERS_7_ROUTER_FIRST"
  );
  assert_eq!(
    table.key_files.get("BMO_KEY_USERS_7_ROUTER_FIRST").map(String::as_str),
    Some("users/7/router-first")
  );
  assert_eq!(
    owners(&[], &members, &personal),
    BTreeMap::from([
      ("flash@u9".to_owned(), Owner::Personal(9)),
      ("router-a@u7".to_owned(), Owner::Personal(7)),
      ("router-b@u7".to_owned(), Owner::Personal(7)),
    ])
  );
}

// The person or the unit a laid member is for, as a number.
fn owner_number(owner: Owner) -> i64 {
  match owner {
    Owner::Personal(number) | Owner::Owned(number) | Owner::Organisation(number) => number,
  }
}

// "An organisation unit's key is laid the same way" (docs/own-llm.md, 「Personal members」): the unit's lowest
// open key of a road lays `<member>@o<unit>` on its own folder's secret, owned by the unit, which no payer's key
// is admitted to by the name alone.
#[test]
fn a_units_lowest_open_key_of_a_road_lays_one_member_of_the_unit_beside_each_member_of_that_road() {
  let members = [road_member("router-a", Some("openrouter")), road_member("flash", Some("deepseek"))];
  let unit_key = |id: i64, secret: &str, place: i64| PersonalKey {
    user_id: None,
    unit_id: Some(12),
    ..personal_key(id, 0, "openrouter", secret, place)
  };
  let keys = [
    unit_key(1, "org-second", 2),
    unit_key(2, "org-first", 1),
    personal_key(3, 7, "deepseek", "mine", 1),
  ];
  let personal = personal_members(&members, &keys);
  let laid: Vec<(&str, &str, Owner)> = personal
    .iter()
    .map(|one| (one.member.name.as_str(), one.member.key_file.as_str(), one.owner))
    .collect();

  assert_eq!(
    laid,
    [
      ("flash@u7", "users/7/mine", Owner::Personal(7)),
      ("router-a@o12", "units/12/org-first", Owner::Organisation(12)),
    ]
  );
  assert_eq!(personal[1].member.account, None, "a unit's member belongs to no account");
  assert!(!Owner::Organisation(12).admits(llm_gateway::supply::Payer::Account(12), None));
  assert!(!Owner::Organisation(12).admits(llm_gateway::supply::Payer::Platform, None));
}
