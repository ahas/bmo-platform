//! Each tool's manifest held to the tool it declares: its name its folder's, the protocol agency's tools speak,
//! its executable the crate's own binary, and what it declares what the executable answers; and the
//! deployment step that registers it allowing what it asks for.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use toml::Value;

fn tools() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("tools")
}

fn read(path: &Path) -> Value {
  toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn text<'a>(value: &'a Value, path: &[&str]) -> &'a str {
  path.iter().fold(value, |at, key| &at[*key]).as_str().unwrap()
}

fn words(value: &Value) -> Vec<&str> {
  value.as_array().unwrap().iter().map(|word| word.as_str().unwrap()).collect()
}

#[test]
fn every_folder_under_tools_is_a_tool_named_after_it_whose_executable_is_its_crates_binary() {
  let mut folders: Vec<String> = std::fs::read_dir(tools())
    .unwrap()
    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
    .collect();

  folders.sort();

  assert_eq!(
    folders,
    ["app_put", "deepseek_top_up", "jobs", "procedures", "relay", "skill_put", "worker_ask"]
  );

  for folder in &folders {
    let manifest = read(&tools().join(folder).join("tool.toml"));
    let crate_manifest = read(&tools().join(folder).join("Cargo.toml"));
    let package = text(&crate_manifest, &["package", "name"]);
    let binary = text(&crate_manifest["bin"][0], &["name"]);

    assert_eq!(text(&manifest, &["tool", "name"]), folder);
    assert_eq!(manifest["tool"]["protocol"].as_integer(), Some(i64::from(tool_call::PROTOCOL)));
    assert_eq!(text(&manifest, &["tool", "language"]), "rust");
    assert_eq!(words(&manifest["tool"]["run"]), [binary]);
    // A call is waited on until the executable answers: no manifest names a timeout or an unbounded entry.
    assert!(manifest["tool"].get("timeout").is_none(), "{folder}");

    for entry in manifest.get("entry").and_then(Value::as_array).into_iter().flatten() {
      assert!(entry.get("timeout").is_none(), "{folder}");
      assert!(entry.get("unbounded").is_none(), "{folder}");
    }
    assert!(text(&manifest, &["tool", "build"]).contains(&format!("-p {package} ")), "{folder}");
    assert!(
      text(&manifest, &["tool", "build"]).contains(&format!("/release/{binary}\"")),
      "{folder}"
    );
  }
}

#[test]
fn the_procedures_declare_their_resources_and_no_entry() {
  let manifest = read(&tools().join("procedures").join("tool.toml"));

  assert!(manifest.get("entry").is_none());
  assert_eq!(text(&manifest, &["resources", "namespace"]), "bmo");
  assert_eq!(words(&manifest["resources"]["capabilities"]), ["room.tools"]);
  assert!(procedures_tool::PROCEDURE_PREFIX.starts_with("bmo://"));
}

#[test]
fn the_worker_ask_entry_is_declared_as_its_tool_answers_it() {
  let folder = tools().join("worker_ask");
  let manifest = read(&folder.join("tool.toml"));
  let entry = &manifest["entry"].as_array().unwrap()[0];

  assert_eq!(text(entry, &["namespace"]), worker_ask_tool::NAMESPACE);
  assert_eq!(text(entry, &["name"]), worker_ask_tool::NAME);
  assert_eq!(text(entry, &["title"]), worker_ask_tool::TITLE);
  assert_eq!(text(entry, &["description"]), worker_ask_tool::DESCRIPTION);
  assert_eq!(words(&entry["capabilities"]), worker_ask_tool::CAPABILITIES);

  let schema: serde_json::Value =
    serde_json::from_str(&std::fs::read_to_string(folder.join(text(entry, &["input"]))).unwrap()).unwrap();

  assert_eq!(schema["required"], serde_json::json!(["words"]));
  assert!(schema["properties"].get("timeBoundSeconds").is_none(), "the ask takes no time bound");
}

#[test]
fn the_relay_entries_are_declared_as_its_tool_answers_them() {
  let folder = tools().join("relay");
  let manifest = read(&folder.join("tool.toml"));
  let entries = manifest["entry"].as_array().unwrap();

  assert_eq!(entries.len(), 2);

  let relay = &entries[0];

  assert_eq!(text(relay, &["namespace"]), relay_tool::NAMESPACE);
  assert_eq!(text(relay, &["name"]), relay_tool::NAME);
  assert_eq!(text(relay, &["title"]), relay_tool::TITLE);
  assert_eq!(text(relay, &["description"]), relay_tool::DESCRIPTION);
  assert_eq!(words(&relay["capabilities"]), relay_tool::CAPABILITIES);

  let schema: serde_json::Value =
    serde_json::from_str(&std::fs::read_to_string(folder.join(text(relay, &["input"]))).unwrap()).unwrap();

  assert_eq!(schema["required"], serde_json::json!(["to", "text"]));
  assert!(schema["properties"].get("from").is_none(), "the sender is never typed");

  let chat = &entries[1];

  assert_eq!(text(chat, &["namespace"]), relay_tool::NAMESPACE);
  assert_eq!(text(chat, &["name"]), relay_tool::CHAT_NAME);
  assert_eq!(text(chat, &["title"]), relay_tool::CHAT_TITLE);
  assert_eq!(text(chat, &["description"]), relay_tool::CHAT_DESCRIPTION);
  assert_eq!(words(&chat["capabilities"]), relay_tool::CAPABILITIES);

  let schema: serde_json::Value =
    serde_json::from_str(&std::fs::read_to_string(folder.join(text(chat, &["input"]))).unwrap()).unwrap();

  assert_eq!(schema["required"], serde_json::json!(["to", "text", "room"]));
}

