//! A person's personal apps shared within their company (docs/person-nodes.md, 「Shared apps」): the company's
//! catalogue at home, kept by `bmo-pocket catalogue` (the tool host's `docs/catalogue.md`), read for each reader as the
//! delegation chain lets them see it; a share judged against the maker's ceiling and reviewed; a copy installed,
//! moved, dropped and adopted on the receiver's PC through its runner — a skill at home, with no PC
//! (docs/person-nodes.md, 「Skills at home」); an app turned on and recalled for a unit; an item's text set or filled
//! in other languages; and each person's copies kept to what the chain and the catalogue say as their PC is heard.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, LazyLock};

use axum::http::StatusCode;
use bmo_agency_client::{
  CatalogueApp, CatalogueApproval, CatalogueCopy, CatalogueList, CatalogueMay, CatalogueNotice,
  CatalogueReview, CatalogueServer, CatalogueSource, CatalogueVersion, PersonalAppPermissions,
};
use policy::organisation::ORGANISATION;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::access::Acting;
use crate::chain::{self, Subject};
use crate::control::Refused;
use crate::personal_apps::{self, Fault as AppFault};
use crate::places::Ground;

/// Where a share stages the bundle it hands the catalogue, under the data root.
const STAGING: &str = "agency/catalogue";

/// The targets a share names where it names none.
const DEFAULT_TARGETS: [&str; 2] = ["team", "heads"];

/// The kinds of item the catalogue keeps.
pub const APP: &str = "app";
pub const SKILL: &str = "skill";
pub const MCP: &str = "mcp";

/// The locales an item's text is written in, each with the language's name a translation is asked in.
pub const LOCALES: [(&str, &str); 4] =
  [("ko", "Korean"), ("en", "English"), ("ja", "Japanese"), ("zh-CN", "Simplified Chinese")];

/// The maker's language where a share names none.
const DEFAULT_LANGUAGE: &str = "en";

/// Why a catalogue's ask was not done.
#[derive(Debug)]
pub enum Fault {
  App(AppFault),
  ScopeWithheld(String),
  OtherCompany(String),
  InstallWithheld,
  NotFound(String),
  Withdrawn(String),
  AwaitingApproval(String),
  NotOffered(String),
  NameTaken(String),
  Skill(crate::skills::Fault),
  Locked(String),
  Forbidden(String),
  BadRequest(String),
  Unchecked,
}

impl From<AppFault> for Fault {
  fn from(fault: AppFault) -> Self {
    match fault {
      AppFault::Refused(why) if why.contains("that is no copy of") => Self::NameTaken(why),
      other => Self::App(other),
    }
  }
}

impl From<crate::skills::Fault> for Fault {
  fn from(fault: crate::skills::Fault) -> Self {
    match fault {
      crate::skills::Fault::NameTaken(why) => Self::NameTaken(why),
      crate::skills::Fault::App(fault) => Self::App(fault),
      other => Self::Skill(other),
    }
  }
}

impl From<Fault> for Refused {
  fn from(fault: Fault) -> Self {
    match fault {
      Fault::App(fault) => fault.into(),
      Fault::ScopeWithheld(scope) => Refused::new(
        StatusCode::FORBIDDEN,
        format!("The person's organisation does not let them share with {scope}"),
      )
      .with_code("scope-withheld"),
      Fault::OtherCompany(what) => {
        Refused::new(StatusCode::FORBIDDEN, format!("{what} stands outside the person's company"))
          .with_code("other-company")
      }
      Fault::InstallWithheld => Refused::new(
        StatusCode::FORBIDDEN,
        "The person's organisation does not let them install apps shared with them",
      )
      .with_code("install-withheld"),
      Fault::NotFound(what) => Refused::new(StatusCode::NOT_FOUND, what).with_code("not-found"),
      Fault::Withdrawn(id) => {
        Refused::new(StatusCode::CONFLICT, format!("the app {id} was withdrawn")).with_code("withdrawn")
      }
      Fault::AwaitingApproval(id) => Refused::new(
        StatusCode::CONFLICT,
        format!("the newest version of {id} waits for a head's approval"),
      )
      .with_code("awaiting-approval"),
      Fault::NotOffered(id) => Refused::new(
        StatusCode::FORBIDDEN,
        format!("the person's organisation does not offer the app {id} to them"),
      )
      .with_code("not-offered"),
      Fault::NameTaken(why) => Refused::new(StatusCode::CONFLICT, why).with_code("name-taken"),
      Fault::Skill(fault) => fault.into(),
      Fault::Locked(why) => Refused::new(StatusCode::CONFLICT, why).with_code("locked"),
      Fault::Forbidden(why) => Refused::new(StatusCode::FORBIDDEN, why).with_code("forbidden"),
      Fault::BadRequest(why) => Refused::new(StatusCode::BAD_REQUEST, why),
      Fault::Unchecked => crate::organisation::unchecked(),
    }
  }
}

/// What a version asks for beyond the app's own folders.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Asks {
  #[serde(default)]
  pub network: Vec<String>,
  #[serde(default)]
  pub secrets: Vec<String>,
}

impl Asks {
  /// Whether this asks for anything `before` did not.
  pub fn grows_over(&self, before: &Self) -> bool {
    self.network.iter().any(|one| !before.network.contains(one))
      || self.secrets.iter().any(|one| !before.secrets.contains(one))
  }

