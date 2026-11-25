//! The skills and MCP servers an account attaches to its agents (docs/person-nodes.md, 「Attaching skills and MCP
//! servers」): the holders an account has — its supervisors and its role agents, and for the administrator's account
//! those of no account too — and what is attached to each, kept in `agent_attachments`; the attached skills a launch
//! places on a line that takes them, refused on one that does not, and none where they do not read whole; agency's
//! own record of the account's MCP servers, `mcp_servers`; and the keys of the account's MCP servers a record's holder
//! is not attached to, which its withheld-apps file names, every one where what is attached cannot be known.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::LazyLock;

use axum::http::StatusCode;
use bmo_agency_client::{AttachableItem, AttachmentHolder, AttachmentList, AttachmentTakes};
use policy::run_lines::RUN_LINES;
use sea_orm::ConnectionTrait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use store::{all, run};

use crate::Refusal;
use crate::control::Refused;
use crate::places::Ground;

/// The kinds of holder an item is attached to.
pub const SUPERVISOR: &str = "supervisor";
pub const AGENT: &str = "agent";

/// The kinds of item attached.
pub const SKILL: &str = "skill";
pub const MCP: &str = "mcp";

/// The line every supervisor's program goes out on.
const SUPERVISOR_LINE: &str = "chat";

/// The head of the key the person's own MCP server stands under at home.
pub const OWN_KEY_HEAD: &str = "own-";

/// The head of a shared app's key.
const SHARED_KEY_HEAD: &str = "shared-";

/// The most an app key holds.
const APP_KEY_MOST: usize = 50;

/// One holder an item is attached to: a supervisor, or a role's agent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holder {
  pub kind: String,
  pub name: String,
}

impl Holder {
  pub fn supervisor(name: &str) -> Self {
    Self {
      kind: SUPERVISOR.to_owned(),
      name: name.to_owned(),
    }
  }

  pub fn agent(name: &str) -> Self {
    Self {
      kind: AGENT.to_owned(),
      name: name.to_owned(),
    }
  }
}

/// The key the person's own MCP server stands under at home (docs/person-nodes.md, 「MCP servers」):
/// `own-<account>-<name>`, `_` written `-`, the name written as the first twelve hex digits of its SHA-256 where the
/// whole is longer than an app key holds.
pub fn own_key(account: &str, name: &str) -> String {
  use sha2::{Digest, Sha256};

  let lower = |text: &str| text.to_ascii_lowercase().replace('_', "-");
  let whole = format!("{OWN_KEY_HEAD}{}-{}", lower(account), lower(name));

  if whole.len() <= APP_KEY_MOST {
    return whole;
  }

  let digest: String = Sha256::digest(name.as_bytes())
    .iter()
    .take(6)
    .map(|byte| format!("{byte:02x}"))
    .collect();

  format!("{OWN_KEY_HEAD}{}-{digest}", lower(account))
}

/// The holders an item of the account's is attached to, by name.
pub async fn holders_of<C: ConnectionTrait>(
  db: &C,
  account: &str,
  item_kind: &str,
  item: &str,
) -> Result<Vec<String>, store::Fault> {
  all(
    db,
    r#"SELECT "holder" FROM "agent_attachments" WHERE "account" = $1 AND "item_kind" = $2 AND "item" = $3
       ORDER BY "holder""#,
    vec![account.into(), item_kind.into(), item.into()],
  )
  .await?
  .iter()
  .map(|row| Ok(row.try_get("", "holder")?))
  .collect()
}

/// Every attachment of an item of the account's taken away, as the item goes.
pub async fn forget_item<C: ConnectionTrait>(
  db: &C,
  account: &str,
  item_kind: &str,
  item: &str,
) -> Result<(), store::Fault> {
  run(
    db,
    r#"DELETE FROM "agent_attachments" WHERE "account" = $1 AND "item_kind" = $2 AND "item" = $3"#,
    vec![account.into(), item_kind.into(), item.into()],
  )
  .await?;
  Ok(())
}

