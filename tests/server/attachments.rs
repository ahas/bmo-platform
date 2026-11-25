//! The skills and MCP servers an account attaches to its agents (docs/person-nodes.md, 「Attaching skills and MCP
//! servers」): the account's holders and what each line takes; a holder's lists set whole, refused on a line that
//! takes none of a kind, for a holder or an item not the account's; an attached skill placed in its own agent's plugin
//! alone, and a supervisor's own plugin; a launch placing attached skills on a line that takes none refused; and the
//! keys of the account's MCP servers each record withholds, by its holder and its line, and every one, the attached
//! among them, where what is attached cannot be known; agency's own record of them made to agree with the tool host's
//! listing; and attached skills placed whole or not at all. The tool host's registry and catalogue stand in as files
//! the test writes.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use agency_server::apps::{self, Issued, Withheld};
use agency_server::attachments::{self, Holder};
use agency_server::lines::Trees;
use agency_server::lines::worker::role_run;
use agency_server::personal_apps;
use agency_server::places::Ground;
use agency_server::role_agents::{self, composed_as};
use axum::http::StatusCode;
use serde_json::json;

use crate::support::grounds::seeded_with;
use crate::support::scratch::Scratch;

// The tool host standing in: an owner's personal tools and a company's catalogue read from files the test writes.
const POCKET: &str = r#"#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
root = args[args.index("--data-root") + 1]
def opt(name):
    return args[args.index(name) + 1] if name in args else None
def held(name):
    file = os.path.join(root, "stand-in", name + ".json")
    return open(file).read() if os.path.exists(file) else "[]"
if args[2] == "list":
    print(held("owner-" + opt("--owner")))
elif args[2] == "catalogue" and args[3] == "list":
    print(json.dumps({"apps": json.loads(held("catalogue-" + opt("--company"))), "seenAt": None}))
else:
    sys.stderr.write("bmo-pocket: no verb here\n"); sys.exit(2)
"#;

const ACCOUNT: &str = "40";
const ADMINISTRATOR: &str = "1";

struct Stood {
  _scratch: Scratch,
  _home: tempfile::TempDir,
  ground: Ground,
}

impl Stood {
  fn data(&self) -> &Path {
    &self.ground.roots.data
  }

  fn stand_in(&self, name: &str, value: serde_json::Value) {
    let folder = self.data().join("stand-in");

    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(format!("{name}.json")), value.to_string()).unwrap();
  }

  // A skill of the account's own, written as a save leaves it.
  fn skill(&self, account: &str, name: &str) {
    let folder = agency_server::skills::folder_of(self.data(), account).join(name);

    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
      folder.join("SKILL.md"),
      format!("---\nname: {name}\ndescription: Does {name}.\n---\nDo {name}.\n"),
    )
    .unwrap();
    std::fs::write(folder.join(".version"), "1-aaaaaa").unwrap();
  }
}

async fn stood() -> Stood {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let pocket = home.path().join("fake-pocket");

  std::fs::write(&pocket, POCKET).unwrap();
  std::fs::set_permissions(&pocket, std::fs::Permissions::from_mode(0o755)).unwrap();

  let variables = BTreeMap::from([
    (personal_apps::POCKET_VARIABLE.to_owned(), pocket.to_string_lossy().into_owned()),
    ("BMO_DEPLOY_DIR".to_owned(), env!("CARGO_MANIFEST_DIR").to_owned()),
  ]);
  let ground = seeded_with(scratch.db(), home.path(), &variables).await;

  scratch
    .run(
      &format!(
        r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '{ADMINISTRATOR}')"#
      ),
      vec![],
    )
    .await;
  scratch
    .run(
      &format!(
        r#"INSERT INTO "supervisors" ("name", "label", "owner_account", "session_scope", "turn_shape", "created_at", "updated_at")
           VALUES ('forty-bmo', 'Forty''s BMO', {ACCOUNT}, 'agent', 'pieces', 1, 1)"#
      ),
      vec![],
    )
    .await;

  // Two agents of the coder role, on the worker line, and one of the voyager role, on the search line.
  for (name, role) in [("coder-u40", "coder"), ("coder-u40-02", "coder"), ("voyager-u40", "voyager")] {
    let mut agent = role_agents::of_role(scratch.db(), role).await.unwrap().remove(0);

    agent.name = name.to_owned();
    agent.owner_account = Some(ACCOUNT.parse().unwrap());
    agent.skills = None;
    role_agents::write(scratch.db(), &agent).await.unwrap();
  }

  Stood {
    _scratch: scratch,
    _home: home,
    ground,
  }
}