  fn wire(&self) -> PersonalAppPermissions {
    PersonalAppPermissions {
      network: self.network.clone(),
      secrets: self.secrets.clone(),
    }
  }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeptApproval {
  required: bool,
  #[serde(default)]
  by: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeptVersion {
  version: String,
  source_hash: String,
  permissions: Asks,
  shared_at: i64,
  review: Value,
  approval: KeptApproval,
}

impl KeptVersion {
  fn approved(&self) -> bool {
    !self.approval.required || self.approval.by.is_some()
  }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeptCopy {
  version: String,
  how: String,
}

/// An app's record as the catalogue keeps it (the tool host's `docs/catalogue.md`, 「An app's record」).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Kept {
  id: String,
  #[serde(default = "app_kind")]
  kind: String,
  key: String,
  name: String,
  title: String,
  description: String,
  #[serde(default)]
  language: Option<String>,
  #[serde(default)]
  titles: BTreeMap<String, String>,
  #[serde(default)]
  descriptions: BTreeMap<String, String>,
  #[serde(default)]
  machine: Vec<String>,
  maker: String,
  owner: String,
  unit: String,
  targets: Value,
  versions: Vec<KeptVersion>,
  #[serde(default)]
  withdrawn: Option<i64>,
  #[serde(default)]
  installs: BTreeMap<String, KeptCopy>,
  #[serde(default)]
  promotion: Option<Value>,
}

fn app_kind() -> String {
  APP.to_owned()
}

/// What the catalogue's record of a key says of its item: its kind and its text.
#[derive(Clone, Debug)]
pub struct Facts {
  pub kind: String,
  pub title: String,
  pub titles: BTreeMap<String, String>,
}

/// The record of the company's catalogue standing for the key, where one does.
pub async fn record_of_key(ground: &Ground, company: &str, key: &str) -> Option<Facts> {
  let company: i64 = company.parse().ok()?;
  let (apps, _) = listed(ground, company, None).await.ok()?;

  apps.into_iter().find(|app| app.key == key).map(|app| Facts {
    kind: app.kind,
    title: app.title,
    titles: app.titles,
  })
}

impl Kept {
  fn latest(&self) -> Option<&KeptVersion> {
    self.versions.last()
  }

  fn version(&self, name: &str) -> Option<&KeptVersion> {
    self.versions.iter().find(|one| one.version == name)
  }

  fn unit(&self) -> Option<i64> {
    self.unit.parse().ok()
  }

  fn setting(&self, setting: &str) -> String {
    format!("app.{}.{setting}", self.key)
  }

  fn targets(&self) -> Vec<Value> {
    self.targets.as_array().cloned().unwrap_or_default()
  }
}

/// Where a person stands: their company, the units from it down to their home unit, and the targets their chain
/// lets them share with.
#[derive(Clone, Debug)]
pub struct Place {
  pub person: i64,
  pub company: i64,
  pub units: Vec<i64>,
  pub ceiling: Vec<String>,
}

impl Place {
  fn home(&self) -> i64 {
    self.units.last().copied().unwrap_or(self.company)
  }
}

/// Who reads a catalogue: a person of the company, or the administrator, naming the company, and a member of it as
/// well where it is the company their home unit stands in, `member` their place there.
#[derive(Clone, Debug)]
pub enum Reader {
  Person(Place),
  Administrator {
    account: i64,
    company: i64,
    member: Option<Place>,
  },
}

impl Reader {
  /// Where the reader stands as a member of the company read: a person's place, or the administrator's in their own
  /// company; none for the administrator reading another.
  pub fn member(&self) -> Option<&Place> {
    match self {
      Self::Person(place) => Some(place),
      Self::Administrator { member, .. } => member.as_ref(),
    }
  }

  fn company(&self) -> i64 {
    match self {
      Self::Person(place) => place.company,
      Self::Administrator { company, .. } => *company,
    }
  }

  fn person(&self) -> i64 {
    match self {
      Self::Person(place) => place.person,
      Self::Administrator { account, .. } => *account,
    }
  }

  fn account(&self) -> String {
    self.person().to_string()
  }
}

fn units_in(path: &[String]) -> Vec<i64> {
  path.iter().filter_map(|link| link.strip_prefix("unit:")?.parse().ok()).collect()
}

fn key_of_scope() -> &'static str {
  ORGANISATION.personal_apps.share_scope.key.as_str()
}

/// Where the person stands, as their chain's path for the share scope names it, and the scope it resolves.
pub async fn place_of(ground: &Ground, person: i64) -> Result<Place, Fault> {
  let answer = chain::resolve(&crate::apps::service(ground), key_of_scope(), Subject::Person(person))
    .await
    .map_err(|_| Fault::Unchecked)?
    .ok_or(Fault::Unchecked)?;
  let units = units_in(&chain::path_of(&answer));
  let company = *units.first().ok_or_else(|| Fault::OtherCompany(format!("the person {person}")))?;
  let ceiling = answer["value"]
    .as_array()
    .map(|keys| keys.iter().filter_map(Value::as_str).map(str::to_owned).collect())
    .unwrap_or_else(|| DEFAULT_TARGETS.iter().map(|key| (*key).to_owned()).collect());

  Ok(Place {
    person,
    company,
    units,
    ceiling,
  })
}

// The units from a unit's company down to it, as its chain's path names them.
async fn unit_path(ground: &Ground, unit: i64) -> Result<Vec<i64>, Fault> {
  let answer = chain::resolve(&crate::apps::service(ground), key_of_scope(), Subject::Unit(unit))
    .await
    .map_err(|_| Fault::Unchecked)?
    .ok_or_else(|| Fault::NotFound(format!("no unit {unit} stands")))?;

  Ok(units_in(&chain::path_of(&answer)))
}

/// The reader a call names: the administrator for the company asked, else the person where they stand.
pub async fn reader(ground: &Ground, account: &str, company: Option<i64>) -> Result<Reader, Fault> {
  let person: i64 = account.parse().map_err(|_| Fault::BadRequest(format!("{account:?} is no account")))?;

  if crate::account_isolation::owners_own(&ground.db, account)
    .await
    .map_err(|_| Fault::Unchecked)?
  {
    // A company named is read, and none named reads as an empty one; where it is the company the administrator's home
    // unit stands in, they read it as a member of it as well.
    let company = company.unwrap_or(0);
    let member = if company == 0 {
      None
    } else {
      place_of(ground, person).await.ok().filter(|place| place.company == company)
    };

    return Ok(Reader::Administrator {
      account: person,
      company,
      member,
    });
  }

  place_of(ground, person).await.map(Reader::Person)
}

// The words of `bmo-pocket catalogue` answered as JSON.
async fn kept_json(ground: &Ground, words: &[&str]) -> Result<Value, Fault> {
  let mut all = vec!["catalogue"];

  all.extend_from_slice(words);

  let said = personal_apps::pocket(ground, &all).await?;

  serde_json::from_str(said.trim())
    .map_err(|why| Fault::App(AppFault::Refused(format!("the catalogue's answer does not read: {why}"))))
}

async fn listed(
  ground: &Ground,
  company: i64,
  reader: Option<&str>,
) -> Result<(Vec<Kept>, Option<i64>), Fault> {
  let company = company.to_string();
  let mut words = vec!["list", "--company", company.as_str()];

  if let Some(reader) = reader {
    words.extend(["--reader", reader]);
  }

  let read = kept_json(ground, &words).await?;
  let apps = read["apps"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|app| serde_json::from_value(app.clone()).ok())
    .collect();

  Ok((apps, read["seenAt"].as_i64()))
}

async fn one(ground: &Ground, company: i64, id: &str) -> Result<Kept, Fault> {
  let company = company.to_string();
  let read = kept_json(ground, &["show", "--company", &company, id])
    .await
    .map_err(|_| Fault::NotFound(format!("the catalogue holds no app {id}")))?;

  serde_json::from_value(read)
    .map_err(|why| Fault::App(AppFault::Refused(format!("the app {id}'s record does not read: {why}"))))
}

// What a reader's judgement asks of the chain more than once in one reading, kept for that reading alone.
#[derive(Default)]
struct Asked {
  heads: BTreeMap<i64, bool>,
  groups: BTreeMap<String, bool>,
  flags: BTreeMap<String, Option<Value>>,
}

impl Asked {
  // Whether the person heads the unit, as the chain answers a unit's link to its writers alone.
  async fn heads(&mut self, ground: &Ground, person: i64, unit: i64) -> bool {
    if let Some(known) = self.heads.get(&unit) {
      return *known;
    }

    let heads = crate::organisation::heads(ground, Acting::Person(person), key_of_scope(), unit)
      .await
      .unwrap_or(false);

    self.heads.insert(unit, heads);
    heads
  }

  // Whether the person is one of the group's members, as the chain answers the group to its company.
  async fn in_group(&mut self, ground: &Ground, person: i64, group: &str) -> bool {
    if let Some(known) = self.groups.get(group) {
      return *known;
    }

    let read =
      chain::read(&crate::apps::service(ground), &format!("/chain/groups/{group}"), Some(person)).await;
    let member = read.is_ok_and(|read| {
      read.ok()
        && read.body["members"]
          .as_array()
          .is_some_and(|members| members.iter().any(|one| one.as_i64() == Some(person)))
    });

    self.groups.insert(group.to_owned(), member);
    member
  }

  // A kind resolved for the person, its answer whole.
  async fn resolved(&mut self, ground: &Ground, person: i64, key: &str) -> Option<Value> {
    if let Some(known) = self.flags.get(key) {
      return known.clone();
    }

    let answer = chain::resolve(&crate::apps::service(ground), key, Subject::Person(person))
      .await
      .ok()
      .flatten();

    self.flags.insert(key.to_owned(), answer.clone());
    answer
  }