/// What is attached to a holder: each item's account, kind and name.
pub async fn attached_to<C: ConnectionTrait>(
  db: &C,
  holder: &Holder,
) -> Result<Vec<(String, String, String)>, store::Fault> {
  all(
    db,
    r#"SELECT "account", "item_kind", "item" FROM "agent_attachments" WHERE "holder_kind" = $1 AND "holder" = $2
       ORDER BY "item_kind", "item""#,
    vec![holder.kind.clone().into(), holder.name.clone().into()],
  )
  .await?
  .iter()
  .map(|row| Ok((row.try_get("", "account")?, row.try_get("", "item_kind")?, row.try_get("", "item")?)))
  .collect()
}

/// The skills attached to a holder, each its name and its folder at home, for a launch on `line`: none where none
/// is attached, and the launch refused where some are and the line's carries take no attached skill. None either
/// where they do not read whole (docs/person-nodes.md, 「Attaching skills and MCP servers」): the holder's
/// attachments unread, or an attached skill's folder, which the log says once.
pub async fn attached_skills(
  ground: &Ground,
  line: &str,
  holder: &Holder,
) -> Result<Vec<(String, PathBuf)>, Refusal> {
  let skills = match skills_read(ground, holder).await {
    Ok(skills) => skills,
    Err(why) => {
      eprintln!(
        "agency: the skills attached to the {} {} are withheld from its launch: {why}",
        holder.kind, holder.name
      );
      Vec::new()
    }
  };

  place_on(line, &holder.name, skills)
}

// The skills attached to a holder, each read whole, or why they were not.
async fn skills_read(ground: &Ground, holder: &Holder) -> Result<Vec<(String, PathBuf)>, String> {
  let mut skills = Vec::new();

  for (account, kind, name) in attached_to(&ground.db, holder)
    .await
    .map_err(|why| format!("its attachments were not read: {}", why.detail))?
  {
    if kind != SKILL {
      continue;
    }

    let folder = crate::skills::folder_of(&ground.roots.data, &account).join(&name);

    reads_whole(&folder).map_err(|why| format!("the skill {name} does not read: {why}"))?;
    skills.push((name, folder));
  }

  Ok(skills)
}

// Whether a skill's folder reads whole: its `SKILL.md`, and every folder and file under it.
fn reads_whole(folder: &std::path::Path) -> Result<(), String> {
  let said = |path: &std::path::Path, why: std::io::Error| format!("{}: {why}", path.display());
  let skill_file = folder.join(crate::skills::SKILL_FILE);

  std::fs::read(&skill_file).map_err(|why| said(&skill_file, why))?;

  let mut open = vec![folder.to_owned()];

  while let Some(dir) = open.pop() {
    for entry in std::fs::read_dir(&dir).map_err(|why| said(&dir, why))? {
      let path = entry.map_err(|why| said(&dir, why))?.path();

      if path.is_dir() {
        open.push(path);
      } else {
        std::fs::File::open(&path).map_err(|why| said(&path, why))?;
      }
    }
  }

  Ok(())
}

/// The attached skills a launch on `line` places, refused where there are some and the line takes none.
pub fn place_on(
  line: &str,
  holder: &str,
  skills: Vec<(String, PathBuf)>,
) -> Result<Vec<(String, PathBuf)>, Refusal> {
  if !skills.is_empty() && !RUN_LINES.carries(line).takes_attached_skills() {
    return Err(Refusal::new(
      StatusCode::CONFLICT,
      "line-takes-none",
      format!("{holder} holds attached skills, and the {line} line carries none"),
    ));
  }

  Ok(skills)
}

/// A supervisor's own plugin of its attached skills, under the agents root, where it holds any; none otherwise.
pub async fn supervisor_plugin(ground: &Ground, supervisor: &str) -> Result<Option<PathBuf>, Refusal> {
  let skills = attached_skills(ground, SUPERVISOR_LINE, &Holder::supervisor(supervisor)).await?;

  if skills.is_empty() {
    return Ok(None);
  }

  let trees = crate::lines::Trees::of(ground);

  crate::lines::agent_plugin::placed(
    &trees,
    &trees.agents,
    crate::lines::agent_plugin::Whose::Supervisor(supervisor),
    &[],
    &skills,
  )
  .map(Some)
}