#[tokio::test]
async fn a_holder_s_lists_are_set_whole_and_refused_where_its_line_takes_none() {
  let stood = stood().await;
  let ground = &stood.ground;

  stood.skill(ACCOUNT, "tidy");

  let listed = attachments::listing(ground, ACCOUNT).await.unwrap();
  let names: Vec<(&str, &str, bool, bool)> = listed
    .holders
    .iter()
    .map(|holder| (holder.kind.as_str(), holder.name.as_str(), holder.takes.skills, holder.takes.mcp))
    .collect();

  assert_eq!(
    names,
    [
      ("supervisor", "forty-bmo", true, true),
      ("agent", "coder-u40", true, true),
      ("agent", "coder-u40-02", true, true),
      ("agent", "voyager-u40", false, false),
    ]
  );
  assert_eq!(listed.skills.iter().map(|skill| skill.name.as_str()).collect::<Vec<_>>(), ["tidy"]);
  assert_eq!(listed.skills[0].origin, "own");

  let set = attachments::set(ground, ACCOUNT, ("agent", "coder-u40"), &["tidy".to_owned()], &[])
    .await
    .unwrap();

  assert_eq!(set.skills, ["tidy"]);
  assert_eq!(
    attachments::holders_of(&ground.db, ACCOUNT, "skill", "tidy").await.unwrap(),
    ["coder-u40"]
  );

  let refused = |result: Result<_, agency_server::control::Refused>| {
    let refused = result.unwrap_err();

    (refused.status, refused.code())
  };

  assert_eq!(
    refused(attachments::set(ground, ACCOUNT, ("agent", "voyager-u40"), &["tidy".to_owned()], &[]).await),
    (StatusCode::CONFLICT, "line-takes-none".to_owned())
  );
  assert_eq!(
    refused(attachments::set(ground, ACCOUNT, ("agent", "nobody"), &[], &[]).await),
    (StatusCode::NOT_FOUND, "not-found".to_owned())
  );
  assert_eq!(
    refused(attachments::set(ground, ACCOUNT, ("agent", "coder-u40"), &["missing".to_owned()], &[]).await),
    (StatusCode::NOT_FOUND, "not-found".to_owned())
  );

  // An agent of no account is the administrator's holder, never another account's.
  let deployment_s = role_agents::of_role(&ground.db, "coder")
    .await
    .unwrap()
    .into_iter()
    .find(|agent| agent.owner_account.is_none())
    .unwrap()
    .name;

  assert_eq!(
    refused(attachments::set(ground, ACCOUNT, ("agent", &deployment_s), &[], &[]).await),
    (StatusCode::FORBIDDEN, "forbidden".to_owned())
  );
  assert!(
    attachments::listing(ground, ADMINISTRATOR)
      .await
      .unwrap()
      .holders
      .iter()
      .any(|holder| holder.name == deployment_s)
  );

  // Set again whole, the list empties.
  attachments::set(ground, ACCOUNT, ("agent", "coder-u40"), &[], &[]).await.unwrap();
  assert!(attachments::holders_of(&ground.db, ACCOUNT, "skill", "tidy").await.unwrap().is_empty());
}

