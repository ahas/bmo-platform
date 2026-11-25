//! What the owner changes about the Claude accounts from the fallback screen (docs/cache-fallback.md,
//! 「The Claude Code road is a list of accounts」): an account added, renamed, moved to another
//! credential, turned on or off, put in order, removed, and chosen to run on now. What is kept is where
//! a credential is reached, never a credential.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use regex::Regex;
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
use serde_json::Value;

use super::claude_accounts::{
  Account, CHOSEN_KEY, Credential, CredentialKind, EndpointModels, IN_USE_KEY, accounts, chosen_account,
};
use super::{Refused, set_setting, setting};
use crate::now_ms;
use crate::text::length;
use store::{one, run};

// What an id may be: it stands in a work key's row, a CLI flag and a session record's key, so it is
// the small alphabet all three take without quoting.
static ID: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9._-]{1,31}$").expect("the account id pattern compiles"));

const NAME_MAX: usize = 40;

/// What the owner gave of an account's credential, each part only where it was given.
#[derive(Clone, Debug, Default)]
pub struct CredentialGiven {
  pub kind: Option<CredentialKind>,
  pub path: Option<String>,
  pub endpoint: Option<String>,
  pub key_file: Option<String>,
  pub models: Option<EndpointModels>,
}

impl CredentialGiven {
  /// Whether nothing of a credential was given.
  pub fn is_empty(&self) -> bool {
    self.kind.is_none()
      && self.path.is_none()
      && self.endpoint.is_none()
      && self.key_file.is_none()
      && self.models.is_none()
  }

  /// The credential it makes over the one the account holds: a part not given keeps the held one's, the
  /// kind a directory where neither names one, each given text with its edges trimmed; or the refusal
  /// naming the part still missing.
  pub fn over(&self, held: Option<&Credential>) -> Result<Credential, String> {
    let words = &CONTROL_WORDS.accounts;
    let trimmed = |given: &Option<String>| given.as_deref().map(|text| text.trim().to_owned());
    let kind = self.kind.or(held.map(Credential::kind)).unwrap_or(CredentialKind::Directory);
    let path = trimmed(&self.path)
      .or_else(|| held.map(|held| held.path().to_string_lossy().into_owned()))
      .filter(|path| !path.is_empty())
      .ok_or_else(|| words.credential_path.clone())?;
    let (held_url, held_key, held_models) = match held {
      Some(Credential::Endpoint {
        url,
        key_file,
        models,
        ..
      }) => (Some(url.clone()), Some(key_file.to_string_lossy().into_owned()), models.clone()),
      _ => (None, None, EndpointModels::default()),
    };
    let held_secret = match held {
      Some(Credential::ApiKey { key_file, .. }) => Some(key_file.to_string_lossy().into_owned()),
      _ => None,
    };

    Ok(match kind {
      CredentialKind::Directory => Credential::Directory { home: path.into() },
      CredentialKind::Token => Credential::Token { file: path.into() },
      CredentialKind::Endpoint => Credential::Endpoint {
        home: path.into(),
        url: trimmed(&self.endpoint)
          .or(held_url)
          .filter(|url| !url.is_empty())
          .ok_or_else(|| words.endpoint_needed.clone())?,
        key_file: trimmed(&self.key_file)
          .or(held_key)
          .filter(|file| !file.is_empty())
          .ok_or_else(|| words.key_file_needed.clone())?
          .into(),
        models: self.models.clone().unwrap_or(held_models),
      },
      CredentialKind::ApiKey => Credential::ApiKey {
        home: path.into(),
        key_file: trimmed(&self.key_file)
          .or(held_secret)
          .filter(|file| !file.is_empty())
          .ok_or_else(|| words.secret_needed.clone())?
          .into(),
      },
    })
  }