  async fn flag(&mut self, ground: &Ground, person: i64, key: &str, fallback: bool) -> bool {
    chain::flag(self.resolved(ground, person, key).await.as_ref(), fallback)
  }
}

// Whether a target takes the person in.
async fn takes_in(ground: &Ground, asked: &mut Asked, place: &Place, app: &Kept, target: &Value) -> bool {
  let maker_unit = app.unit().unwrap_or_default();

  if target["company"] == true {
    return true;
  }

  if target["team"] == true {
    return place.units.contains(&maker_unit);
  }

  if target["heads"] == true {
    return asked.heads(ground, place.person, maker_unit).await;
  }

  if let Some(person) = target["person"].as_i64().or_else(|| target["person"].as_str()?.parse().ok()) {
    return person == place.person;
  }

  if let Some(unit) = target["unit"].as_i64().or_else(|| target["unit"].as_str()?.parse().ok()) {
    return place.units.contains(&unit);
  }

  if let Some(group) = target["group"]
    .as_i64()
    .map(|id| id.to_string())
    .or_else(|| target["group"].as_str().map(str::to_owned))
  {
    return asked.in_group(ground, place.person, &group).await;
  }

  false
}

// Whether the reader sees the app: its maker, its owner, a head of the maker's home unit, whoever a target takes in,
// whoever holds a copy, and the administrator.
async fn sees(ground: &Ground, asked: &mut Asked, reader: &Reader, app: &Kept) -> bool {
  let Reader::Person(place) = reader else { return true };
  let account = place.person.to_string();

  if app.maker == account || app.owner == account || app.installs.contains_key(&account) {
    return true;
  }

  if asked.heads(ground, place.person, app.unit().unwrap_or_default()).await {
    return true;
  }

  for target in app.targets() {
    if takes_in(ground, asked, place, app, &target).await {
      return true;
    }
  }

  false
}

// The units on the reader's path they head, as the links a turning on or a recall is written at.
async fn headed_links(ground: &Ground, asked: &mut Asked, reader: &Reader) -> Vec<String> {
  match reader {
    Reader::Administrator { .. } => vec!["platform".to_owned()],
    Reader::Person(place) => {
      let mut links = Vec::new();

      for unit in &place.units {
        if asked.heads(ground, place.person, *unit).await {
          links.push(format!("unit:{unit}"));
        }
      }

      links
    }
  }
}

fn review_of(value: &Value) -> CatalogueReview {
  CatalogueReview {
    state: value["state"].as_str().unwrap_or("pending").to_owned(),
    report: value["report"].as_str().map(str::to_owned),
    at: value["at"].as_i64(),
  }
}

// The app as the reader sees it, or none where they do not.
async fn view(
  ground: &Ground,
  asked: &mut Asked,
  reader: &Reader,
  app: &Kept,
  headed: &[String],
) -> Option<CatalogueApp> {
  let latest = app.latest()?;

  if !sees(ground, asked, reader, app).await {
    return None;
  }

  let account = reader.account();
  let person = reader.person();
  let administrator = matches!(reader, Reader::Administrator { .. });
  let copy = app.installs.get(&account);
  let from = copy.and_then(|copy| app.version(&copy.version));
  let update_waiting = copy.is_some_and(|copy| copy.version != latest.version);
  let grown = from.is_some_and(|from| latest.permissions.grows_over(&from.permissions));
  let maker_unit = app.unit().unwrap_or_default();
  let heads_maker = !administrator && asked.heads(ground, person, maker_unit).await;
  let open = app.withdrawn.is_none() && latest.approved();
  let mut may = CatalogueMay::default();

  if reader.member().is_some() {
    let install = asked.flag(ground, person, ORGANISATION.personal_apps.install.key.as_str(), true).await;
    let offered = asked.flag(ground, person, &app.setting("offered"), true).await;

    may.install = copy.is_none() && app.maker != account && open && install && offered;
    may.update = update_waiting && latest.approved();
    may.uninstall = copy.is_some();
    may.clone = copy.is_some();
  }

  if !administrator {
    may.approve = latest.approval.required && latest.approval.by.is_none() && heads_maker;
    may.promote = app.maker == account || app.owner == account || heads_maker;
  }

  may.withdraw = app.withdrawn.is_none() && (administrator || app.maker == account || app.owner == account);
  may.text = app.maker == account || app.owner == account;
  may.translate = may.text || heads_maker;
  may.recall = headed.to_vec();
  may.turn_on = if open && !administrator { headed.to_vec() } else { Vec::new() };

  let server = if app.kind == MCP {
    server_of(ground, reader.company(), &app.id, &latest.version).await
  } else {
    None
  };

  Some(CatalogueApp {
    id: app.id.clone(),
    kind: app.kind.clone(),
    key: app.key.clone(),
    name: app.name.clone(),
    title: app.title.clone(),
    description: app.description.clone(),
    language: app.language.clone(),
    titles: app.titles.clone(),
    descriptions: app.descriptions.clone(),
    machine: app.machine.clone(),
    server,
    maker: app.maker.clone(),
    owner: app.owner.clone(),
    targets: app.targets(),
    version: latest.version.clone(),
    versions: app
      .versions
      .iter()
      .map(|one| CatalogueVersion {
        version: one.version.clone(),
        shared_at: one.shared_at,
        source_hash: one.source_hash.clone(),
        permissions: one.permissions.wire(),
        review: review_of(&one.review),
      })
      .collect(),
    permissions: latest.permissions.wire(),
    review: review_of(&latest.review),
    approval: CatalogueApproval {
      required: latest.approval.required,
      approved: latest.approval.by.is_some(),
    },
    installs: app.installs.len() as i64,
    withdrawn: app.withdrawn,
    promotion: app.promotion.clone(),
    installed: copy.map(|copy| CatalogueCopy {
      version: copy.version.clone(),
      how: copy.how.clone(),
    }),
    update_waiting,
    permissions_grown: grown,
    may,
  })
}

// How each MCP server's version is reached, read once off its source's `mcp.toml`: a version's source never changes.
static SERVERS: LazyLock<std::sync::Mutex<BTreeMap<String, Option<CatalogueServer>>>> =
  LazyLock::new(Default::default);

// The `[server]` of an MCP server's `mcp.toml`, as the console shows how it is reached.
#[derive(Deserialize)]
struct McpFile {
  server: McpServer,
}

#[derive(Deserialize)]
struct McpServer {
  transport: String,
  #[serde(default)]
  url: Option<String>,
  #[serde(default)]
  auth: Option<String>,
  #[serde(default)]
  header: Option<String>,
  #[serde(default)]
  secret: Option<String>,
  #[serde(default)]
  command: Option<Vec<String>>,
}

/// How an MCP server's `mcp.toml` says it is reached, where it reads.
pub fn server_in(mcp_toml: &str) -> Option<CatalogueServer> {
  let read: McpFile = toml::from_str(mcp_toml).ok()?;
  let server = read.server;

  Some(CatalogueServer {
    transport: server.transport,
    url: server.url,
    auth: server.auth,
    header: server.header,
    secret: server.secret,
    command: server.command,
  })
}

async fn server_of(ground: &Ground, company: i64, id: &str, version: &str) -> Option<CatalogueServer> {
  let held = format!("{company}/{id}@{version}");

  if let Some(known) = SERVERS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&held) {
    return known.clone();
  }

  let company = company.to_string();
  let source = kept_json(ground, &["source", "--company", &company, id, "--version", version])
    .await
    .ok()?;
  let server = source["files"]["mcp.toml"].as_str().and_then(server_in);

  SERVERS
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner)
    .insert(held, server.clone());
  server
}