/// The holder a record is issued for: the seat a role's run is let in on, else the supervisor of the agent's name;
/// none for anybody else.
pub async fn holder_of_run(
  ground: &Ground,
  agent: &str,
  role: Option<&str>,
  folder: Option<&str>,
) -> Result<Option<Holder>, Refusal> {
  if role.is_some() {
    return Ok(
      crate::role_agents::seated_at(&ground.db, role, folder)
        .await?
        .map(|seat| Holder::agent(&seat.name)),
    );
  }

  Ok(
    crate::supervisors::named(&ground.db, agent)
      .await
      .map_err(|why| Refusal::new(StatusCode::INTERNAL_SERVER_ERROR, "failed", why.detail))?
      .map(|supervisor| Holder::supervisor(&supervisor.name)),
  )
}

/// One MCP server of an account's: its name, the key its home copy stands under, whether it is the person's own,
/// and its text where the catalogue holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpItem {
  pub name: String,
  pub key: String,
  pub own: bool,
  pub title: Option<String>,
  pub titles: BTreeMap<String, String>,
}

// The kind of each shared key read so far: a shared item never changes its kind.
static KINDS: LazyLock<std::sync::Mutex<BTreeMap<String, String>>> = LazyLock::new(Default::default);

/// The account's MCP servers (docs/person-nodes.md, 「MCP servers」): of its personal tools as the tool host lists
/// them, each standing under an own key, and each whose shared key is a catalogue record's of the kind `mcp`, its
/// company read off the key. `texts` reads each shared one's text from the catalogue as well.
pub async fn mcp_items(ground: &Ground, account: &str, texts: bool) -> Result<Vec<McpItem>, String> {
  let tools = listed(ground, account).await?;

  Ok(mcp_items_of(ground, &tools, texts).await)
}

// The account's personal tools as the tool host lists them, or why they were not read.
async fn listed(ground: &Ground, account: &str) -> Result<Vec<Value>, String> {
  crate::personal_apps::listed_tools(ground, account)
    .await
    .map_err(|fault| Refused::from(fault).error)
}

// The MCP servers among the personal tools listed.
async fn mcp_items_of(ground: &Ground, tools: &[Value], texts: bool) -> Vec<McpItem> {
  let mut items = Vec::new();

  for tool in tools {
    let (Some(name), Some(key)) = (tool["name"].as_str(), tool["app"].as_str()) else {
      continue;
    };

    if key.starts_with(OWN_KEY_HEAD) {
      items.push(McpItem {
        name: name.to_owned(),
        key: key.to_owned(),
        own: true,
        title: first_title(tool),
        titles: BTreeMap::new(),
      });
      continue;
    }

    let Some(company) = key.strip_prefix(SHARED_KEY_HEAD).and_then(|rest| rest.split('-').next()) else {
      continue;
    };
    let known = KINDS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(key).cloned();

    if known.as_deref().is_some_and(|kind| kind != MCP) {
      continue;
    }

    if known.is_some() && !texts {
      items.push(McpItem {
        name: name.to_owned(),
        key: key.to_owned(),
        own: false,
        title: None,
        titles: BTreeMap::new(),
      });
      continue;
    }

    let Some(record) = crate::catalogue::record_of_key(ground, company, key).await else {
      continue;
    };

    KINDS
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .insert(key.to_owned(), record.kind.clone());

    if record.kind == MCP {
      items.push(McpItem {
        name: name.to_owned(),
        key: key.to_owned(),
        own: false,
        title: Some(record.title),
        titles: record.titles,
      });
    }
  }

  items
}

fn first_title(tool: &Value) -> Option<String> {
  tool["entries"][0]["title"].as_str().map(str::to_owned)
}

/// Agency's own record of the account's MCP servers (docs/person-nodes.md, 「MCP servers」): each server's name and
/// the key its home copy stands under.
pub async fn recorded_servers<C: ConnectionTrait>(
  db: &C,
  account: &str,
) -> Result<BTreeMap<String, String>, store::Fault> {
  all(
    db,
    r#"SELECT "name", "key" FROM "mcp_servers" WHERE "account" = $1"#,
    vec![account.into()],
  )
  .await?
  .iter()
  .map(|row| Ok((row.try_get("", "name")?, row.try_get("", "key")?)))
  .collect()
}