// "the carries-manifest assembly includes an attached skill only for its agent"
#[tokio::test]
async fn an_attached_skill_reaches_its_own_agent_s_plugin_alone() {
  let stood = stood().await;
  let ground = &stood.ground;
  let trees = Trees::of(ground);

  stood.skill(ACCOUNT, "tidy");
  attachments::set(ground, ACCOUNT, ("agent", "coder-u40"), &["tidy".to_owned()], &[])
    .await
    .unwrap();
  attachments::set(ground, ACCOUNT, ("supervisor", "forty-bmo"), &["tidy".to_owned()], &[])
    .await
    .unwrap();

  let run_as = |agent: &'static str| {
    let (ground, trees) = (ground, &trees);

    composed_as(
      agent.to_owned(),
      role_run(ground, trees, "coder", None, None, "claude-opus-5-5", "/work", "claude-code"),
    )
  };
  let attached = run_as("coder-u40").await.unwrap().plugin_dir;
  let other = run_as("coder-u40-02").await.unwrap().plugin_dir;

  assert!(attached.join("skills/tidy/SKILL.md").is_file(), "{}", attached.display());
  assert!(attached.ends_with("role-agents/coder-u40/plugin"));
  assert!(!other.join("skills/tidy").exists(), "{}", other.display());

  // The role's own skills stand beside it.
  let role_skills = agency_server::lines::agent_plugin::held_in(&other);

  for skill in &role_skills {
    assert!(attached.join("skills").join(skill).is_dir(), "{skill}");
  }

  // A supervisor holding attached skills is handed a plugin of its own; one holding none, none.
  let plugin = attachments::supervisor_plugin(ground, "forty-bmo").await.unwrap().unwrap();

  assert!(plugin.join("skills/tidy/SKILL.md").is_file());
  assert!(plugin.join(".claude-plugin/plugin.json").is_file());
  assert!(plugin.ends_with("supervisor-skills/forty-bmo/plugin"));
  assert_eq!(attachments::supervisor_plugin(ground, "nobody-bmo").await.unwrap(), None);
}

// "a launch asked to place attached skills on a line that takes none is refused"
#[tokio::test]
async fn attached_skills_on_a_line_that_takes_none_are_refused() {
  let stood = stood().await;
  let ground = &stood.ground;
  let tidy = vec![("tidy".to_owned(), stood.data().join("tidy"))];

  for line in ["search", "browse", "plain", "gate", "translate"] {
    let refused = attachments::place_on(line, "an agent", tidy.clone()).unwrap_err();

    assert_eq!(refused.code, "line-takes-none", "{line}");
    assert!(attachments::place_on(line, "an agent", Vec::new()).unwrap().is_empty(), "{line}");
  }

  for line in ["worker", "chat"] {
    assert_eq!(attachments::place_on(line, "an agent", tidy.clone()).unwrap(), tidy, "{line}");
  }

  // A row standing for a holder whose line takes none: its launch is refused, never placed.
  stood.skill(ACCOUNT, "tidy");
  sea_orm::ConnectionTrait::execute_unprepared(
    &ground.db,
    r#"INSERT INTO "agent_attachments" VALUES ('40', 'agent', 'voyager-u40', 'skill', 'tidy', 1)"#,
  )
  .await
  .unwrap();
  assert!(
    attachments::attached_skills(ground, "search", &Holder::agent("voyager-u40"))
      .await
      .is_err()
  );
}

