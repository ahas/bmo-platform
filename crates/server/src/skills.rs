//! A person's skills, kept at home (docs/person-nodes.md, 「Skills at home」): one folder a skill under the data root,
//! `agency/skills/<account>/<name>/`, its `SKILL.md` opening on front matter and its reference files beside it; a
//! copy of a shared skill marked with the catalogue's id and version; saved whole, read, removed, received from the
//! catalogue checked against its hash, dropped and adopted, with no PC involved.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use axum::http::StatusCode;
use bmo_agency_client::PersonalSkill;
use pulldown_cmark::{Event, MetadataBlockKind, Options, Parser, Tag, TagEnd};
use serde::Deserialize;
use serde_json::json;

use crate::control::Refused;
use crate::personal_apps::{self, Fault as AppFault};
use crate::places::Ground;

/// Where the skills stand, under the data root.
pub const STORE: &str = "agency/skills";

/// The file a skill opens on.
pub const SKILL_FILE: &str = "SKILL.md";

/// The file holding the version a skill's folder stands on.
pub const VERSION_FILE: &str = ".version";

/// The mark of an installed copy: the catalogue's id and the version it is a copy of.
pub const RECEIVED_MARK: &str = ".received.json";

/// The most a skill's files hold together: the catalogue's bundle size.
pub const BUNDLE_MOST: usize = node_link::keeping::SOURCE_MOST;

/// The locales a skill's text may be written in.
pub const LOCALES: [&str; 4] = ["ko", "en", "ja", "zh-CN"];

/// Why a skill was not saved, read or removed.
#[derive(Debug)]
pub enum Fault {
  App(AppFault),
  Bad(String),
  NameTaken(String),
  InstalledCopy(String),
  NotFound(String),
  Failed(String),
}

impl From<AppFault> for Fault {
  fn from(fault: AppFault) -> Self {
    Self::App(fault)
  }
}

impl From<store::Fault> for Fault {
  fn from(fault: store::Fault) -> Self {
    Self::Failed(fault.detail)
  }
}

impl From<Fault> for Refused {
  fn from(fault: Fault) -> Self {
    match fault {
      Fault::App(fault) => fault.into(),
      Fault::Bad(why) => Refused::new(StatusCode::BAD_REQUEST, why).with_code("bad-request"),
      Fault::NameTaken(why) => Refused::new(StatusCode::CONFLICT, why).with_code("name-taken"),
      Fault::InstalledCopy(name) => Refused::new(
        StatusCode::CONFLICT,
        format!("the skill {name} is an installed copy, dropped through the catalogue"),
      )
      .with_code("installed-copy"),
      Fault::NotFound(why) => Refused::new(StatusCode::NOT_FOUND, why).with_code("not-found"),
      Fault::Failed(why) => Refused::new(StatusCode::INTERNAL_SERVER_ERROR, why).with_code("failed"),
    }
  }
}

/// A text a skill's front matter writes: one string, or one a locale.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Text {
  Plain(String),
  Locales(BTreeMap<String, String>),
}

impl Text {
  // The text in its own language, and the text by locale, a plain string kept under the language named.
  fn read(text: Option<&Self>, language: Option<&str>) -> (Option<String>, BTreeMap<String, String>) {
    match text {
      None => (None, BTreeMap::new()),
      Some(Self::Plain(plain)) => (
        Some(plain.clone()),
        language
          .map(|language| BTreeMap::from([(language.to_owned(), plain.clone())]))
          .unwrap_or_default(),
      ),
      Some(Self::Locales(locales)) => (
        language
          .and_then(|language| locales.get(language))
          .or_else(|| locales.values().next())
          .cloned(),
        locales.clone(),
      ),
    }
  }
}

/// A skill's front matter: its name, what the agent's program reads it by, and the text a screen shows it by. Any
/// other key is the program's.
#[derive(Clone, Debug, Deserialize)]
pub struct FrontMatter {
  pub name: String,
  pub description: String,
  #[serde(default)]
  pub title: Option<Text>,
  #[serde(default)]
  pub summary: Option<Text>,
  #[serde(default)]
  pub language: Option<String>,
}

/// A skill's name: a lower-case letter, then lower-case letters, digits and `-`, at most 64.
pub fn skill_name(name: &str) -> bool {
  name.len() <= 64
    && name.chars().next().is_some_and(|first| first.is_ascii_lowercase())
    && name.chars().all(|one| one.is_ascii_lowercase() || one.is_ascii_digit() || one == '-')
}