/// The account's MCP server of the name recorded under its key, as it is put at home.
pub async fn record_server<C: ConnectionTrait>(
  db: &C,
  account: &str,
  name: &str,
  key: &str,
) -> Result<(), store::Fault> {
  run(
    db,
    r#"INSERT INTO "mcp_servers" ("account", "name", "key", "recorded_at") VALUES ($1, $2, $3, $4)
       ON CONFLICT ("account", "name") DO UPDATE SET "key" = EXCLUDED."key", "recorded_at" = EXCLUDED."recorded_at""#,
    vec![account.into(), name.into(), key.into(), crate::now_ms().into()],
  )
  .await?;
  Ok(())
}

/// The account's MCP server of the name taken out of the record, as it leaves home or an app stands in its name.
pub async fn forget_server<C: ConnectionTrait>(
  db: &C,
  account: &str,
  name: &str,
) -> Result<(), store::Fault> {
  run(
    db,
    r#"DELETE FROM "mcp_servers" WHERE "account" = $1 AND "name" = $2"#,
    vec![account.into(), name.into()],
  )
  .await?;
  Ok(())
}

// The record made to agree with the tool host's listing: every server listed written, and every row whose key the
// listing holds no tool of taken away.
async fn reconcile(
  ground: &Ground,
  account: &str,
  recorded: &mut BTreeMap<String, String>,
  tools: &[Value],
  items: &[McpItem],
) -> Result<(), store::Fault> {
  let keys: BTreeSet<&str> = tools.iter().filter_map(|tool| tool["app"].as_str()).collect();
  let gone: Vec<String> = recorded
    .iter()
    .filter(|(_, key)| !keys.contains(key.as_str()))
    .map(|(name, _)| name.clone())
    .collect();

  for name in gone {
    forget_server(&ground.db, account, &name).await?;
    recorded.remove(&name);
  }

  for item in items {
    if recorded.get(&item.name) != Some(&item.key) {
      record_server(&ground.db, account, &item.name, &item.key).await?;
      recorded.insert(item.name.clone(), item.key.clone());
    }
  }

  Ok(())
}

/// What a record withholds of the account's MCP servers: their keys, and why the attached ones are among them where
/// they are.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WithheldMcp {
  pub keys: BTreeSet<String>,
  pub why: Option<String>,
}

/// The keys of the account's MCP servers a record withholds (docs/tools.md, 「The apps a person reaches」): every one
/// not attached to its holder, and every one where its line takes no attached MCP server or it names no holder. The
/// servers are agency's record's and the tool host's listing's together, the record made to agree with the listing
/// where it reads; where the listing or the holder's attachments cannot be read, every one the record holds, the
/// attached among them, and why. Agency's own record unread is the error, which withholds every app.
pub async fn withheld_mcp(
  ground: &Ground,
  account: &str,
  holder: Option<&Holder>,
  line: Option<&str>,
) -> Result<WithheldMcp, String> {
  let mut recorded = recorded_servers(&ground.db, account)
    .await
    .map_err(|why| format!("agency's record of the account's MCP servers was not read: {}", why.detail))?;
  let mut listed_items = Vec::new();
  let mut why = None;

  match listed(ground, account).await {
    Ok(tools) => {
      listed_items = mcp_items_of(ground, &tools, false).await;

      if let Err(fault) = reconcile(ground, account, &mut recorded, &tools, &listed_items).await {
        eprintln!(
          "agency: the record of account {account}'s MCP servers was not made to agree with the tool host: {}",
          fault.detail
        );
      }
    }
    Err(fault) => {
      why = Some(format!("the tool host's listing of the account's personal tools was not read: {fault}"));
    }
  }

  // Each server by its name and its key: a name the record and the listing key apart withholds both keys.
  let servers: BTreeSet<(String, String)> = recorded
    .into_iter()
    .chain(listed_items.into_iter().map(|item| (item.name, item.key)))
    .collect();

  if servers.is_empty() {
    return Ok(WithheldMcp::default());
  }

  let takes = line.is_some_and(|line| RUN_LINES.carries(line).takes_attached_mcp());
  let attached: BTreeSet<String> = match holder.filter(|_| takes && why.is_none()) {
    Some(holder) => match attached_to(&ground.db, holder).await {
      Ok(rows) => rows
        .into_iter()
        .filter(|(of, kind, _)| of == account && kind == MCP)
        .map(|(_, _, name)| name)
        .collect(),
      Err(fault) => {
        why = Some(format!("the holder's attachments were not read: {}", fault.detail));
        BTreeSet::new()
      }
    },
    None => BTreeSet::new(),
  };

  Ok(WithheldMcp {
    keys: servers
      .into_iter()
      .filter(|(name, _)| !attached.contains(name))
      .map(|(_, key)| key)
      .collect(),
    why,
  })
}