// "every record's withheld-apps file names the key of every MCP server of the record's account not attached to its
// holder, and every one where its line takes none; the administrator's names those alone, personal both true"
#[tokio::test]
async fn each_record_withholds_the_mcp_servers_its_holder_is_not_attached_to() {
  let stood = stood().await;
  let ground = &stood.ground;

  stood.stand_in(
    "owner-40",
    json!([
      { "name": "tracker", "app": "own-40-tracker", "entries": [{ "name": "my/list_issues", "title": "Issues" }] },
      { "name": "files", "app": "shared-3-17-files", "entries": [] },
      { "name": "weekly", "app": "shared-3-17-weekly", "entries": [] },
      { "name": "notes", "entries": [] },
    ]),
  );
  stood.stand_in(
    "catalogue-3",
    json!([
      { "id": "17-files", "kind": "mcp", "key": "shared-3-17-files", "name": "files", "title": "Files",
        "titles": { "en": "Files" }, "description": "", "maker": "17", "owner": "17", "unit": "40", "targets": [],
        "versions": [] },
      { "id": "17-weekly", "key": "shared-3-17-weekly", "name": "weekly", "title": "Weekly", "description": "",
        "maker": "17", "owner": "17", "unit": "40", "targets": [], "versions": [] },
    ]),
  );

  let items: Vec<(String, bool)> = attachments::mcp_items(ground, ACCOUNT, true)
    .await
    .unwrap()
    .into_iter()
    .map(|item| (item.name, item.own))
    .collect();

  assert_eq!(
    items,
    [("tracker".to_owned(), true), ("files".to_owned(), false)],
    "an app is no MCP server"
  );

  attachments::set(ground, ACCOUNT, ("agent", "coder-u40"), &[], &["tracker".to_owned()])
    .await
    .unwrap();

  let withheld = |holder: Option<Holder>, line: &'static str| async move {
    let withheld = attachments::withheld_mcp(ground, ACCOUNT, holder.as_ref(), Some(line)).await.unwrap();

    assert_eq!(withheld.why, None, "what is attached was known");
    withheld.keys
  };
  let both = ["own-40-tracker".to_owned(), "shared-3-17-files".to_owned()].into();

  assert_eq!(
    withheld(Some(Holder::agent("coder-u40")), "worker").await,
    ["shared-3-17-files".to_owned()].into()
  );
  assert_eq!(withheld(Some(Holder::agent("coder-u40-02")), "worker").await, both);
  assert_eq!(withheld(Some(Holder::supervisor("forty-bmo")), "chat").await, both);
  assert_eq!(withheld(Some(Holder::agent("coder-u40")), "search").await, both, "the line takes none");
  assert_eq!(withheld(None, "worker").await, both, "a record naming no holder");

  // The administrator's file names its MCP servers alone, and keeps the network and secrets.
  stood.stand_in("owner-1", json!([{ "name": "tracker", "app": "own-1-tracker", "entries": [] }]));

  let file = stood.data().join("withheld.json");
  let issued = Issued {
    holder: Some(Holder::supervisor("forty-bmo")),
    line: Some("chat".to_owned()),
  };

  apps::write_for_administrator(ground, &file, ADMINISTRATOR, &issued).await;

  let written: Withheld = apps::read(&file).unwrap();

  assert_eq!(written.apps, Some(["own-1-tracker".to_owned()].into()));
  assert!(written.personal.network && written.personal.secrets);
  assert_eq!(written.holder, issued.holder);
  assert_eq!(written.attached, None, "nothing attached was withheld for want of knowing");
}

// The tool host's listing of an account's personal tools, standing in as text that does not read.
fn listing_unread(stood: &Stood, account: &str) {
  std::fs::write(stood.data().join("stand-in").join(format!("owner-{account}.json")), "not a listing")
    .unwrap();
}