#[test]
fn the_deepseek_top_up_entry_runs_under_the_administrators_payment_grant_and_takes_the_amount_alone() {
  let folder = tools().join("deepseek_top_up");
  let manifest = read(&folder.join("tool.toml"));
  let entry = &manifest["entry"].as_array().unwrap()[0];

  assert_eq!(text(entry, &["namespace"]), deepseek_top_up_tool::NAMESPACE);
  assert_eq!(text(entry, &["name"]), deepseek_top_up_tool::NAME);
  assert_eq!(text(entry, &["title"]), deepseek_top_up_tool::TITLE);
  assert_eq!(text(entry, &["description"]), deepseek_top_up_tool::DESCRIPTION);
  assert_eq!(words(&entry["capabilities"]), deepseek_top_up_tool::CAPABILITIES);
  assert_eq!(text(entry, &["grant"]), deepseek_top_up_tool::GRANT);
  assert!(
    policy::person_nodes::PERSON_NODES
      .administrator_grants
      .iter()
      .any(|grant| grant == deepseek_top_up_tool::GRANT),
    "the administrator's records hold the grant the entry declares"
  );

  let schema: serde_json::Value =
    serde_json::from_str(&std::fs::read_to_string(folder.join(text(entry, &["input"]))).unwrap()).unwrap();
  let properties: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();

  assert_eq!(properties, ["amount"], "the card is never taken from a call");
  assert_eq!(schema["additionalProperties"], false);
}

/// Every capability and grant a manifest asks for, as the tool host's placement reads it: its entries'
/// capabilities and bundles, its resources' capabilities and its entries' grants.
fn asks(manifest: &Value) -> BTreeSet<&str> {
  let entries = manifest.get("entry").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
  let entry_asks = entries.iter().flat_map(|entry| {
    let capabilities = entry.get("capabilities").map(words).unwrap_or_default();
    let bundle = entry
      .get("bundle")
      .and_then(Value::as_table)
      .into_iter()
      .flat_map(|bundle| bundle.keys());
    let grant = entry.get("grant").and_then(Value::as_str);

    capabilities.into_iter().chain(bundle.map(String::as_str)).chain(grant)
  });
  let resource_asks =
    manifest.get("resources").and_then(|resources| resources.get("capabilities")).map(words);

  entry_asks.chain(resource_asks.unwrap_or_default()).collect()
}

/// The tool each `bmo-pocket put` of a placed folder under `tools/` places, with what its `--allow`s name.
fn placements(testbed: &Value) -> BTreeMap<String, BTreeSet<String>> {
  let steps = testbed["deploy"]["step"].as_array().unwrap();
  let lines = steps.iter().flat_map(|step| text(step, &["command"]).lines());

  lines
    .filter_map(shlex::split)
    .filter(
      |words| matches!(words.as_slice(), [command, verb, ..] if command == "bmo-pocket" && verb == "put"),
    )
    .filter_map(|words| {
      let tool = words
        .iter()
        .find_map(|word| word.strip_prefix("tools/")?.strip_suffix("/.placed"))?
        .to_owned();
      let allowed = words
        .windows(2)
        .filter(|pair| pair[0] == "--allow")
        .map(|pair| pair[1].clone())
        .collect();

      Some((tool, allowed))
    })
    .collect()
}

#[test]
fn each_tools_placing_step_allows_exactly_what_its_manifest_asks_for() {
  let testbed = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("testbed.toml"));
  let placements = placements(&testbed);

  for (tool, allowed) in &placements {
    let manifest = read(&tools().join(tool).join("tool.toml"));
    let allowed: BTreeSet<&str> = allowed.iter().map(String::as_str).collect();

    assert_eq!(allowed, asks(&manifest), "the --allow of {tool}'s placing step");
  }

  let with_entries: Vec<String> = std::fs::read_dir(tools())
    .unwrap()
    .map(|folder| folder.unwrap().file_name().to_string_lossy().into_owned())
    .filter(|tool| read(&tools().join(tool).join("tool.toml")).get("entry").is_some())
    .collect();

  for tool in &with_entries {
    assert!(placements.contains_key(tool), "a deployment step puts {tool}, which declares entries");
  }
}