/// The front matter a `SKILL.md` opens on: the YAML block the markdown reader finds at its head, read by the YAML
/// library.
pub fn front_matter(text: &str) -> Result<FrontMatter, String> {
  let mut inside = false;
  let mut yaml = None::<String>;

  for event in Parser::new_ext(text, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS) {
    match event {
      Event::Start(Tag::MetadataBlock(MetadataBlockKind::YamlStyle)) => {
        inside = true;
        yaml = Some(String::new());
      }
      Event::End(TagEnd::MetadataBlock(_)) => break,
      Event::Text(said) if inside => yaml.get_or_insert_default().push_str(&said),
      _ if !inside => break,
      _ => {}
    }
  }

  let yaml = yaml.ok_or_else(|| format!("its {SKILL_FILE} opens on no front matter between --- lines"))?;
  let read: FrontMatter = serde_norway::from_str(&yaml)
    .map_err(|why| format!("its {SKILL_FILE}'s front matter does not read: {why}"))?;

  if read.description.trim().is_empty() {
    return Err(format!("its {SKILL_FILE}'s front matter holds no description"));
  }

  Ok(read)
}

/// The folder an account's skills stand in.
pub fn folder_of(data: &Path, account: &str) -> PathBuf {
  data.join(STORE).join(account)
}

fn named(name: &str) -> Result<(), Fault> {
  if skill_name(name) {
    Ok(())
  } else {
    Err(Fault::Bad(format!(
      "{name:?} is no skill's name: a lower-case letter, then lower-case letters, digits and `-`, at most 64"
    )))
  }
}

// A path inside a folder, made of folder and file names alone, and none of the marks.
fn inside(path: &str) -> bool {
  !path.is_empty()
    && Path::new(path).components().all(|part| matches!(part, Component::Normal(_)))
    && path != VERSION_FILE
    && path != RECEIVED_MARK
}

/// What a skill's files must be to be kept: every path inside its folder, every file text, all of them at most the
/// bundle size, and a `SKILL.md` whose front matter reads and names the skill.
pub fn check(name: &str, files: &BTreeMap<String, String>) -> Result<FrontMatter, Fault> {
  named(name)?;

  if let Some(path) = files.keys().find(|path| !inside(path)) {
    return Err(Fault::Bad(format!(
      "the file {path:?} stands outside the skill's folder or names a mark"
    )));
  }

  if let Some(path) = files.iter().find(|(_, text)| text.contains('\0')).map(|(path, _)| path) {
    return Err(Fault::Bad(format!("the file {path} is not text")));
  }

  if files.values().map(String::len).sum::<usize>() > BUNDLE_MOST {
    return Err(Fault::Bad(format!("the skill's files hold more than {BUNDLE_MOST} bytes")));
  }

  let text = files
    .get(SKILL_FILE)
    .ok_or_else(|| Fault::Bad(format!("the skill holds no {SKILL_FILE}")))?;
  let read = front_matter(text).map_err(Fault::Bad)?;

  if read.name != name {
    return Err(Fault::Bad(format!("its front matter names {:?}, not {name:?}", read.name)));
  }

  Ok(read)
}

// Every file under a folder, by its path from it, the marks left out.
fn read_files(dir: &Path) -> BTreeMap<String, String> {
  let mut found = BTreeMap::new();
  let mut open = vec![dir.to_owned()];

  while let Some(at) = open.pop() {
    let Ok(read) = std::fs::read_dir(&at) else { continue };

    for entry in read.filter_map(Result::ok) {
      let path = entry.path();

      if path.is_dir() {
        open.push(path);
      } else if let Ok(inner) = path.strip_prefix(dir) {
        let inner = inner.to_string_lossy().into_owned();

        if inside(&inner)
          && let Ok(text) = std::fs::read_to_string(&path)
        {
          found.insert(inner, text);
        }
      }
    }
  }

  found
}

/// One skill standing at home: its name, its front matter, its version, and the catalogue's id and version where it
/// is an installed copy.
#[derive(Clone, Debug)]
pub struct Held {
  pub name: String,
  pub front: FrontMatter,
  pub version: String,
  pub received: Option<(String, String)>,
}

impl Held {
  pub fn origin(&self) -> &'static str {
    if self.received.is_some() { "installed" } else { "own" }
  }

  /// The title it is shown by in its own language, and its titles by locale.
  pub fn titles(&self) -> (Option<String>, BTreeMap<String, String>) {
    Text::read(self.front.title.as_ref(), self.front.language.as_deref())
  }
}