// "agency keeps its own record of each account's MCP servers, made to agree with the tool host's listing where it
// reads: every server listed written, a row whose key no listed tool stands under taken away"
#[tokio::test]
async fn the_record_of_the_mcp_servers_is_made_to_agree_with_the_listing() {
  let stood = stood().await;
  let ground = &stood.ground;

  attachments::record_server(&ground.db, ACCOUNT, "gone", "own-40-gone").await.unwrap();
  attachments::record_server(&ground.db, ACCOUNT, "files", "shared-3-17-files")
    .await
    .unwrap();
  stood.stand_in(
    "owner-40",
    json!([
      { "name": "tracker", "app": "own-40-tracker", "entries": [] },
      { "name": "files", "app": "shared-3-17-files", "entries": [] },
    ]),
  );

  // The catalogue holds no record of the shared key: the listing alone cannot class it, so its row stands.
  let withheld = attachments::withheld_mcp(ground, ACCOUNT, None, Some("worker")).await.unwrap();

  assert_eq!(withheld.keys, ["own-40-tracker".to_owned(), "shared-3-17-files".to_owned()].into());
  assert_eq!(withheld.why, None);
  assert_eq!(
    attachments::recorded_servers(&ground.db, ACCOUNT).await.unwrap(),
    BTreeMap::from([
      ("files".to_owned(), "shared-3-17-files".to_owned()),
      ("tracker".to_owned(), "own-40-tracker".to_owned()),
    ])
  );

  // Gone from the listing, the server leaves the record.
  stood.stand_in("owner-40", json!([{ "name": "tracker", "app": "own-40-tracker", "entries": [] }]));
  attachments::withheld_mcp(ground, ACCOUNT, None, Some("worker")).await.unwrap();
  assert_eq!(
    attachments::recorded_servers(&ground.db, ACCOUNT).await.unwrap(),
    BTreeMap::from([("tracker".to_owned(), "own-40-tracker".to_owned())])
  );

  attachments::forget_server(&ground.db, ACCOUNT, "tracker").await.unwrap();
  assert!(attachments::recorded_servers(&ground.db, ACCOUNT).await.unwrap().is_empty());
}

// "where the tool host's listing cannot be read, the file names the key of every MCP server agency's record holds, the
// attached ones among them, and its `attached` says why"
#[tokio::test]
async fn an_unread_listing_withholds_every_recorded_mcp_server_attached_or_not() {
  let stood = stood().await;
  let ground = &stood.ground;

  stood.stand_in("owner-40", json!([{ "name": "tracker", "app": "own-40-tracker", "entries": [] }]));
  attachments::set(ground, ACCOUNT, ("agent", "coder-u40"), &[], &["tracker".to_owned()])
    .await
    .unwrap();
  attachments::record_server(&ground.db, ACCOUNT, "weekly", "own-40-weekly")
    .await
    .unwrap();

  let holder = Holder::agent("coder-u40");
  let both: std::collections::BTreeSet<String> =
    ["own-40-tracker".to_owned(), "own-40-weekly".to_owned()].into();

  // The listing reads: the attached server reaches its holder; the recorded one it no longer lists has left.
  let read = attachments::withheld_mcp(ground, ACCOUNT, Some(&holder), Some("worker")).await.unwrap();

  assert!(read.keys.is_empty(), "{read:?}");
  attachments::record_server(&ground.db, ACCOUNT, "weekly", "own-40-weekly")
    .await
    .unwrap();

  // The listing does not read: every recorded server is withheld, the attached one too, and why is said.
  listing_unread(&stood, ACCOUNT);

  let unread = attachments::withheld_mcp(ground, ACCOUNT, Some(&holder), Some("worker")).await.unwrap();

  assert_eq!(unread.keys, both);
  assert!(unread.why.as_deref().is_some_and(|why| why.contains("listing")), "{unread:?}");

  // The administrator's file says so in its `attached`.
  attachments::record_server(&ground.db, ADMINISTRATOR, "tracker", "own-1-tracker")
    .await
    .unwrap();
  listing_unread(&stood, ADMINISTRATOR);

  let file = stood.data().join("withheld.json");
  let issued = Issued {
    holder: Some(Holder::supervisor("forty-bmo")),
    line: Some("chat".to_owned()),
  };

  apps::write_for_administrator(ground, &file, ADMINISTRATOR, &issued).await;

  let raw: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();

  assert_eq!(raw["apps"], json!(["own-1-tracker"]));
  assert_eq!(raw["attached"]["withheld"], true, "{raw}");
  assert!(raw["attached"]["why"].as_str().unwrap().contains("listing"), "{raw}");
}