  /// The same parts with the secret an `api-key` account's key sits in resolved under the data root's
  /// `env/`: a change names the secret, never a path, as the person's `users/<user>/<name>` for a person's
  /// account or a plain name for the platform's (docs/cache-fallback.md, 「The Claude Code road is a list of
  /// accounts」). A key file given for another kind is left as it was given; `held` is the kind of the
  /// account changed, none for one added.
  pub fn with_secret_under(
    mut self,
    held: Option<CredentialKind>,
    data_root: &Path,
    user: Option<i64>,
  ) -> Result<Self, String> {
    let kind = self.kind.or(held).unwrap_or(CredentialKind::Directory);

    if kind != CredentialKind::ApiKey {
      return Ok(self);
    }

    if let Some(named) = self.key_file.as_deref().map(str::trim).filter(|named| !named.is_empty()) {
      let path =
        secret_path(data_root, user, named).ok_or_else(|| CONTROL_WORDS.accounts.secret_name.clone())?;

      self.key_file = Some(path.to_string_lossy().into_owned());
    }

    Ok(self)
  }
}

/// A secret's name as one plain file name, none where it is empty, names a directory step or carries a
/// separator: what a person's secret space and the platform's hold.
pub fn plain_secret(name: &str) -> Option<&str> {
  let name = name.trim();

  (!name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])).then_some(name)
}

/// The path a secret named for an `api-key` account's key stands at under the data root's `env/`: a person's
/// `users/<user>/<name>`, named by its plain name or by that whole path, and the platform's plain name;
/// none for a name that is neither.
pub fn secret_path(data_root: &Path, user: Option<i64>, named: &str) -> Option<PathBuf> {
  let env = data_root.join("env");

  match user {
    None => plain_secret(named).map(|name| env.join(name)),
    Some(user) => {
      let space = format!("users/{user}/");
      let name = named.trim().strip_prefix(&space).unwrap_or(named);

      plain_secret(name).map(|name| env.join("users").join(user.to_string()).join(name))
    }
  }
}

fn no_such(id: &str) -> String {
  fill_marks(&CONTROL_WORDS.accounts.unknown, &[("id", id)])
}

/// What the owner gave of an account's base prompt: nothing at all, a prompt by name, or no prompt where
/// one was held. A change that gives none keeps the base the account holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum BasePromptGiven {
  #[default]
  Keep,
  None,
  Named(String),
}

impl BasePromptGiven {
  /// The prompt the change names, where it names one.
  pub fn named_prompt(&self) -> Option<&str> {
    match self {
      Self::Named(name) => Some(name.trim()),
      Self::Keep | Self::None => None,
    }
  }

  /// What the change writes where the column holds it: the prompt's name, no prompt at all, or nothing
  /// where the account keeps the one it holds.
  fn to_write(&self) -> Option<Option<String>> {
    match self {
      Self::Keep => None,
      Self::None => Some(None),
      Self::Named(name) => Some(Some(name.trim().to_owned())),
    }
  }
}

/// Why an account cannot name this base prompt, or nothing: a prompt it names is one that is kept.
async fn base_prompt_fault<C: ConnectionTrait>(
  db: &C,
  given: &BasePromptGiven,
) -> Result<Option<String>, Refused> {
  let Some(name) = given.named_prompt() else {
    return Ok(None);
  };

  Ok(
    one(db, "SELECT 1 FROM base_prompts WHERE name = $1", vec![name.into()])
      .await?
      .is_none()
      .then(|| fill_marks(&CONTROL_WORDS.accounts.base_prompt, &[("name", name)])),
  )
}

/// The id minted for a new account from the name the owner gave it: never the owner's to type, since every
/// record keys on it, and made even for a name with nothing usable in it.
pub fn minted_id(name: &str, taken: &[&str]) -> Result<String, String> {
  let mut slug = String::new();

  for letter in name.to_lowercase().chars() {
    if letter.is_ascii_lowercase() || letter.is_ascii_digit() || matches!(letter, '.' | '_' | '-') {
      slug.push(letter);
    } else if !slug.ends_with('\u{0}') {
      slug.push('\u{0}');
    }
  }

  let slug = slug.replace('\u{0}', "-");
  let slug =
    slug.trim_start_matches(|letter: char| !letter.is_ascii_lowercase() && !letter.is_ascii_digit());
  let slug = slug.trim_end_matches(['-', '.', '_']);
  let slug: String = slug.chars().take(32).collect();
  let stem = if ID.is_match(&slug) { slug } else { "account".to_owned() };

  if !taken.contains(&stem.as_str()) {
    return Ok(stem);
  }

  let head: String = stem.chars().take(28).collect();

  (2..1000)
    .map(|next| format!("{head}-{next}"))
    .find(|candidate| !taken.contains(&candidate.as_str()))
    .ok_or_else(|| CONTROL_WORDS.accounts.no_id.clone())
}