// One holder of the account's as its row reads, before what is attached to it.
struct Row {
  kind: &'static str,
  name: String,
  label: Option<String>,
  role: Option<String>,
  line: String,
}

// The account's holders: its supervisors, the oldest first, then its role agents by name; for the administrator's
// account, those of no account as well.
async fn rows(ground: &Ground, account: &str) -> Result<Vec<Row>, Refused> {
  let administrator = crate::account_isolation::owners_own(&ground.db, account).await?;
  let number: Option<i64> = account.parse().ok();
  let supervisors = all(
    &ground.db,
    r#"SELECT "name", "label" FROM "supervisors"
       WHERE "owner_account" = $1 OR ($2 AND "owner_account" IS NULL) ORDER BY "created_at", "name""#,
    vec![number.into(), administrator.into()],
  )
  .await?;
  let agents = all(
    &ground.db,
    r#"SELECT a."name", a."role", r."line" FROM "role_agents" a LEFT JOIN "roles" r ON r."name" = a."role"
       WHERE a."owner_account" = $1 OR ($2 AND a."owner_account" IS NULL) ORDER BY a."name""#,
    vec![number.into(), administrator.into()],
  )
  .await?;
  let mut found = Vec::new();

  for row in supervisors {
    found.push(Row {
      kind: SUPERVISOR,
      name: row.try_get("", "name").map_err(Refused::failed)?,
      label: row.try_get::<Option<String>>("", "label").ok().flatten(),
      role: None,
      line: SUPERVISOR_LINE.to_owned(),
    });
  }

  for row in agents {
    found.push(Row {
      kind: AGENT,
      name: row.try_get("", "name").map_err(Refused::failed)?,
      label: None,
      role: row.try_get::<Option<String>>("", "role").ok().flatten(),
      line: row.try_get::<Option<String>>("", "line").ok().flatten().unwrap_or_default(),
    });
  }

  Ok(found)
}

async fn holder_view(ground: &Ground, account: &str, row: &Row) -> Result<AttachmentHolder, Refused> {
  let carries = RUN_LINES.carries(&row.line);
  let mut skills = Vec::new();
  let mut mcp = Vec::new();

  for (of, kind, item) in attached_to(
    &ground.db,
    &Holder {
      kind: row.kind.to_owned(),
      name: row.name.clone(),
    },
  )
  .await?
  {
    if of != account {
      continue;
    }

    if kind == SKILL {
      skills.push(item);
    } else if kind == MCP {
      mcp.push(item);
    }
  }

  Ok(AttachmentHolder {
    kind: row.kind.to_owned(),
    name: row.name.clone(),
    label: row.label.clone(),
    role: row.role.clone(),
    line: row.line.clone(),
    takes: AttachmentTakes {
      skills: carries.takes_attached_skills(),
      mcp: carries.takes_attached_mcp(),
    },
    skills,
    mcp,
  })
}

/// The account's holders, what is attached to each, and the skills and MCP servers that may be.
pub async fn listing(ground: &Ground, account: &str) -> Result<AttachmentList, Refused> {
  let mut holders = Vec::new();

  for row in rows(ground, account).await? {
    holders.push(holder_view(ground, account, &row).await?);
  }

  let skills = crate::skills::held(&ground.roots.data, account)
    .iter()
    .map(|held| {
      let (title, titles) = held.titles();

      AttachableItem {
        name: held.name.clone(),
        title,
        titles,
        origin: held.origin().to_owned(),
      }
    })
    .collect();
  let mcp = mcp_items(ground, account, true)
    .await
    .map_err(|why| Refused::new(StatusCode::UNPROCESSABLE_ENTITY, why).with_code("app-refused"))?
    .into_iter()
    .map(|item| AttachableItem {
      name: item.name,
      title: item.title,
      titles: item.titles,
      origin: if item.own { "own" } else { "installed" }.to_owned(),
    })
    .collect();

  Ok(AttachmentList {
    holders,
    skills,
    mcp,
  })
}