fn read_held(dir: &Path, name: &str) -> Option<Held> {
  let front = front_matter(&std::fs::read_to_string(dir.join(SKILL_FILE)).ok()?).ok()?;
  let version = std::fs::read_to_string(dir.join(VERSION_FILE)).unwrap_or_default().trim().to_owned();
  let received = std::fs::read_to_string(dir.join(RECEIVED_MARK))
    .ok()
    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    .map(|mark| {
      (
        mark["app"].as_str().unwrap_or_default().to_owned(),
        mark["version"].as_str().unwrap_or_default().to_owned(),
      )
    });

  Some(Held {
    name: name.to_owned(),
    front,
    version,
    received,
  })
}

/// Every skill of the account's that reads, by name.
pub fn held(data: &Path, account: &str) -> Vec<Held> {
  let Ok(read) = std::fs::read_dir(folder_of(data, account)) else {
    return Vec::new();
  };
  let mut found: Vec<Held> = read
    .filter_map(Result::ok)
    .filter(|entry| entry.path().is_dir())
    .filter_map(|entry| {
      let name = entry.file_name().to_string_lossy().into_owned();

      skill_name(&name).then(|| read_held(&entry.path(), &name)).flatten()
    })
    .collect();

  found.sort_by(|one, other| one.name.cmp(&other.name));
  found
}

/// One skill of the account's, where it stands.
pub fn one(data: &Path, account: &str, name: &str) -> Option<Held> {
  skill_name(name)
    .then(|| read_held(&folder_of(data, account).join(name), name))
    .flatten()
}

/// The skill as the listing answers it, with the holders it is attached to.
pub fn view(held: &Held, attached_to: Vec<String>) -> PersonalSkill {
  let language = held.front.language.as_deref();
  let (title, titles) = held.titles();
  let (summary, summaries) = Text::read(held.front.summary.as_ref(), language);

  PersonalSkill {
    name: held.name.clone(),
    description: held.front.description.clone(),
    title,
    summary,
    titles,
    summaries,
    language: held.front.language.clone(),
    version: held.version.clone(),
    origin: held.origin().to_owned(),
    app: held.received.as_ref().map(|(app, _)| app.clone()),
    attached_to,
  }
}

/// The account's skills as the listing answers them.
pub async fn list(ground: &Ground, account: &str) -> Result<Vec<PersonalSkill>, Fault> {
  let mut skills = Vec::new();

  for held in held(&ground.roots.data, account) {
    let holders = crate::attachments::holders_of(&ground.db, account, "skill", &held.name).await?;

    skills.push(view(&held, holders));
  }

  Ok(skills)
}

/// One skill's files, its version and its origin.
pub fn files(data: &Path, account: &str, name: &str) -> Result<(Held, BTreeMap<String, String>), Fault> {
  named(name)?;

  let held = one(data, account, name)
    .ok_or_else(|| Fault::NotFound(format!("the account {account} holds no skill {name}")))?;

  Ok((held, read_files(&folder_of(data, account).join(name))))
}

// A version's name: the moment it is saved and a random suffix, as the runner names an app's.
fn fresh_version() -> String {
  let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
  let suffix = (now.subsec_nanos() ^ std::process::id().rotate_left(16)) & 0x00ff_ffff;

  format!("{}-{suffix:06x}", now.as_millis())
}

// The folder written whole beside its place and moved into it: the files, the version and the mark where it is a
// copy.
fn write_whole(
  dir: &Path,
  files: &BTreeMap<String, String>,
  version: &str,
  mark: Option<&serde_json::Value>,
) -> Result<(), Fault> {
  let parent = dir.parent().ok_or_else(|| Fault::Failed("a skill's folder has no parent".to_owned()))?;
  let name = dir.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
  let staged = parent.join(format!(".{name}.{}", fresh_version()));
  let fail = |why: std::io::Error| Fault::Failed(format!("the skill {name} could not be written: {why}"));
  let written = (|| {
    for (path, text) in files {
      let to = staged.join(path);

      std::fs::create_dir_all(to.parent().unwrap_or(&staged))?;
      std::fs::write(&to, text)?;
    }

    std::fs::create_dir_all(&staged)?;
    std::fs::write(staged.join(VERSION_FILE), version)?;

    if let Some(mark) = mark {
      std::fs::write(staged.join(RECEIVED_MARK), mark.to_string())?;
    }

    let _ = std::fs::remove_dir_all(dir);

    std::fs::rename(&staged, dir)
  })();

  if written.is_err() {
    let _ = std::fs::remove_dir_all(&staged);
  }

  written.map_err(fail)
}