/// Why a name cannot be an account's beside the names others hold, or nothing.
pub fn name_fault(name: &str, taken: &[&str]) -> Option<String> {
  let given = name.trim();

  if given.is_empty() {
    return Some(CONTROL_WORDS.accounts.name_empty.clone());
  }

  if length(given) > NAME_MAX {
    return Some(fill_marks(&CONTROL_WORDS.accounts.name_long, &[("most", &NAME_MAX.to_string())]));
  }

  taken
    .contains(&given)
    .then(|| fill_marks(&CONTROL_WORDS.accounts.name_taken, &[("name", given)]))
}

// Why a path cannot be reached, or nothing: it must be absolute and there, a directory where one is
// asked and a file otherwise, with the words a path that is not one says.
fn path_fault(path: &Path, directory: bool, not_it: &str) -> Option<String> {
  let words = &CONTROL_WORDS.accounts;

  if path.as_os_str().is_empty() {
    return Some(words.path_empty.clone());
  }

  if !path.is_absolute() {
    return Some(words.path_relative.clone());
  }

  if !path.exists() {
    return Some(words.path_missing.clone());
  }

  (path.is_dir() != directory).then(|| not_it.to_owned())
}

/// Why a credential cannot be reached, or nothing: a directory account needs a directory that is
/// there, a token account the file its token sits in, an endpoint account a configuration home that is
/// there, an http or https base URL and the file its key sits in, and an `api-key` account a configuration
/// home that is there and the secret its key sits in. No file is opened.
pub fn credential_fault(credential: &Credential) -> Option<String> {
  let words = &CONTROL_WORDS.accounts;

  match credential {
    Credential::Directory { home } => path_fault(home, true, &words.path_not_directory),
    Credential::Token { file } => path_fault(file, false, &words.path_not_file),
    Credential::Endpoint {
      home,
      url,
      key_file,
      ..
    } => path_fault(home, true, &words.path_not_directory)
      .or_else(|| {
        url::Url::parse(url)
          .ok()
          .filter(|url| matches!(url.scheme(), "http" | "https") && url.host().is_some())
          .is_none()
          .then(|| words.endpoint_url.clone())
      })
      .or_else(|| path_fault(key_file, false, &words.key_not_file)),
    Credential::ApiKey { home, key_file } => path_fault(home, true, &words.path_not_directory)
      .or_else(|| path_fault(key_file, false, &words.key_not_file)),
  }
}

// The columns a credential is kept in, each in the order the statements name them: its kind, its path,
// an endpoint account's base URL, key file and model map, and an `api-key` account's secret, none on the
// other kinds.
fn columns(credential: &Credential) -> [sea_orm::Value; 5] {
  let (url, key_file, models) = match credential {
    Credential::Endpoint {
      url,
      key_file,
      models,
      ..
    } => (
      Some(url.clone()),
      Some(key_file.to_string_lossy().into_owned()),
      Some(serde_json::to_string(models).unwrap_or_default()),
    ),
    Credential::ApiKey { key_file, .. } => (None, Some(key_file.to_string_lossy().into_owned()), None),
    Credential::Directory { .. } | Credential::Token { .. } => (None, None, None),
  };

  [
    credential.kind().word().into(),
    credential.path().to_string_lossy().into_owned().into(),
    url.into(),
    key_file.into(),
    models.into(),
  ]
}

const CREDENTIAL_COLUMNS: [&str; 5] =
  ["credential_kind", "credential_path", "endpoint_url", "key_file", "endpoint_models"];

/// One account by its id, or by the name the owner calls it.
pub async fn account_named(db: &DatabaseConnection, named: &str) -> Result<Option<Account>, Refused> {
  let held = accounts(db).await?;
  let by_id = held.iter().position(|one| one.id == named);
  let by_name = || held.iter().position(|one| one.name == named.trim());

  Ok(by_id.or_else(by_name).map(|at| held[at].clone()))
}