/// One holder's attached skills and MCP servers set whole: refused where a list is not empty and the holder's line
/// takes none of its kind, where the holder or an item is none of the account's.
pub async fn set(
  ground: &Ground,
  account: &str,
  (holder_kind, holder): (&str, &str),
  skills: &[String],
  mcp: &[String],
) -> Result<AttachmentHolder, Refused> {
  if holder_kind != SUPERVISOR && holder_kind != AGENT {
    return Err(
      Refused::new(
        StatusCode::BAD_REQUEST,
        format!("{holder_kind} is no holder's kind: supervisor or agent"),
      )
      .with_field("holderKind"),
    );
  }

  let row = rows(ground, account)
    .await?
    .into_iter()
    .find(|row| row.kind == holder_kind && row.name == holder);
  let Some(row) = row else {
    let elsewhere = match holder_kind {
      SUPERVISOR => crate::supervisors::named(&ground.db, holder).await.ok().flatten().is_some(),
      _ => crate::role_agents::named(&ground.db, holder).await.ok().flatten().is_some(),
    };

    return Err(if elsewhere {
      Refused::new(StatusCode::FORBIDDEN, format!("the {holder_kind} {holder} is not the account's"))
        .with_code("forbidden")
    } else {
      Refused::new(StatusCode::NOT_FOUND, format!("no {holder_kind} {holder} stands"))
        .with_code("not-found")
    });
  };
  let carries = RUN_LINES.carries(&row.line);

  for (kind, list, takes) in [
    (SKILL, skills, carries.takes_attached_skills()),
    (MCP, mcp, carries.takes_attached_mcp()),
  ] {
    if !list.is_empty() && !takes {
      return Err(
        Refused::new(
          StatusCode::CONFLICT,
          format!("the {} line of {holder} takes no attached {kind}", row.line),
        )
        .with_code("line-takes-none")
        .with_field(if kind == SKILL { "skills" } else { "mcp" }),
      );
    }
  }

  let held_skills: BTreeSet<String> = crate::skills::held(&ground.roots.data, account)
    .into_iter()
    .map(|held| held.name)
    .collect();

  if let Some(missing) = skills.iter().find(|name| !held_skills.contains(*name)) {
    return Err(
      Refused::new(StatusCode::NOT_FOUND, format!("the account holds no skill {missing}"))
        .with_code("not-found"),
    );
  }

  if !mcp.is_empty() {
    let held_mcp: BTreeSet<String> = mcp_items(ground, account, false)
      .await
      .map_err(|why| Refused::new(StatusCode::UNPROCESSABLE_ENTITY, why).with_code("app-refused"))?
      .into_iter()
      .map(|item| item.name)
      .collect();

    if let Some(missing) = mcp.iter().find(|name| !held_mcp.contains(*name)) {
      return Err(
        Refused::new(StatusCode::NOT_FOUND, format!("the account holds no MCP server {missing}"))
          .with_code("not-found"),
      );
    }
  }

  let now = crate::now_ms();

  run(
    &ground.db,
    r#"DELETE FROM "agent_attachments" WHERE "account" = $1 AND "holder_kind" = $2 AND "holder" = $3"#,
    vec![account.into(), holder_kind.into(), holder.into()],
  )
  .await?;

  for (kind, list) in [(SKILL, skills), (MCP, mcp)] {
    for item in list.iter().collect::<BTreeSet<_>>() {
      run(
        &ground.db,
        r#"INSERT INTO "agent_attachments" ("account", "holder_kind", "holder", "item_kind", "item", "attached_at")
           VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING"#,
        vec![
          account.into(),
          holder_kind.into(),
          holder.into(),
          kind.into(),
          item.clone().into(),
          now.into(),
        ],
      )
      .await?;
    }
  }

  holder_view(ground, account, &row).await
}