// "where agency's own record cannot be read, the file is written with `apps` null, which the tool host takes as every
// app withheld, keeping its account, holder and line"
#[tokio::test]
async fn an_unread_record_writes_a_file_the_tool_host_takes_as_every_app_withheld() {
  let stood = stood().await;
  let ground = &stood.ground;

  stood.stand_in("owner-1", json!([{ "name": "tracker", "app": "own-1-tracker", "entries": [] }]));
  sea_orm::ConnectionTrait::execute_unprepared(&ground.db, r#"DROP TABLE "mcp_servers""#)
    .await
    .unwrap();

  let refused = attachments::withheld_mcp(ground, ADMINISTRATOR, None, Some("chat")).await.unwrap_err();

  assert!(refused.contains("record"), "{refused}");

  let file = stood.data().join("withheld.json");
  let issued = Issued {
    holder: Some(Holder::supervisor("forty-bmo")),
    line: Some("chat".to_owned()),
  };

  apps::write_for_administrator(ground, &file, ADMINISTRATOR, &issued).await;

  let raw: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();

  assert_eq!(raw["apps"], serde_json::Value::Null, "{raw}");
  assert_eq!(raw["attached"]["withheld"], true, "{raw}");

  let written = apps::read(&file).unwrap();

  assert_eq!(written.apps, None);
  assert_eq!((written.account.as_str(), written.holder), (ADMINISTRATOR, issued.holder));
}

// "where a holder's attachments or an attached skill's folder do not read whole, none of its attached skills is
// placed: the run carries the plugin it carries with nothing attached, and a supervisor no plugin of attached skills"
#[tokio::test]
async fn attached_skills_that_do_not_read_whole_are_none_placed() {
  let stood = stood().await;
  let ground = &stood.ground;
  let trees = Trees::of(ground);
  let both = ["tidy".to_owned(), "sort".to_owned()];

  stood.skill(ACCOUNT, "tidy");
  stood.skill(ACCOUNT, "sort");
  attachments::set(ground, ACCOUNT, ("agent", "coder-u40"), &both, &[]).await.unwrap();
  attachments::set(ground, ACCOUNT, ("supervisor", "forty-bmo"), &both, &[])
    .await
    .unwrap();

  let plugin_of = |agent: &'static str| {
    let (ground, trees) = (ground, &trees);

    async move {
      composed_as(
        agent.to_owned(),
        role_run(ground, trees, "coder", None, None, "claude-opus-5-5", "/work", "claude-code"),
      )
      .await
      .unwrap()
      .plugin_dir
    }
  };
  let bare = plugin_of("coder-u40-02").await;

  assert!(plugin_of("coder-u40").await.join("skills/sort/SKILL.md").is_file());

  // One attached skill's folder no longer reads: neither is placed, never the one that reads alone.
  std::fs::remove_file(agency_server::skills::folder_of(stood.data(), ACCOUNT).join("sort/SKILL.md"))
    .unwrap();

  assert!(
    attachments::attached_skills(ground, "worker", &Holder::agent("coder-u40"))
      .await
      .unwrap()
      .is_empty()
  );
  assert_eq!(plugin_of("coder-u40").await, bare, "the role's plugin alone");
  assert_eq!(attachments::supervisor_plugin(ground, "forty-bmo").await.unwrap(), None);

  // The attachments themselves do not read: none placed, and the launch goes on.
  stood.skill(ACCOUNT, "sort");
  sea_orm::ConnectionTrait::execute_unprepared(&ground.db, r#"DROP TABLE "agent_attachments""#)
    .await
    .unwrap();
  assert_eq!(plugin_of("coder-u40").await, bare);
  assert_eq!(attachments::supervisor_plugin(ground, "forty-bmo").await.unwrap(), None);
}

#[test]
fn an_own_key_holds_the_account_and_the_name_within_an_app_key() {
  assert_eq!(attachments::own_key("40", "issue_tracker"), "own-40-issue-tracker");

  let long = attachments::own_key("40", &"a".repeat(60));

  assert!(long.len() <= 50 && long.starts_with("own-40-"), "{long}");
  assert_eq!(long.len(), "own-40-".len() + 12);
}