// Every app whose owner left passed to a head of the maker's home unit, the nearest unit up its line holding one
// who has not left; an app whose maker no longer stands in that unit, or whose unit cannot be read, stays.
async fn pass_departed(ground: &Ground, company: i64, apps: &mut [Kept]) {
  let service = crate::apps::service(ground);
  let mut leavers: BTreeMap<String, Option<String>> = BTreeMap::new();

  for app in apps.iter() {
    if app.owner != app.maker || leavers.contains_key(&app.owner) {
      continue;
    }

    let Some(unit) = app.unit() else { continue };
    let Ok(read) = chain::read(&service, &format!("/units/{unit}"), None).await else {
      continue;
    };
    let left = read.body["left"].as_array().is_some_and(|left| {
      left.iter().any(|one| one.as_i64().map(|id| id.to_string()) == Some(app.owner.clone()))
    });

    if !left {
      continue;
    }

    let mut heir = None;
    let mut line: Vec<i64> = read.body["path"]
      .as_array()
      .into_iter()
      .flatten()
      .filter_map(|step| step["id"].as_i64().or_else(|| step.as_i64()))
      .collect();

    if !line.contains(&unit) {
      line.push(unit);
    }

    for step in line.iter().rev() {
      let Ok(step_read) = chain::read(&service, &format!("/units/{step}"), None).await else {
        continue;
      };
      let gone: BTreeSet<i64> = step_read.body["left"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .collect();
      let found = step_read.body["heads"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .find(|head| !gone.contains(head) && head.to_string() != app.owner);

      if let Some(head) = found {
        heir = Some(head.to_string());
        break;
      }
    }

    leavers.insert(app.owner.clone(), heir);
  }

  let company = company.to_string();

  for (leaver, heir) in leavers {
    let Some(heir) = heir else { continue };

    if kept_json(ground, &["pass", "--company", &company, "--from", &leaver, "--to", &heir])
      .await
      .is_ok()
    {
      for app in apps.iter_mut().filter(|app| app.owner == leaver) {
        app.owner = heir.clone();
      }
    }
  }
}

/// The reader's company's catalogue as they see it.
pub async fn list(ground: &Ground, reader: &Reader) -> Result<CatalogueList, Fault> {
  let account = reader.account();
  let (mut apps, seen_at) = listed(ground, reader.company(), Some(&account)).await?;

  pass_departed(ground, reader.company(), &mut apps).await;

  let mut asked = Asked::default();
  let headed = headed_links(ground, &mut asked, reader).await;
  let mut seen = Vec::new();

  for app in &apps {
    if let Some(view) = view(ground, &mut asked, reader, app, &headed).await {
      seen.push(view);
    }
  }

  let since = seen_at.unwrap_or(0);
  let mut notices = Vec::new();

  for app in &seen {
    let at = app.versions.last().map_or(0, |version| version.shared_at);

    if app.may.install && app.maker != account && at > since {
      notices.push(CatalogueNotice {
        kind: "shared".to_owned(),
        app: app.id.clone(),
        at,
      });
    }

    if app.may.update && app.permissions_grown {
      notices.push(CatalogueNotice {
        kind: "update".to_owned(),
        app: app.id.clone(),
        at,
      });
    }

    if app.may.approve {
      notices.push(CatalogueNotice {
        kind: "approval".to_owned(),
        app: app.id.clone(),
        at,
      });
    }
  }

  notices.sort_by_key(|notice| std::cmp::Reverse(notice.at));

  Ok(CatalogueList {
    company: reader.company().to_string(),
    ceiling: match reader {
      Reader::Person(place) => place.ceiling.clone(),
      Reader::Administrator { .. } => Vec::new(),
    },
    apps: seen,
    notices,
    seen_at,
  })
}

// The app as the reader sees it, or not found where they do not.
async fn seen_app(ground: &Ground, reader: &Reader, id: &str) -> Result<(Kept, CatalogueApp), Fault> {
  let app = one(ground, reader.company(), id).await?;
  let mut asked = Asked::default();
  let headed = headed_links(ground, &mut asked, reader).await;
  let view = view(ground, &mut asked, reader, &app, &headed)
    .await
    .ok_or_else(|| Fault::NotFound(format!("the catalogue holds no app {id}")))?;

  Ok((app, view))
}

/// One app as the reader sees it.
pub async fn entry(ground: &Ground, reader: &Reader, id: &str) -> Result<CatalogueApp, Fault> {
  seen_app(ground, reader, id).await.map(|(_, view)| view)
}

/// A version's source, for a reader who sees the app.
pub async fn source(
  ground: &Ground,
  reader: &Reader,
  id: &str,
  version: Option<&str>,
) -> Result<CatalogueSource, Fault> {
  seen_app(ground, reader, id).await?;

  let company = reader.company().to_string();
  let mut words = vec!["source", "--company", company.as_str(), id];

  if let Some(version) = version {
    words.extend(["--version", version]);
  }

  let read = kept_json(ground, &words).await?;

  serde_json::from_value(read)
    .map_err(|why| Fault::App(AppFault::Refused(format!("the source does not read: {why}"))))
}

// What a target needs of the maker's ceiling: each scope it reaches beyond, refused where it stands outside the
// maker's company.
async fn needs(
  ground: &Ground,
  asked: &mut Asked,
  maker: &Place,
  target: &Value,
) -> Result<BTreeSet<String>, Fault> {
  let one = |scope: &str| Ok(BTreeSet::from([scope.to_owned()]));
  let home = maker.home();

  for scope in ["team", "heads", "company"] {
    if target[scope] == true {
      return one(scope);
    }
  }

  let number = |field: &str| target[field].as_i64().or_else(|| target[field].as_str()?.parse().ok());

  if let Some(person) = number("person") {
    let theirs = place_of(ground, person)
      .await
      .map_err(|_| Fault::OtherCompany(format!("the person {person}")))?;

    if theirs.company != maker.company {
      return Err(Fault::OtherCompany(format!("the person {person}")));
    }

    if theirs.home() == home {
      return one("team");
    }

    let heads = crate::organisation::heads(ground, Acting::Person(person), key_of_scope(), home)
      .await
      .unwrap_or(false);

    return one(if heads { "heads" } else { "company" });
  }

  if let Some(unit) = number("unit") {
    let path = unit_path(ground, unit).await?;

    if path.first() != Some(&maker.company) {
      return Err(Fault::OtherCompany(format!("the unit {unit}")));
    }

    return one(if path.contains(&home) { "team" } else { "company" });
  }

  if let Some(group) = number("group") {
    let read =
      chain::read(&crate::apps::service(ground), &format!("/chain/groups/{group}"), Some(maker.person))
        .await
        .map_err(|_| Fault::Unchecked)?;

    if !read.ok() {
      return Err(Fault::OtherCompany(format!("the group {group}")));
    }

    let mut all = BTreeSet::new();

    for member in read.body["members"].as_array().into_iter().flatten().filter_map(Value::as_i64) {
      let target = json!({ "person": member });

      all.extend(Box::pin(needs(ground, asked, maker, &target)).await?);
    }

    return Ok(all);
  }

  Err(Fault::BadRequest(format!(
    "{target} is no target: team, heads, company, a person, a group or a unit"
  )))
}

// A bundle staged under the data root for the catalogue to read; taken away after.
struct Staged(std::path::PathBuf);

impl Drop for Staged {
  fn drop(&mut self) {
    let _ = std::fs::remove_file(&self.0);
  }
}

/// One of the maker's items shared with the targets named: judged against their ceiling, its version in place read
/// with the source it was placed from — an app's or an MCP server's from their PC, a skill's from their skills at home
/// — kept in the catalogue under the maker's language, `en` where none is named, and reviewed after.
pub async fn share(
  ground: &Arc<Ground>,
  reader: &Reader,
  name: &str,
  targets: &[Value],
  (kind, language): (Option<&str>, Option<&str>),
) -> Result<CatalogueApp, Fault> {
  let Reader::Person(maker) = reader else {
    return Err(Fault::Forbidden("the administrator shares no personal app".to_owned()));
  };
  let kind = kind.map(str::trim).filter(|kind| !kind.is_empty()).unwrap_or(APP);
  let language = language
    .map(str::trim)
    .filter(|language| !language.is_empty())
    .unwrap_or(DEFAULT_LANGUAGE);

  match kind {
    APP | MCP if personal_apps::app_name(name) => {}
    SKILL if crate::skills::skill_name(name) => {}
    APP | MCP | SKILL => return Err(Fault::BadRequest(format!("{name:?} is no {kind}'s name"))),
    other => return Err(Fault::BadRequest(format!("{other} is no kind: app, skill or mcp"))),
  }

  if !LOCALES.iter().any(|(locale, _)| *locale == language) {
    return Err(Fault::BadRequest(format!("{language} is no language: ko, en, ja or zh-CN")));
  }

  personal_apps::may_make(ground, &reader.account()).await?;

  let targets: Vec<Value> = if targets.is_empty() {
    DEFAULT_TARGETS.iter().map(|scope| json!({ *scope: true })).collect()
  } else {
    targets.to_vec()
  };
  let mut asked = Asked::default();

  for target in &targets {
    for scope in needs(ground, &mut asked, maker, target).await? {
      if !maker.ceiling.contains(&scope) {
        return Err(Fault::ScopeWithheld(scope));
      }
    }
  }

  let account = reader.account();
  let (version, files) = if kind == SKILL {
    let (version, files) = crate::skills::own_source(&ground.roots.data, &account, name)?;

    (version, json!(files))
  } else {
    pc_source(ground, &account, name).await?
  };

  let review_required = asked
    .flag(ground, maker.person, ORGANISATION.personal_apps.share_review.key.as_str(), false)
    .await;
  let folder = ground.roots.data.join(STAGING);
  let file = folder.join(format!("{account}-{name}-{}.json", crate::now_ms()));

  std::fs::create_dir_all(&folder)
    .and_then(|()| std::fs::write(&file, json!({ "files": files }).to_string()))
    .map_err(|why| Fault::App(AppFault::Refused(format!("the source could not be staged: {why}"))))?;

  let staged = Staged(file);
  let company = maker.company.to_string();
  let unit = maker.home().to_string();
  let targets_said = Value::Array(targets).to_string();
  let bundle = staged.0.to_string_lossy().into_owned();
  let mut words = vec![
    "share",
    "--company",
    &company,
    "--maker",
    &account,
    "--unit",
    &unit,
    "--version",
    &version,
    "--targets",
    &targets_said,
    "--bundle",
    &bundle,
    "--language",
    language,
  ];

  if review_required {
    words.push("--review-required");
  }

  let shared = kept_json(ground, &words).await?;
  let id = shared["id"].as_str().unwrap_or_default().to_owned();
  let kind = shared["kind"].as_str().unwrap_or(APP).to_owned();
  let key = shared["key"].as_str().unwrap_or_default().to_owned();

  offer_to_company(ground, &key, maker.company).await?;

  drop(staged);
  spawn_review(ground, maker.company, &id, &version, &kind);

  let holders: Vec<String> = shared["installs"]
    .as_object()
    .map(|copies| copies.keys().cloned().collect())
    .unwrap_or_default();

  spawn_keep(ground, holders);
  entry(ground, reader, &id).await
}

// The version of an app or an MCP server the person's PC points at, and the source it was placed from.
async fn pc_source(ground: &Ground, account: &str, name: &str) -> Result<(String, Value), Fault> {
  let runner = personal_apps::runner(ground, account).await?;
  let kept = personal_apps::ask(&runner, &json!({ "owner": account, "versions": {} })).await?;

  personal_apps::expect(&kept, "versions")?;

  let version = kept.last["tools"][name]["current"]
    .as_str()
    .ok_or_else(|| Fault::NotFound(format!("the account {account} has no app {name} in place")))?
    .to_owned();
  let source = personal_apps::ask(
    &runner,
    &json!({ "owner": account, "source": { "tool": name, "version": version } }),
  )
  .await?;

  personal_apps::expect(&source, "source")?;

  Ok((version, source.last["files"].clone()))
}

// The app offered to its maker's company alone: the platform's rule naming the company, written as the platform's
// administrator, the one writer of the platform's link, so no other company's people are offered it.
async fn offer_to_company(ground: &Ground, key: &str, company: i64) -> Result<(), Fault> {
  let administrator = crate::account_isolation::administrator(&ground.db)
    .await
    .map_err(|_| Fault::Unchecked)?
    .and_then(|account| account.parse::<i64>().ok())
    .ok_or(Fault::Unchecked)?;
  let rules = json!([{ "who": format!("unit:{company}"), "value": true }]);
  let written = chain::write_rules(
    &crate::apps::service(ground),
    &format!("app.{key}.offered"),
    "platform",
    administrator,
    rules,
  )
  .await
  .map_err(|_| Fault::Unchecked)?;

  if written.ok() { Ok(()) } else { Err(Fault::Unchecked) }
}

// The review asked once a version is kept, in the background, its report written onto the version.
fn spawn_review(ground: &Arc<Ground>, company: i64, id: &str, version: &str, kind: &str) {
  let ground = ground.clone();
  let (id, version, kind) = (id.to_owned(), version.to_owned(), kind.to_owned());

  tokio::spawn(async move {
    let company = company.to_string();
    let (state, report) =
      match kept_json(&ground, &["source", "--company", &company, &id, "--version", &version]).await {
        Ok(source) => {
          let manifest_asks = one(&ground, company.parse().unwrap_or_default(), &id)
            .await
            .ok()
            .and_then(|app| app.version(&version).map(|one| one.permissions.clone()))
            .unwrap_or_default();

          match crate::translate::review_app(
            &ground,
            &review_input(&kind, &manifest_asks, &source["files"]),
          )
          .await
          {
            Ok(report) => ("written", report),
            Err(why) => ("failed", why),
          }
        }
        Err(why) => ("failed", Refused::from(why).error),
      };
    let file = ground
      .roots
      .data
      .join(STAGING)
      .join(format!("review-{id}-{version}-{}.txt", crate::now_ms()));
    let written = std::fs::create_dir_all(file.parent().unwrap_or(&ground.roots.data))
      .and_then(|()| std::fs::write(&file, &report));

    if written.is_ok() {
      let report_file = file.to_string_lossy().into_owned();

      if let Err(why) = kept_json(
        &ground,
        &[
          "review",
          "--company",
          &company,
          &id,
          "--version",
          &version,
          "--state",
          state,
          "--report",
          &report_file,
        ],
      )
      .await
      {
        eprintln!("catalogue: the review of {id} {version} was not written: {}", Refused::from(why).error);
      }
    }

    let _ = std::fs::remove_file(&file);
  });
}

/// What a shared item's review is handed (docs/agent-roles.md, 「The app-reviewer role」): a first line naming its
/// kind, what it asks for beyond its own folders, then each file of its source under a line naming its path — for a
/// skill its `SKILL.md` and its reference files, for an MCP server its `mcp.toml` and its code.
pub fn review_input(kind: &str, asks: &Asks, files: &Value) -> String {
  let named = |list: &[String]| if list.is_empty() { "none".to_owned() } else { list.join(", ") };
  let (first, whose) = match kind {
    SKILL => ("The item is a skill: instructions an agent's program reads.", "skill"),
    MCP => ("The item is an MCP server: a server an agent's program reaches as tools.", "server"),
    _ => (
      "The item is an app: a personal tool built and run on each receiver's PC.",
      "app's manifest",
    ),
  };
  let mut input = format!(
    "{first}\nThe {whose} asks for the network at: {}.\nIt asks to be handed the secrets: {}.\n",
    named(&asks.network),
    named(&asks.secrets)
  );

  for (path, text) in files.as_object().into_iter().flatten() {
    input.push_str(&format!("\n=== {path} ===\n{}\n", text.as_str().unwrap_or_default()));
  }

  input
}

// The account's copy of the app's version received on their PC, placed, put at home standing for the shared app,
// pointed at and recorded — a skill's written into their skills at home instead; `how` is `self` or `unit`.
async fn install_version(
  ground: &Ground,
  account: &str,
  app: &Kept,
  version: &str,
  how: &str,
  company: i64,
) -> Result<(), Fault> {
  let company = company.to_string();
  let installed = [
    "install",
    "--company",
    company.as_str(),
    app.id.as_str(),
    "--account",
    account,
    "--version",
    version,
    "--how",
    how,
  ];

  if app.kind == SKILL {
    let held = personal_apps::hold(account, &format!("skill:{}", app.name));
    let _held = held.lock().await;
    let source =
      kept_json(ground, &["source", "--company", &company, &app.id, "--version", version]).await?;
    let files: BTreeMap<String, String> = serde_json::from_value(source["files"].clone())
      .map_err(|why| Fault::App(AppFault::Refused(format!("the source does not read: {why}"))))?;

    crate::skills::receive(
      &ground.roots.data,
      account,
      &app.name,
      &files,
      source["sourceHash"].as_str().unwrap_or_default(),
      (&app.id, version),
    )?;

    return kept_json(ground, &installed).await.map(|_| ());
  }

  let runner = personal_apps::runner(ground, account).await?;
  let held = personal_apps::hold(account, &app.name);
  let _held = held.lock().await;
  let source = kept_json(ground, &["source", "--company", &company, &app.id, "--version", version]).await?;
  let received = personal_apps::ask(
    &runner,
    &json!({
      "owner": account,
      "receive": { "tool": app.name, "files": source["files"], "hash": source["sourceHash"], "app": app.id },
    }),
  )
  .await?;

  personal_apps::expect(&received, "received")?;

  // An MCP server's place carries what its version asks for (docs/person-nodes.md, 「MCP servers」).
  let asks = app.version(version).map(|one| one.permissions.clone()).unwrap_or_default();
  let carried = (app.kind == MCP).then_some((asks.network.as_slice(), asks.secrets.as_slice()));

  personal_apps::put_received(ground, &runner, account, &app.name, Some(&app.key), carried).await?;
  kept_json(ground, &installed).await.map(|_| ())
}

// A copy taken off the account's PC and let go in the catalogue.
async fn drop_copy(ground: &Ground, account: &str, app: &Kept, company: i64) -> Result<(), Fault> {
  if app.kind == SKILL {
    crate::skills::drop_copy(ground, account, &app.name).await?;
  } else {
    personal_apps::remove(ground, account, &app.name).await?;
  }

  kept_json(ground, &["uninstall", "--company", &company.to_string(), &app.id, "--account", account])
    .await?;
  Ok(())
}

// The person's own link of the app's `on` written, refused where a lock above holds it.
async fn write_own(ground: &Ground, app: &Kept, person: i64, value: Value) -> Result<(), Fault> {
  let answered = chain::write_value(
    &crate::apps::service(ground),
    &app.setting("on"),
    &format!("person:{person}"),
    person,
    value,
  )
  .await
  .map_err(|_| Fault::Unchecked)?;

  match answered.status {
    status if (200..300).contains(&status) => Ok(()),
    409 => Err(Fault::Locked(format!(
      "the person's organisation keeps the app {} as it set it",
      app.id
    ))),
    403 => Err(Fault::Forbidden(format!("the app {} is not the person's to switch", app.id))),
    _ => Err(Fault::Unchecked),
  }
}

// Whether the person's own link of the app's `on` is theirs to change now.
async fn own_is_open(ground: &Ground, app: &Kept, person: i64) -> Result<(), Fault> {
  let read =
    chain::entry(&crate::apps::service(ground), &app.setting("on"), &format!("person:{person}"), person)
      .await
      .map_err(|_| Fault::Unchecked)?;

  if read.ok() && read.body["writable"] == false {
    return Err(Fault::Locked(format!("the person's organisation keeps the app {} on for them", app.id)));
  }

  Ok(())
}

/// The app's newest version installed for the reader.
pub async fn install(ground: &Ground, reader: &Reader, id: &str) -> Result<CatalogueApp, Fault> {
  let Some(place) = reader.member() else {
    return Err(Fault::Forbidden(
      "the administrator installs no shared app of a company they are no member of".to_owned(),
    ));
  };
  let (app, view) = seen_app(ground, reader, id).await?;
  let latest = app.latest().ok_or_else(|| Fault::NotFound(format!("the app {id} has no version")))?;

  if view.installed.is_some() {
    return Ok(view);
  }

  if app.withdrawn.is_some() {
    return Err(Fault::Withdrawn(id.to_owned()));
  }

  if !latest.approved() {
    return Err(Fault::AwaitingApproval(id.to_owned()));
  }

  let mut asked = Asked::default();

  if !asked
    .flag(ground, place.person, ORGANISATION.personal_apps.install.key.as_str(), true)
    .await
  {
    return Err(Fault::InstallWithheld);
  }

  if !asked.flag(ground, place.person, &app.setting("offered"), true).await {
    return Err(Fault::NotOffered(id.to_owned()));
  }

  let account = reader.account();

  install_version(ground, &account, &app, &latest.version, "self", place.company).await?;
  write_own(ground, &app, place.person, Value::Bool(true)).await?;
  entry(ground, reader, id).await
}

/// The reader's copy moved onto the newest version, what it asks for confirmed by asking.
pub async fn update(ground: &Ground, reader: &Reader, id: &str) -> Result<CatalogueApp, Fault> {
  let Some(place) = reader.member() else {
    return Err(Fault::Forbidden(
      "the administrator holds no copy in a company they are no member of".to_owned(),
    ));
  };
  let (app, _) = seen_app(ground, reader, id).await?;
  let account = reader.account();
  let copy = app
    .installs
    .get(&account)
    .ok_or_else(|| Fault::NotFound(format!("the person holds no copy of {id}")))?;
  let latest = app.latest().ok_or_else(|| Fault::NotFound(format!("the app {id} has no version")))?;

  if !latest.approved() {
    return Err(Fault::AwaitingApproval(id.to_owned()));
  }

  if copy.version != latest.version {
    install_version(ground, &account, &app, &latest.version, &copy.how, place.company).await?;
  }

  entry(ground, reader, id).await
}

/// The reader's copy dropped, their own link of the app's `on` turned off so a unit's turning on brings it not back.
pub async fn uninstall(ground: &Ground, reader: &Reader, id: &str) -> Result<CatalogueApp, Fault> {
  let Some(place) = reader.member() else {
    return Err(Fault::Forbidden(
      "the administrator holds no copy in a company they are no member of".to_owned(),
    ));
  };
  let (app, _) = seen_app(ground, reader, id).await?;
  let account = reader.account();

  if !app.installs.contains_key(&account) {
    return Err(Fault::NotFound(format!("the person holds no copy of {id}")));
  }

  own_is_open(ground, &app, place.person).await?;
  write_own(ground, &app, place.person, Value::Bool(false)).await?;
  drop_copy(ground, &account, &app, place.company).await?;
  entry(ground, reader, id).await
}

/// The reader's copy adopted as their own personal app: no update reaches it again.
pub async fn adopt(ground: &Ground, reader: &Reader, id: &str) -> Result<CatalogueApp, Fault> {
  let Some(place) = reader.member() else {
    return Err(Fault::Forbidden(
      "the administrator holds no copy in a company they are no member of".to_owned(),
    ));
  };
  let (app, _) = seen_app(ground, reader, id).await?;
  let account = reader.account();

  if !app.installs.contains_key(&account) {
    return Err(Fault::NotFound(format!("the person holds no copy of {id}")));
  }

  own_is_open(ground, &app, place.person).await?;
  personal_apps::may_make(ground, &account).await?;

  if app.kind == SKILL {
    crate::skills::adopt(&ground.roots.data, &account, &app.name)?;
  } else {
    let runner = personal_apps::runner(ground, &account).await?;
    let adopted =
      personal_apps::ask(&runner, &json!({ "owner": account, "adopt": { "tool": app.name } })).await?;

    personal_apps::expect(&adopted, "adopted")?;
    personal_apps::put(ground, &account, &app.name).await?;
  }

  write_own(ground, &app, place.person, Value::Bool(false)).await?;
  kept_json(
    ground,
    &["uninstall", "--company", &place.company.to_string(), &app.id, "--account", &account],
  )
  .await?;
  entry(ground, reader, id).await
}

// A link the reader writes at: one the listing names for them.
async fn link_of(ground: &Ground, reader: &Reader, unit: Option<i64>) -> Result<String, Fault> {
  let mut asked = Asked::default();
  let headed = headed_links(ground, &mut asked, reader).await;
  let link = match (reader, unit) {
    (Reader::Administrator { .. }, None) => "platform".to_owned(),
    (_, Some(unit)) => format!("unit:{unit}"),
    (Reader::Person(_), None) => return Err(Fault::BadRequest("a head names the unit".to_owned())),
  };

  if headed.contains(&link) || matches!(reader, Reader::Administrator { .. }) {
    Ok(link)
  } else {
    Err(Fault::Forbidden(format!("the person heads no {link} of theirs")))
  }
}

async fn write_link(
  ground: &Ground,
  reader: &Reader,
  key: &str,
  link: &str,
  value: Value,
  delegation: &str,
) -> Result<(), Fault> {
  let service = crate::apps::service(ground);
  let caller = reader.person();
  let judged = |answered: chain::Answered| match answered.status {
    status if (200..300).contains(&status) => Ok(()),
    409 => Err(Fault::Locked(format!("a link above {link} holds the setting {key}"))),
    403 => Err(Fault::Forbidden(format!("the person does not write {link}"))),
    _ => Err(Fault::Unchecked),
  };

  judged(
    chain::write_value(&service, key, link, caller, value)
      .await
      .map_err(|_| Fault::Unchecked)?,
  )?;
  judged(
    chain::write_delegation(&service, key, link, caller, delegation)
      .await
      .map_err(|_| Fault::Unchecked)?,
  )
}

// The people standing under a unit, as its subtree answers its head.
async fn members_under(ground: &Ground, unit: i64, head: i64) -> Vec<String> {
  let Ok(read) =
    chain::read(&crate::apps::service(ground), &format!("/units/{unit}/subtree"), Some(head)).await
  else {
    return Vec::new();
  };
  let mut people = BTreeSet::new();

  for one in read.body["units"].as_array().into_iter().flatten() {
    for member in one["members"].as_array().into_iter().flatten() {
      if member["left"] != true
        && let Some(user) = member["user"].as_i64()
      {
        people.insert(user.to_string());
      }
    }

    for head in one["heads"].as_array().into_iter().flatten() {
      if let Some(user) = head["user"].as_i64() {
        people.insert(user.to_string());
      }
    }
  }

  people.into_iter().collect()
}

/// The app turned on or off for a unit the reader heads, locked below where they ask; each member's copy is kept
/// after.
pub async fn switch_unit(
  ground: &Arc<Ground>,
  reader: &Reader,
  id: &str,
  unit: i64,
  on: bool,
  lock: bool,
) -> Result<CatalogueApp, Fault> {
  let (app, view) = seen_app(ground, reader, id).await?;
  let link = link_of(ground, reader, Some(unit)).await?;

  if on && (app.withdrawn.is_some() || !view.approval.approved && view.approval.required) {
    return Err(if app.withdrawn.is_some() {
      Fault::Withdrawn(id.to_owned())
    } else {
      Fault::AwaitingApproval(id.to_owned())
    });
  }

  if on {
    write_link(ground, reader, &app.setting("offered"), &link, Value::Null, "inherit").await?;
  }

  write_link(
    ground,
    reader,
    &app.setting("on"),
    &link,
    Value::Bool(on),
    if lock { "lock" } else { "inherit" },
  )
  .await?;
  spawn_keep(ground, members_under(ground, unit, reader.person()).await);
  entry(ground, reader, id).await
}

/// The app recalled at a unit the reader heads, or at the platform for the administrator: its `offered` off and
/// locked there, and each copy under it taken away as its PC is reached.
pub async fn recall(
  ground: &Arc<Ground>,
  reader: &Reader,
  id: &str,
  unit: Option<i64>,
) -> Result<CatalogueApp, Fault> {
  let (app, view) = seen_app(ground, reader, id).await?;
  let link = link_of(ground, reader, unit).await?;

  write_link(ground, reader, &app.setting("offered"), &link, Value::Bool(false), "lock").await?;

  // At the platform, the rule offering it to its company goes too, since a rule stands over its link's own word.
  if link == "platform" {
    let answered = chain::write_rules(
      &crate::apps::service(ground),
      &app.setting("offered"),
      "platform",
      reader.person(),
      json!([]),
    )
    .await
    .map_err(|_| Fault::Unchecked)?;

    if !answered.ok() {
      return Err(Fault::Unchecked);
    }
  }

  let people = match unit {
    Some(unit) => members_under(ground, unit, reader.person()).await,
    None => app.installs.keys().cloned().collect(),
  };

  spawn_keep(ground, people);
  Ok(entry(ground, reader, id).await.unwrap_or(view))
}

/// The app withdrawn by its owner, its maker or the administrator.
pub async fn withdraw(ground: &Ground, reader: &Reader, id: &str) -> Result<CatalogueApp, Fault> {
  let (_, view) = seen_app(ground, reader, id).await?;

  if !view.may.withdraw {
    return Err(Fault::Forbidden(format!("the app {id} is not the person's to withdraw")));
  }

  kept_json(ground, &["withdraw", "--company", &reader.company().to_string(), id]).await?;
  entry(ground, reader, id).await
}

/// A version approved by a head of the maker's home unit.
pub async fn approve(
  ground: &Ground,
  reader: &Reader,
  id: &str,
  version: &str,
) -> Result<CatalogueApp, Fault> {
  let (app, view) = seen_app(ground, reader, id).await?;

  if !view.may.approve || app.version(version).is_none() {
    return Err(Fault::Forbidden(format!(
      "the version {version} of {id} is not the person's to approve"
    )));
  }

  let by = reader.account();

  kept_json(
    ground,
    &[
      "approve",
      "--company",
      &reader.company().to_string(),
      id,
      "--version",
      version,
      "--by",
      &by,
    ],
  )
  .await?;
  entry(ground, reader, id).await
}

/// The request to make the app a platform app.
pub async fn promote(
  ground: &Ground,
  reader: &Reader,
  id: &str,
  note: &str,
) -> Result<CatalogueApp, Fault> {
  let (_, view) = seen_app(ground, reader, id).await?;

  if !view.may.promote {
    return Err(Fault::Forbidden(format!("the app {id} is not the person's to put forward")));
  }

  let by = reader.account();

  kept_json(
    ground,
    &["promote", "--company", &reader.company().to_string(), id, "--by", &by, "--note", note],
  )
  .await?;
  entry(ground, reader, id).await
}

// One locale's text written on the record, marked as a translation's where `machine` says so; the locale cleared where
// no title is given.
async fn write_text(
  ground: &Ground,
  company: i64,
  id: &str,
  locale: &str,
  text: Option<(&str, &str)>,
  machine: bool,
) -> Result<(), Fault> {
  let company = company.to_string();
  let mut words = vec!["text", "--company", company.as_str(), id, "--locale", locale];

  match text {
    Some((title, description)) => {
      words.extend(["--title", title, "--description", description]);

      if machine {
        words.push("--machine");
      }
    }
    None => words.push("--clear"),
  }

  kept_json(ground, &words).await.map(|_| ())
}

fn locale_named(locale: &str) -> Result<&'static str, Fault> {
  LOCALES
    .iter()
    .find(|(one, _)| *one == locale)
    .map(|(_, language)| *language)
    .ok_or_else(|| Fault::BadRequest(format!("{locale} is no locale: ko, en, ja or zh-CN")))
}

/// One locale's text set by the item's maker or owner, its machine mark taken away; the locale cleared where no title
/// and no description are given.
pub async fn text(
  ground: &Ground,
  reader: &Reader,
  id: &str,
  locale: &str,
  title: Option<&str>,
  description: Option<&str>,
) -> Result<CatalogueApp, Fault> {
  let (_, view) = seen_app(ground, reader, id).await?;

  locale_named(locale)?;

  if !view.may.text {
    return Err(Fault::Forbidden(format!("the text of {id} is not the person's to set")));
  }

  let text = match (title, description) {
    (None, None) => None,
    (title, description) => Some((title.unwrap_or_default(), description.unwrap_or_default())),
  };

  write_text(ground, reader.company(), id, locale, text, false).await?;
  entry(ground, reader, id).await
}

/// Each locale of the four the item holds no title in filled by `translate`, from the maker's title and description,
/// and written with the machine mark (docs/person-nodes.md, 「Shared apps」); a locale whose translation fails is left
/// out. `translate` is handed the language's name and the text, and answers the translation.
pub async fn fill_languages<F, Done>(
  ground: &Ground,
  reader: &Reader,
  id: &str,
  translate: F,
) -> Result<CatalogueApp, Fault>
where
  F: Fn(&'static str, String) -> Done,
  Done: std::future::Future<Output = Result<String, String>>,
{
  let (app, view) = seen_app(ground, reader, id).await?;

  if !view.may.translate {
    return Err(Fault::Forbidden(format!("the languages of {id} are not the person's to fill")));
  }

  for (locale, language) in LOCALES {
    if app.titles.contains_key(locale) || app.language.as_deref() == Some(locale) {
      continue;
    }

    let title = translate(language, app.title.clone()).await;
    let description = if app.description.trim().is_empty() {
      Ok(String::new())
    } else {
      translate(language, app.description.clone()).await
    };

    match (title, description) {
      (Ok(title), Ok(description)) => {
        write_text(ground, reader.company(), id, locale, Some((title.trim(), description.trim())), true)
          .await?;
      }
      (Err(why), _) | (_, Err(why)) => {
        eprintln!("catalogue: {id} was not translated into {language}: {why}")
      }
    }
  }

  entry(ground, reader, id).await
}

/// The reader's looking at what was shared with them kept as now.
pub async fn seen(ground: &Ground, reader: &Reader) -> Result<i64, Fault> {
  let read = kept_json(
    ground,
    &["seen", "--company", &reader.company().to_string(), "--account", &reader.account()],
  )
  .await?;

  Ok(read["at"].as_i64().unwrap_or_default())
}

/// Every company's requests to make an app a platform app.
pub async fn promotions(ground: &Ground) -> Result<Value, Fault> {
  kept_json(ground, &["promotions"]).await
}

// One keeping of an account's copies at a time: a keeping asked while one runs waits for it, then keeps again.
static KEEPING: LazyLock<std::sync::Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>> =
  LazyLock::new(Default::default);

fn spawn_keep(ground: &Arc<Ground>, accounts: Vec<String>) {
  for account in accounts {
    let ground = ground.clone();

    tokio::spawn(async move {
      if let Err(why) = keep(&ground, &account).await {
        eprintln!("catalogue: the copies of account {account} were not kept: {why}");
      }
    });
  }
}

/// The person's copies kept to what the chain and the catalogue say (docs/person-nodes.md, 「Keeping the copies」):
/// a copy no longer offered taken away, one on an older version moved where the newest asks for nothing more, and one
/// a unit turned on for them installed where they hold none. Nothing of an app or an MCP server is done where their PC
/// has no runner; their skills stand at home and are kept whatever their PC is.
pub async fn keep(ground: &Ground, account: &str) -> Result<(), String> {
  let held = KEEPING
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner)
    .entry(account.to_owned())
    .or_default()
    .clone();
  let _held = held.lock().await;

  keep_now(ground, account).await
}

async fn keep_now(ground: &Ground, account: &str) -> Result<(), String> {
  let said = |fault: Fault| Refused::from(fault).error;
  let person: i64 = account.parse().map_err(|_| format!("{account} is no person's account"))?;
  let pc = personal_apps::runner(ground, account).await.is_ok();

  let place = place_of(ground, person).await.map_err(said)?;
  let (apps, _) = listed(ground, place.company, None).await.map_err(said)?;
  let mut asked = Asked::default();
  let may_install = asked.flag(ground, person, ORGANISATION.personal_apps.install.key.as_str(), true).await;

  for app in &apps {
    let Some(latest) = app.latest() else { continue };

    if app.kind != SKILL && !pc {
      continue;
    }

    let copy = app.installs.get(account);
    let offered = asked.flag(ground, person, &app.setting("offered"), true).await;
    let done = match copy {
      Some(_) if !offered => drop_copy(ground, account, app, place.company).await,
      Some(copy) if copy.version != latest.version && latest.approved() => {
        let grows = app
          .version(&copy.version)
          .is_none_or(|from| latest.permissions.grows_over(&from.permissions));

        if grows {
          Ok(())
        } else {
          install_version(ground, account, app, &latest.version, &copy.how, place.company).await
        }
      }
      Some(_) => Ok(()),
      None => {
        let on = asked.resolved(ground, person, &app.setting("on")).await;
        let by_unit = on.as_ref().is_some_and(|on| {
          on["value"] == true && on["from"].as_str().is_some_and(|from| from.starts_with("unit:"))
        });

        if by_unit && offered && may_install && app.withdrawn.is_none() && latest.approved() {
          install_version(ground, account, app, &latest.version, "unit", place.company).await
        } else {
          Ok(())
        }
      }
    };

    if let Err(fault) = done {
      eprintln!("catalogue: {account}'s copy of {} was not kept: {}", app.id, said(fault));
    }
  }

  Ok(())
}

// Which nodes' links were heard at the last pass, so a node heard again is kept once.
static HEARD: LazyLock<std::sync::Mutex<BTreeSet<String>>> = LazyLock::new(Default::default);

/// One pass of the copies' keeping: each account whose PC's link is heard now and was not at the last pass has its
/// copies kept, in the background.
pub async fn pass(ground: &Arc<Ground>) -> Result<(), String> {
  let nodes = crate::person_nodes::every(&ground.db).await.map_err(|why| why.detail)?;
  let mut heard_now = BTreeSet::new();
  let mut fresh = Vec::new();

  for node in nodes {
    if crate::watches::links::link_heard(&ground.roots.data, &node.node) {
      heard_now.insert(node.node.clone());

      if !HEARD.lock().unwrap_or_else(std::sync::PoisonError::into_inner).contains(&node.node) {
        fresh.push(node.owner_account.clone());
      }
    }
  }

  *HEARD.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = heard_now;
  spawn_keep(ground, fresh);
  Ok(())
}