/// Whom an account added belongs to and where its login stands: a person's, pinned to them, and a login
/// signed in on their node, whose directory stands there; none of either for the platform's (docs/supply.md,
/// 「What a person brings」).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Owning {
  pub user_id: Option<i64>,
  /// The organisation unit an account belongs to in place of a person (docs/supply.md, 「Organisation supply」).
  pub unit_id: Option<i64>,
  pub node: Option<String>,
}

/// An account the owner named added at the end of the order, minted its own id, or the refusal. The
/// name, the credential's parts and the base prompt it names are stored with their edges trimmed; a
/// relative path is refused, never resolved.
pub async fn add(
  db: &DatabaseConnection,
  name: &str,
  given: &CredentialGiven,
  base_prompt: BasePromptGiven,
) -> Result<(), Refused> {
  add_owned(db, name, given, base_prompt, &Owning::default()).await.map(|_| ())
}

/// An account added as `add` adds one, belonging to whom `owning` names, answered with the id it was
/// minted. A login on a person's node names a directory there, which this machine cannot look at: its path
/// is held to being absolute alone.
pub async fn add_owned(
  db: &DatabaseConnection,
  name: &str,
  given: &CredentialGiven,
  base_prompt: BasePromptGiven,
  owning: &Owning,
) -> Result<String, Refused> {
  let held = accounts(db).await?;
  let ids: Vec<&str> = held.iter().map(|one| one.id.as_str()).collect();
  let names: Vec<&str> = held.iter().map(|one| one.name.as_str()).collect();
  let id = minted_id(name, &ids).map_err(Refused::failed)?;
  let asked = given.over(None).map_err(Refused::failed)?;
  let reached = |credential: &Credential| match owning.node {
    Some(_) => (!credential.path().is_absolute()).then(|| CONTROL_WORDS.accounts.path_relative.clone()),
    None => credential_fault(credential),
  };

  if let Some(fault) = name_fault(name, &names).or_else(|| reached(&asked)) {
    return Err(Refused::failed(fault));
  }
  if let Some(fault) = base_prompt_fault(db, &base_prompt).await? {
    return Err(Refused::failed(fault));
  }

  let written = db.begin().await?;
  let last = one(&written, "SELECT MAX(place) AS place FROM claude_accounts", vec![])
    .await?
    .map(|row| row.try_get::<Option<i64>>("", "place"))
    .transpose()?
    .flatten();

  let mut values: Vec<sea_orm::Value> = vec![
    id.clone().into(),
    name.trim().into(),
    (last.unwrap_or(0) + 1).into(),
    now_ms().div_euclid(1000).into(),
  ];

  values.extend(columns(&asked));
  values.push(base_prompt.named_prompt().map(str::to_owned).into());
  values.push(owning.user_id.into());
  values.push(owning.node.clone().into());
  values.push(owning.unit_id.into());
  run(
    &written,
    &format!(
      "INSERT INTO claude_accounts (id, name, place, enabled, added_at, {}, base_prompt, user_id, node, unit_id)
       VALUES ($1, $2, $3, 1, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
      CREDENTIAL_COLUMNS.join(", ")
    ),
    values,
  )
  .await?;
  written.commit().await?;

  Ok(id)
}

/// The organisation unit an account of no person's belongs to, or none to give it back to the platform; a
/// person's account is refused, and an account given to a unit is lent no more (docs/supply.md, 「Organisation
/// supply」).
pub async fn set_unit(db: &DatabaseConnection, id: &str, unit: Option<i64>) -> Result<(), Refused> {
  let held = accounts(db).await?;
  let Some(account) = held.iter().find(|one| one.id == id) else {
    return Err(Refused::failed(no_such(id)));
  };

  if account.user_id.is_some() {
    return Err(
      Refused::new(
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        fill_marks(&CONTROL_WORDS.accounts.not_unit_owned, &[("name", &account.name)]),
      )
      .with_code("persons-own"),
    );
  }

  run(
    db,
    "UPDATE claude_accounts SET unit_id = $1 WHERE id = $2",
    vec![unit.into(), id.into()],
  )
  .await?;

  Ok(())
}

/// Whether an account, whoever owns it, lends itself to another person's run on a platform source: the one
/// switch, which the caller has already been judged the administrator to set (docs/supply.md, 「The lendable
/// accounts」).
pub async fn set_lendable(db: &DatabaseConnection, id: &str, lendable: bool) -> Result<(), Refused> {
  let held = accounts(db).await?;

  if !held.iter().any(|one| one.id == id) {
    return Err(Refused::failed(no_such(id)));
  }

  run(
    db,
    "UPDATE claude_accounts SET lendable = $1 WHERE id = $2",
    vec![lendable.into(), id.into()],
  )
  .await?;

  Ok(())
}

/// What the owner changed about one account by its id — its name, its credential, its switch, the base
/// prompt its runs open on — or the refusal: only what was named is judged, and the last account that is
/// on stays on. A credential given in part keeps the rest of the one the account holds, a base prompt
/// left out keeps the one it holds, and one given must be a prompt that is kept. Any change to an
/// endpoint account lifts its mark of waiting for a person, since a balance topped up at the endpoint
/// leaves nothing on this side to read (docs/cache-fallback.md, 「A login that is about to end」).
pub async fn update(
  db: &DatabaseConnection,
  id: &str,
  name: Option<&str>,
  given: Option<&CredentialGiven>,
  enabled: Option<bool>,
  base_prompt: BasePromptGiven,
) -> Result<(), Refused> {
  let held = accounts(db).await?;
  let Some(account) = held.iter().find(|one| one.id == id) else {
    return Err(Refused::failed(no_such(id)));
  };

  let others: Vec<&Account> = held.iter().filter(|one| one.id != id).collect();
  let names: Vec<&str> = others.iter().map(|one| one.name.as_str()).collect();
  let credential = given
    .filter(|given| !given.is_empty())
    .map(|given| given.over(Some(&account.credential)))
    .transpose()
    .map_err(Refused::failed)?;
  // A key account's mark lifts by any change, as its key written again is all a person does to it.
  let endpoint = |credential: &Credential| {
    matches!(credential.kind(), CredentialKind::Endpoint | CredentialKind::ApiKey)
  };
  let base = base_prompt.to_write();
  let lift_mark = account.needs_login_at.is_some()
    && (endpoint(&account.credential) || credential.as_ref().is_some_and(endpoint))
    && (name.is_some() || enabled.is_some() || credential.is_some() || base.is_some());
  let fault = name
    .and_then(|name| name_fault(name, &names))
    .or_else(|| credential.as_ref().and_then(credential_fault))
    .or_else(|| {
      (enabled == Some(false) && !others.iter().any(|one| one.enabled))
        .then(|| CONTROL_WORDS.accounts.last_off.as_str().into())
    });

  if let Some(fault) = fault {
    return Err(Refused::failed(fault));
  }
  if let Some(fault) = base_prompt_fault(db, &base_prompt).await? {
    return Err(Refused::failed(fault));
  }

  let mut sets = Vec::new();
  let mut values: Vec<sea_orm::Value> = Vec::new();
  let mut set = |column: &str, value: sea_orm::Value| {
    values.push(value);
    sets.push(format!("{column} = ${}", values.len()));
  };

  if let Some(name) = name {
    set("name", name.trim().into());
  }

  if let Some(enabled) = enabled {
    set("enabled", i64::from(enabled).into());
  }

  if let Some(credential) = &credential {
    for (column, value) in CREDENTIAL_COLUMNS.into_iter().zip(columns(credential)) {
      set(column, value);
    }
  }

  if let Some(base) = base {
    set("base_prompt", base.into());
  }

  if lift_mark {
    set("needs_login_at", Option::<i64>::None.into());
  }

  if !sets.is_empty() {
    values.push(id.into());
    run(
      db,
      &format!("UPDATE claude_accounts SET {} WHERE id = ${}", sets.join(", "), values.len()),
      values,
    )
    .await?;
  }

  // Another home, token file or endpoint is another login: the windows read of the one before are not this
  // account's any more (docs/usage-windows.md, 「What reads the windows」).
  if credential.as_ref().is_some_and(|credential| *credential != account.credential) {
    super::claude_accounts::forget_windows(db, id).await?;
  }

  let_choice_go_if_off(db).await
}

/// One account removed by its id, or the refusal: the last one stays, since every road needs an
/// account to name.
pub async fn remove(db: &DatabaseConnection, id: &str) -> Result<(), Refused> {
  let held = accounts(db).await?;

  if !held.iter().any(|one| one.id == id) {
    return Err(Refused::failed(no_such(id)));
  }

  if held.len() == 1 {
    return Err(Refused::failed(CONTROL_WORDS.accounts.last_gone.as_str()));
  }

  run(db, "DELETE FROM claude_accounts WHERE id = $1", vec![id.into()]).await?;

  let_choice_go_if_off(db).await
}

/// An order over exactly the accounts held, as the entries it saves: every one once, its id alone —
/// keeping the switch it has — or with its on/off for all of them, never the two mixed.
pub fn order_to_save(value: &Value, held: &[Account]) -> Option<Vec<(String, bool)>> {
  let entries = value.as_array().filter(|entries| entries.len() == held.len())?;
  let read: Vec<(String, bool)> = if entries.iter().all(Value::is_string) {
    entries
      .iter()
      .map(|one| {
        let id = one.as_str()?;

        Some((id.to_owned(), held.iter().find(|held| held.id == id)?.enabled))
      })
      .collect::<Option<_>>()?
  } else {
    entries
      .iter()
      .map(|one| Some((one.get("id")?.as_str()?.to_owned(), one.get("enabled")?.as_bool()?)))
      .collect::<Option<_>>()?
  };
  let unique = read
    .iter()
    .enumerate()
    .all(|(at, (id, _))| read[..at].iter().all(|(before, _)| before != id));

  (unique && read.iter().all(|(id, _)| held.iter().any(|one| &one.id == id))).then_some(read)
}

/// The order the owner put the accounts in, places and switches written as one, or the refusal of
/// one leaving every account off.
pub async fn set_order(db: &DatabaseConnection, order: &[(String, bool)]) -> Result<(), Refused> {
  if !order.iter().any(|(_, enabled)| *enabled) {
    return Err(Refused::failed(CONTROL_WORDS.accounts.last_off.as_str()));
  }

  let written = db.begin().await?;

  for (place, (id, enabled)) in order.iter().enumerate() {
    run(
      &written,
      "UPDATE claude_accounts SET place = $1, enabled = $2 WHERE id = $3",
      vec![(place as i64 + 1).into(), i64::from(*enabled).into(), id.clone().into()],
    )
    .await?;
  }

  written.commit().await?;

  let_choice_go_if_off(db).await
}

/// One account chosen to run on now, which moves the login in use with it; or the choice let go,
/// which hands the road back to the order.
pub async fn choose(db: &DatabaseConnection, id: Option<&str>) -> Result<(), Refused> {
  let Some(id) = id else {
    for key in [IN_USE_KEY, CHOSEN_KEY] {
      run(db, r#"DELETE FROM "settings" WHERE "key" = $1"#, vec![key.into()]).await?;
    }

    return Ok(());
  };

  match accounts(db).await?.iter().find(|one| one.id == id) {
    None => return Err(Refused::failed(no_such(id))),
    Some(account) if !account.enabled => {
      return Err(Refused::failed(CONTROL_WORDS.accounts.chosen_off.as_str()));
    }
    Some(_) => {}
  }

  set_setting(db, IN_USE_KEY, id).await?;
  set_setting(db, CHOSEN_KEY, id).await
}

/// A choice naming an account that is no longer on, let go rather than left to wake again.
pub async fn let_choice_go_if_off(db: &DatabaseConnection) -> Result<(), Refused> {
  if setting(db, CHOSEN_KEY).await?.is_some() && chosen_account(db).await?.is_none() {
    run(db, r#"DELETE FROM "settings" WHERE "key" = $1"#, vec![CHOSEN_KEY.into()]).await?;
  }

  Ok(())
}