/// One of the account's own skills saved whole under a new version, where the chain lets the person make personal
/// apps; refused over an installed copy.
pub async fn save(
  ground: &Ground,
  account: &str,
  name: &str,
  files: &BTreeMap<String, String>,
) -> Result<PersonalSkill, Fault> {
  named(name)?;
  personal_apps::may_make(ground, account).await?;

  let held = personal_apps::hold(account, &format!("skill:{name}"));
  let _held = held.lock().await;

  if one(&ground.roots.data, account, name).is_some_and(|held| held.received.is_some()) {
    return Err(Fault::NameTaken(format!(
      "the skill {name} is an installed copy; adopt it before changing it"
    )));
  }

  check(name, files)?;
  write_whole(&folder_of(&ground.roots.data, account).join(name), files, &fresh_version(), None)?;

  let saved = one(&ground.roots.data, account, name)
    .ok_or_else(|| Fault::Failed(format!("the skill {name} does not read back")))?;
  let holders = crate::attachments::holders_of(&ground.db, account, "skill", name).await?;

  Ok(view(&saved, holders))
}

/// One of the account's own skills removed, and every attachment of it.
pub async fn remove(ground: &Ground, account: &str, name: &str) -> Result<(), Fault> {
  named(name)?;

  let held = one(&ground.roots.data, account, name)
    .ok_or_else(|| Fault::NotFound(format!("the account {account} holds no skill {name}")))?;

  if held.received.is_some() {
    return Err(Fault::InstalledCopy(name.to_owned()));
  }

  forget(ground, account, name).await
}

// The folder and every attachment of it taken away.
async fn forget(ground: &Ground, account: &str, name: &str) -> Result<(), Fault> {
  let dir = folder_of(&ground.roots.data, account).join(name);

  std::fs::remove_dir_all(&dir).map_err(|why| Fault::Failed(format!("{}: {why}", dir.display())))?;
  crate::attachments::forget_item(&ground.db, account, "skill", name).await?;
  Ok(())
}

/// A shared skill's version written into the account's skills as a copy of the catalogue's app, where its files'
/// hash is the one recorded; a folder of the name that is the person's own is refused.
pub fn receive(
  data: &Path,
  account: &str,
  name: &str,
  files: &BTreeMap<String, String>,
  hash: &str,
  (app, version): (&str, &str),
) -> Result<(), Fault> {
  named(name)?;

  if node_link::keeping::source_hash(files) != hash {
    return Err(Fault::Bad(format!("the skill {name}'s source is not the one shared")));
  }

  if one(data, account, name).is_some_and(|held| held.received.as_ref().is_none_or(|(of, _)| of != app)) {
    return Err(Fault::NameTaken(format!(
      "the account {account} holds a skill {name} that is no copy of {app}"
    )));
  }

  check(name, files)?;
  write_whole(
    &folder_of(data, account).join(name),
    files,
    version,
    Some(&json!({ "app": app, "version": version })),
  )
}

/// An installed copy dropped: its folder and every attachment of it.
pub async fn drop_copy(ground: &Ground, account: &str, name: &str) -> Result<(), Fault> {
  if one(&ground.roots.data, account, name).is_none() {
    return Ok(());
  }

  forget(ground, account, name).await
}

/// An installed copy adopted as the person's own: its mark taken away.
pub fn adopt(data: &Path, account: &str, name: &str) -> Result<(), Fault> {
  let mark = folder_of(data, account).join(name).join(RECEIVED_MARK);

  match std::fs::remove_file(&mark) {
    Ok(()) => Ok(()),
    Err(why) if why.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(why) => Err(Fault::Failed(format!("{}: {why}", mark.display()))),
  }
}

/// The account's own skill as a share reads it: its version and its files; an installed copy is none of the
/// person's to share.
pub fn own_source(
  data: &Path,
  account: &str,
  name: &str,
) -> Result<(String, BTreeMap<String, String>), Fault> {
  let (held, files) = files(data, account, name)?;

  if held.received.is_some() {
    return Err(Fault::NotFound(format!("the account {account} holds no skill {name} of its own")));
  }

  Ok((held.version, files))
}
