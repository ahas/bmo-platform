//! The Claude accounts as agency reads them: the order the owner set, which of them would answer now, and
//! the login an automatic run goes out on (docs/cache-fallback.md, 「The Claude Code road is a list of
//! accounts」). A login marked as waiting for a person is asked once more of its own credential, and the
//! mark goes where the owner has signed in again. An account's windows are agency's own keeping, read with
//! how long its reader says a reading stands and its unread mark (docs/usage-windows.md).

use std::path::{Path, PathBuf};

use cache_siren::claude::{Credential as WindowCredential, MODE as CLAUDE_CODE_MODE};
use cache_siren::{Keeping, Slot};
use harness_providers::Machine;
use harness_providers::claude_code::ClaudeCode;
use policy::providers::claude_code::CLAUDE_CODE;
use policy::rest::REST;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use store::{all, run};

use super::{Refused, set_setting, setting};
use crate::now_ms;
use crate::places::Ground;

/// What the account judgement reads: the store, the keeping of every account's windows, and the data root
/// this machine keeps each account's token for the work nodes under.
#[derive(Clone, Copy)]
pub struct Judging<'a> {
  pub db: &'a DatabaseConnection,
  pub windows: &'a Keeping,
  pub data_root: &'a Path,
}

impl<'a> Judging<'a> {
  /// The judgement on agency's ground.
  pub fn of(ground: &'a Ground) -> Self {
    Self {
      db: &ground.db,
      windows: &ground.windows,
      data_root: &ground.roots.data,
    }
  }
}

/// The account a fresh store is seeded with, standing for the login this machine already holds.
pub const SEED_ACCOUNT: &str = "default";

// The settings the walk reads: the account chosen to run on now, and the login in use.
pub(crate) const CHOSEN_KEY: &str = "claude_account_chosen";
pub(crate) const IN_USE_KEY: &str = "claude_account_in_use";

// The file a directory account's login sits in.
pub const CREDENTIAL_FILE: &str = ".credentials.json";

/// What the mark of an account's token for the work nodes found gone is kept under, the account's id after
/// it (docs/work-nodes.md, 「The logins on a node」).
pub const NODE_TOKEN_GONE_KEY: &str = "claude_node_token_gone:";

// A token carries no ending of its own, so what stands for it is the year it was issued for, as Claude
// Code declares it.
fn token_life_ms() -> f64 {
  CLAUDE_CODE.login.token_life_days as f64 * 24.0 * 60.0 * 60_000.0
}

/// The ways a credential is reached, by the word the store and the owner name each by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialKind {
  Directory,
  Token,
  Endpoint,
  ApiKey,
}

impl CredentialKind {
  pub const ALL: [Self; 4] = [Self::Directory, Self::Token, Self::Endpoint, Self::ApiKey];

  /// The word the kind is stored and named by.
  pub fn word(self) -> &'static str {
    match self {
      Self::Directory => "directory",
      Self::Token => "token",
      Self::Endpoint => "endpoint",
      Self::ApiKey => "api-key",
    }
  }

  /// The kind a word names, none where it names no kind.
  pub fn named(word: &str) -> Option<Self> {
    Self::ALL.into_iter().find(|kind| kind.word() == word.trim())
  }

  /// Every kind's word, as a refusal lists them.
  pub fn words() -> String {
    Self::ALL.map(Self::word).join(", ")
  }
}

/// The endpoint's own model id for each family word Claude Code picks a model by, where the account maps
/// one; a family it does not map goes out as Claude Code names it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointModels {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub opus: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sonnet: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub haiku: Option<String>,
}

impl EndpointModels {
  /// Each family word with the endpoint's model id where the map names one.
  pub fn families(&self) -> [(&'static str, Option<&str>); 3] {
    [
      ("opus", self.opus.as_deref()),
      ("sonnet", self.sonnet.as_deref()),
      ("haiku", self.haiku.as_deref()),
    ]
  }

  /// The endpoint's model id for one family word, where the map names one.
  pub fn of_family(&self, family: &str) -> Option<&str> {
    self.families().into_iter().find(|(word, _)| *word == family).and_then(|(_, id)| id)
  }

  /// The map a text names: family words paired with model ids, separated by commas, as
  /// `opus=<id>,sonnet=<id>,haiku=<id>`, or none where a pair names no family or no id, a family twice,
  /// or no family at all.
  pub fn of_words(text: &str) -> Option<Self> {
    let mut models = Self::default();

    for pair in text.split(',').map(str::trim).filter(|pair| !pair.is_empty()) {
      let (word, id) = pair.split_once('=')?;
      let id = id.trim();

      if id.is_empty() {
        return None;
      }

      let slot = match word.trim() {
        "opus" => &mut models.opus,
        "sonnet" => &mut models.sonnet,
        "haiku" => &mut models.haiku,
        _ => return None,
      };

      if slot.is_some() {
        return None;
      }

      *slot = Some(id.to_owned());
    }

    models.families().iter().any(|(_, id)| id.is_some()).then_some(models)
  }
}

/// How a run reaches an account's login, read once from its row (docs/cache-fallback.md, 「The Claude Code
/// road is a list of accounts」): a configuration directory holding the login, a file holding a long-lived
/// token, another provider's Anthropic-compatible endpoint, reached under a configuration home of the
/// account's own with the key its file holds, or Anthropic's own API on an API key, under a configuration
/// home of its own with the key its secret under the data root's `env/` holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Credential {
  Directory {
    home: PathBuf,
  },
  Token {
    file: PathBuf,
  },
  Endpoint {
    home: PathBuf,
    url: String,
    key_file: PathBuf,
    models: EndpointModels,
  },
  ApiKey {
    home: PathBuf,
    key_file: PathBuf,
  },
}

impl Credential {
  /// The credential a row's columns name, or why they name none.
  pub fn of_columns(
    kind: &str,
    path: &str,
    url: Option<String>,
    key_file: Option<String>,
    models: Option<String>,
  ) -> Result<Self, String> {
    let path = PathBuf::from(path);

    match CredentialKind::named(kind) {
      Some(CredentialKind::Directory) => Ok(Self::Directory { home: path }),
      Some(CredentialKind::Token) => Ok(Self::Token { file: path }),
      Some(CredentialKind::ApiKey) => Ok(Self::ApiKey {
        home: path,
        key_file: key_file
          .filter(|file| !file.is_empty())
          .map(PathBuf::from)
          .ok_or("an api-key account names no secret")?,
      }),
      Some(CredentialKind::Endpoint) => Ok(Self::Endpoint {
        home: path,
        url: url.filter(|url| !url.is_empty()).ok_or("an endpoint account names no base URL")?,
        key_file: key_file
          .filter(|file| !file.is_empty())
          .map(PathBuf::from)
          .ok_or("an endpoint account names no key file")?,
        models: match models.filter(|models| !models.trim().is_empty()) {
          Some(models) => serde_json::from_str(&models).map_err(|why| format!("its model map: {why}"))?,
          None => EndpointModels::default(),
        },
      }),
      None => Err(format!("its credential kind {kind} is none of {}", CredentialKind::words())),
    }
  }

  pub fn kind(&self) -> CredentialKind {
    match self {
      Self::Directory { .. } => CredentialKind::Directory,
      Self::Token { .. } => CredentialKind::Token,
      Self::Endpoint { .. } => CredentialKind::Endpoint,
      Self::ApiKey { .. } => CredentialKind::ApiKey,
    }
  }

  /// Every variable a run is handed this credential in, which a pane is given by name: the login's own
  /// for a directory or token account, an endpoint account's configuration home, base URL, key and model
  /// ids, and an `api-key` account's configuration home and key (docs/cache-fallback.md, 「The Claude Code
  /// road is a list of accounts」).
  pub fn variables(&self) -> &'static [&'static str] {
    match self {
      Self::Directory { .. } => &["CLAUDE_CONFIG_DIR", "CLAUDE_CODE_OAUTH_TOKEN"],
      Self::Token { .. } => &["CLAUDE_CODE_OAUTH_TOKEN"],
      Self::Endpoint { .. } => &[
        "CLAUDE_CONFIG_DIR",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
      ],
      Self::ApiKey { .. } => &["CLAUDE_CONFIG_DIR", "ANTHROPIC_API_KEY"],
    }
  }

  /// The path the row keeps as the credential's: the configuration directory, the token's file, or the
  /// endpoint or `api-key` account's own configuration home.
  pub fn path(&self) -> &Path {
    match self {
      Self::Directory { home } | Self::Endpoint { home, .. } | Self::ApiKey { home, .. } => home,
      Self::Token { file } => file,
    }
  }

  /// What of an endpoint account a view reports beside its kind: its base URL, the file its key sits in
  /// and its model map, each none on another kind. The key's content is never read.
  pub fn endpoint_parts(&self) -> (Option<&str>, Option<&Path>, Option<&EndpointModels>) {
    match self {
      Self::Endpoint {
        url,
        key_file,
        models,
        ..
      } => (Some(url), Some(key_file), Some(models)),
      Self::Directory { .. } | Self::Token { .. } | Self::ApiKey { .. } => (None, None, None),
    }
  }

  /// The file its key sits in: an endpoint account's key file or an `api-key` account's secret, none on
  /// another kind. The key's content is never read here.
  pub fn key_file(&self) -> Option<&Path> {
    match self {
      Self::Endpoint { key_file, .. } | Self::ApiKey { key_file, .. } => Some(key_file),
      Self::Directory { .. } | Self::Token { .. } => None,
    }
  }

  /// The configuration home of the account's own the CLI keeps its records under, none for a token
  /// account, which goes out under the machine's.
  pub fn own_home(&self) -> Option<&Path> {
    match self {
      Self::Directory { home } | Self::Endpoint { home, .. } | Self::ApiKey { home, .. } => Some(home),
      Self::Token { .. } => None,
    }
  }

  /// The file its login sits in, none for an endpoint or `api-key` account, which holds no login.
  pub fn login_file(&self) -> Option<PathBuf> {
    match self {
      Self::Directory { home } => Some(home.join(CREDENTIAL_FILE)),
      Self::Token { file } => Some(file.clone()),
      Self::Endpoint { .. } | Self::ApiKey { .. } => None,
    }
  }

  /// The file a token account's token sits in, none for any other.
  pub fn token_file(&self) -> Option<&Path> {
    match self {
      Self::Token { file } => Some(file),
      Self::Directory { .. } | Self::Endpoint { .. } | Self::ApiKey { .. } => None,
    }
  }

  /// The credential the Claude account reader asks the provider's usage of, none for an endpoint or
  /// `api-key` account, which is paid by the request and has no windows (docs/usage-windows.md).
  pub fn window_credential(&self) -> Option<WindowCredential> {
    match self {
      Self::Directory { home } => Some(WindowCredential::Directory(home.join(CREDENTIAL_FILE))),
      Self::Token { file } => Some(WindowCredential::Token(file.clone())),
      Self::Endpoint { .. } | Self::ApiKey { .. } => None,
    }
  }

  /// Whether its login may renew itself, which only a directory's OAuth pair does.
  fn may_renew(&self) -> bool {
    matches!(self, Self::Directory { .. })
  }

  /// Whether its login stands for the year a long-lived token is issued for where it states no end.
  fn stands_a_year(&self) -> bool {
    matches!(self, Self::Token { .. })
  }

  /// Whether a run may go out on it from a work node (docs/work-nodes.md): a directory or token account
  /// by its token for the nodes, never an endpoint account, whose key file stays on this machine, nor an
  /// `api-key` account as the credential alone says it, since the platform's key stays here too; a person's
  /// `api-key` account goes out on their node all the same (`Account::runs_on_nodes`).
  pub fn runs_on_nodes(&self) -> bool {
    !matches!(self, Self::Endpoint { .. } | Self::ApiKey { .. })
  }

  /// Whether its token for the work nodes is a login apart from its login here: a directory account's,
  /// whereas a token account's own file is both, and an endpoint account has none.
  pub fn node_token_apart(&self) -> bool {
    matches!(self, Self::Directory { .. })
  }

  /// Whether a run is ever sent on it only to keep its cache warm: never on an endpoint or `api-key`
  /// account, which is paid by the request (docs/cache-fallback.md, 「Warming only enabled providers」).
  pub fn warmed(&self) -> bool {
    !matches!(self, Self::Endpoint { .. } | Self::ApiKey { .. })
  }

  /// Whether its key is there to be handed to a run: an endpoint account's key file or an `api-key`
  /// account's secret present and not empty, never opened; every other kind holds no key to judge.
  pub fn key_held(&self) -> bool {
    match self {
      Self::Endpoint { key_file, .. } | Self::ApiKey { key_file, .. } => filled(key_file),
      Self::Directory { .. } | Self::Token { .. } => true,
    }
  }

  /// Whether a login stands to be handed to a run (docs/cache-fallback.md, 「The account judgement」): a
  /// directory account's credentials file or a token account's token file present and not empty, never
  /// opened; an endpoint or `api-key` account holds no login, its key being judged apart.
  pub fn login_held(&self) -> bool {
    self.login_file().is_none_or(|file| filled(&file))
  }
}

// Whether a file stands, a file and not empty; what it holds is never read.
fn filled(path: &Path) -> bool {
  std::fs::metadata(path).is_ok_and(|held| held.is_file() && held.len() > 0)
}

/// One account as the store holds it.
#[derive(Clone, Debug)]
pub struct Account {
  pub id: String,
  pub name: String,
  pub place: i64,
  pub enabled: bool,
  pub credential: Credential,
  pub needs_login_at: Option<i64>,
  /// The user whose account it is, where the multi-user model names one.
  pub owner_account: Option<i64>,
  /// The organisation unit whose account it is, where a unit owns it in place of a person (docs/supply.md,
  /// 「Organisation supply」).
  pub owner_unit: Option<i64>,
  /// The node a person's login was signed in on and stands on, none for an account of this machine's
  /// (docs/supply.md, 「Signing a person's login in」).
  pub node: Option<String>,
  /// Whether the platform's account goes out for another person's run (docs/supply.md, 「The lendable
  /// accounts」).
  pub lendable: bool,
  /// The base prompt a worker's run on it opens on, where it names one (docs/base-prompts.md).
  pub base_prompt: Option<String>,
  /// The token for the work nodes its mark of one found gone names, as the token file's moment stood then;
  /// a token written since stands clear of it.
  pub node_token_gone: Option<String>,
}

impl Account {
  /// The account as a sentence names it: the name the owner gave it, and the identifier it is known by
  /// where the two differ.
  pub fn words(&self) -> String {
    if self.name == self.id {
      self.name.clone()
    } else {
      format!("{} ({})", self.name, self.id)
    }
  }

  /// The model a base prompt's `{{model}}` names for a run of this account: the endpoint's own id for the
  /// run's family where its map holds one, and the run's model otherwise (docs/base-prompts.md).
  pub fn base_model(&self, model: &str) -> String {
    match &self.credential {
      Credential::Endpoint { models, .. } => models
        .of_family(ClaudeCode::family(model))
        .map_or_else(|| model.to_owned(), str::to_owned),
      Credential::Directory { .. } | Credential::Token { .. } | Credential::ApiKey { .. } => {
        model.to_owned()
      }
    }
  }

  /// Whether a run may go out on it from a work node: as its credential says, and a person's or an
  /// organisation's `api-key` account besides, whose key is theirs and goes out on the node of the person whose
  /// run it carries (docs/supply.md, 「The lendable accounts」 and 「Organisation supply」).
  pub fn runs_on_nodes(&self) -> bool {
    self.credential.runs_on_nodes()
      || matches!(self.credential, Credential::ApiKey { .. })
        && (self.owner_account.is_some() || self.owner_unit.is_some())
  }

  /// Whether it is a person's login signed in on their node, whose configuration directory stands there
  /// and nowhere here.
  pub fn on_own_node(&self) -> bool {
    self.owner_account.is_some() && self.node.is_some()
  }

  /// The directory the CLI keeps this account's records under: its configuration home where it has
  /// one, and the machine's for a token account, which has none.
  pub fn records_dir(&self, machine: &Machine) -> PathBuf {
    self
      .credential
      .own_home()
      .map_or_else(|| machine_config_dir(machine), Path::to_path_buf)
  }

  /// Where its login sits, none for an endpoint account.
  pub fn credential_file(&self) -> Option<PathBuf> {
    self.credential.login_file()
  }

  /// Where this machine keeps its token for the work nodes (docs/work-nodes.md, 「The logins on a
  /// node」): a token account's own file, a file named by its id under the data root's env directory for
  /// a directory account, and none for an endpoint or `api-key` account.
  pub fn node_token_file(&self, data_root: &Path) -> Option<PathBuf> {
    match &self.credential {
      Credential::Token { file } => Some(file.clone()),
      Credential::Directory { .. } => {
        Some(data_root.join("env").join(&CLAUDE_CODE.node_login.token_dir).join(&self.id))
      }
      Credential::Endpoint { .. } | Credential::ApiKey { .. } => None,
    }
  }

  /// Its token for the work nodes, none where this machine holds none.
  pub fn node_token(&self, data_root: &Path) -> Option<String> {
    std::fs::read_to_string(self.node_token_file(data_root)?)
      .ok()
      .map(|token| token.trim().to_owned())
      .filter(|token| !token.is_empty())
  }

  /// When its token for the work nodes was written, in milliseconds, which tells one token from the
  /// next; none where this machine holds none.
  pub fn node_token_written(&self, data_root: &Path) -> Option<f64> {
    self.node_token(data_root)?;

    written_ms(&self.node_token_file(data_root)?)
  }

  /// When its token for the work nodes ends: the year it was issued for, from the day it was written.
  pub fn node_token_end(&self, data_root: &Path) -> Option<f64> {
    self.node_token_written(data_root).map(|written| written + token_life_ms())
  }

  /// Which token for the work nodes this machine holds now, as a mark names it: the moment its file was
  /// written, or none held.
  pub fn node_token_stamp(&self, data_root: &Path) -> String {
    self
      .node_token_written(data_root)
      .map_or_else(|| "none".to_owned(), crate::keepalive::logins::moment_text)
  }

  /// Its token for the work nodes where this machine holds one that is not marked gone.
  pub fn answering_node_token(&self, data_root: &Path) -> Option<String> {
    let token = self.node_token(data_root)?;

    (self.node_token_gone.as_deref() != Some(self.node_token_stamp(data_root).as_str())).then_some(token)
  }

  /// Whether its login here cannot be counted on at `now` (docs/cache-fallback.md, 「A directory account
  /// carries on by its token」): a directory account's login marked as waiting for a person, or past the end
  /// its file states, or within the policy's span before it.
  pub fn login_here_failing(&self, now: i64) -> bool {
    let ahead = CLAUDE_CODE.login.hand_over_ahead_hours as f64 * 60.0 * 60_000.0;

    self.credential.node_token_apart()
      && (self.needs_login_at.is_some() || login_end(self).is_some_and(|end| end - now as f64 <= ahead))
  }

  /// The token a run here goes out on beside the account's configuration directory at `now`: its token for
  /// the work nodes, where its login here cannot be counted on and that token answers; none otherwise, and
  /// never for a token or endpoint account, whose own credential is its login here.
  pub fn carrying_token(&self, data_root: &Path, now: i64) -> Option<String> {
    if !self.login_here_failing(now) {
      return None;
    }

    self.answering_node_token(data_root)
  }

  /// Whether a login marked as waiting for a person stands again on this machine, whose data root holds the
  /// accounts' tokens for the work nodes: as `login_back` says, and, for a directory account that carries on by
  /// such a token meanwhile, only once its credential was written after the mark, by a person signing in
  /// again. A credential that merely still states a moment ahead would lift the mark at once, send the next
  /// run out on the login that refused it, and tell the owner again (docs/cache-fallback.md, 「A login that is
  /// about to end」).
  pub fn login_back_here(&self, data_root: &Path, now: i64) -> bool {
    if !self.login_back(now) {
      return false;
    }

    if !self.credential.node_token_apart() || self.answering_node_token(data_root).is_none() {
      return true;
    }

    match (self.needs_login_at, self.credential_file().and_then(|file| written_ms(&file))) {
      (Some(marked), Some(written)) => written >= (marked + 1) as f64 * 1000.0,
      _ => true,
    }
  }

  /// Whether a login marked as waiting for a person stands again, read off its credential rather than
  /// anybody's word: a login whose credential states a moment after `now`, which somebody signed in to
  /// since, or an endpoint or `api-key` account whose key file was written after the mark and is not empty.
  pub fn login_back(&self, now: i64) -> bool {
    match &self.credential {
      Credential::Endpoint { key_file, .. } | Credential::ApiKey { key_file, .. } => {
        self.credential.key_held()
          && self.needs_login_at.is_some_and(|marked| {
            written_ms(key_file).is_some_and(|written| written > marked as f64 * 1000.0)
          })
      }
      Credential::Directory { .. } | Credential::Token { .. } => {
        credential_expiry(self).is_some_and(|expiry| expiry > now as f64)
      }
    }
  }
}

// When a file was written, in milliseconds with the fraction kept, to the last digit: a moment a notice
// is recorded under is read off the same file again next, and the two readings must agree.
fn written_ms(path: &Path) -> Option<f64> {
  let written = std::fs::metadata(path).ok()?.modified().ok()?;
  let since = written.duration_since(std::time::UNIX_EPOCH).ok()?;

  Some(since.as_secs() as f64 * 1000.0 + f64::from(since.subsec_nanos()) / 1e6)
}

/// The configuration directory the CLI itself would use: `CLAUDE_CONFIG_DIR` where it is set, and
/// `.claude` under the home otherwise.
pub fn machine_config_dir(machine: &Machine) -> PathBuf {
  machine
    .var("CLAUDE_CONFIG_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from(machine.var("HOME").unwrap_or_default()).join(".claude"))
}

/// Every account in the order the owner set, on or off. A row whose credential cannot be read is said
/// with its cause and left out, so no run goes out on a credential read as another kind.
pub async fn accounts(db: &DatabaseConnection) -> Result<Vec<Account>, Refused> {
  let rows = all(
    db,
    r#"SELECT a.id, a.name, a.place, a.enabled, a.credential_kind, a.credential_path, a.endpoint_url,
       a.key_file, a.endpoint_models, a.needs_login_at, a.owner_account, a.owner_unit, a.node, a.lendable, a.base_prompt,
       s."value" AS node_token_gone
     FROM claude_accounts a
     LEFT JOIN "settings" s ON s."key" = $1 || a.id
     ORDER BY a.place, a.rowid"#,
    vec![NODE_TOKEN_GONE_KEY.into()],
  )
  .await?;
  let mut held = Vec::new();

  for row in &rows {
    let id: String = row.try_get("", "id")?;
    let credential = Credential::of_columns(
      &row.try_get::<String>("", "credential_kind")?,
      &row.try_get::<String>("", "credential_path")?,
      row.try_get("", "endpoint_url")?,
      row.try_get("", "key_file")?,
      row.try_get("", "endpoint_models")?,
    );

    match credential {
      Ok(credential) => held.push(Account {
        id,
        name: row.try_get("", "name")?,
        place: row.try_get("", "place")?,
        enabled: row.try_get::<i64>("", "enabled")? == 1,
        credential,
        needs_login_at: row.try_get("", "needs_login_at")?,
        owner_account: row.try_get("", "owner_account")?,
        owner_unit: row.try_get("", "owner_unit")?,
        node: row.try_get("", "node")?,
        lendable: row.try_get("", "lendable")?,
        base_prompt: row.try_get("", "base_prompt")?,
        node_token_gone: row.try_get("", "node_token_gone")?,
      }),
      Err(why) => eprintln!("claude account {id}: left out, {why}"),
    }
  }

  Ok(held)
}

/// The directory every account keeps its session records under on this machine, each once, in the owner's
/// order: the logins a reader of a session's records looks in besides the machine's own
/// (docs/supervisors.md, 「Starting it afresh」).
pub async fn login_homes(db: &DatabaseConnection, machine: &Machine) -> Result<Vec<PathBuf>, Refused> {
  let mut homes: Vec<PathBuf> = vec![];

  for account in accounts(db).await? {
    let home = account.records_dir(machine);

    if !homes.contains(&home) {
      homes.push(home);
    }
  }

  Ok(homes)
}

/// A moment a setting holds in epoch seconds, read as the whole number it was written as, one written in
/// milliseconds read as the second it names.
async fn moment(db: &DatabaseConnection, key: &str) -> Result<Option<i64>, Refused> {
  Ok(
    setting(db, key)
      .await?
      .and_then(|value| leading_integer(&value))
      .map(store::settings::epoch_seconds),
  )
}

pub use store::settings::leading_integer;

/// How long one account answers nothing for its limit, in epoch seconds, or nothing: the later of the
/// rest written under it — a rest written with no login named is the seeded account's, unless it is the
/// service's outage — and a window of its own read full, not yet reset, by a reading still fresh.
/// A window is read on its reader's clock and off every answer, so a login whose window filled is closed
/// whether or not a run has met the refusal, and whatever became of the rest written for it; a reading
/// nothing renewed closes nothing, so the next run renews the login's token or meets the refusal that rests
/// it.
pub async fn rest_until(judging: Judging<'_>, account: &str) -> Result<Option<i64>, Refused> {
  let db = judging.db;
  let own = match moment(db, &format!("rest_until:{account}")).await? {
    Some(own) => Some(own),
    None if account == SEED_ACCOUNT && setting(db, "rest_kind").await?.as_deref() != Some("outage") => {
      moment(db, "rest_until").await?
    }
    None => None,
  };

  Ok(own.max(full_window_until(judging.windows, account, now_ms().div_euclid(1000)).await))
}

/// The moment the latest of an account's windows read full resets, where one is and has not reset
/// by `now`: what closes the login by its windows, and the windows' confirmation that a limit a run met is
/// the login's own limit. A full window that names its reset closes the login until then however old its
/// reading is, since only a newer reading, which replaces it, can say it came back early. A full window that
/// named no reset closes it only where the reading stands — within the span its reader says a reading
/// stands, and after the account's unread mark — for the policy's fallback after it was read. A keeping that
/// cannot be read is said with its cause and closes nothing: the next run meets the refusal that rests the
/// login.
pub async fn full_window_until(windows: &Keeping, account: &str, now: i64) -> Option<i64> {
  let reading = match windows
    .read(Slot {
      mode: CLAUDE_CODE_MODE,
      account,
    })
    .await
  {
    Ok(reading) => reading,
    Err(fault) => {
      eprintln!("claude account {account}: its windows could not be read: {fault}");
      return None;
    }
  };
  let mut until = None;

  for kept in &reading.windows {
    let window = &kept.window;

    if !window.is_full() {
      continue;
    }

    let reset = match window.resets_at {
      Some(reset) => reset,
      None if reading.stands(kept, now) => kept.seen_at + REST.fallback_seconds,
      None => continue,
    };

    if reset > now {
      until = until.max(Some(reset));
    }
  }

  until
}

/// The soonest moment, after `now`, any window kept of an account resets: what the account's windows say of
/// when its usage next moves, read whatever the reading's age, since a reset moment stays true until it
/// passes. None where no window kept names one ahead.
pub async fn next_reset(windows: &Keeping, account: &str, now: i64) -> Option<i64> {
  let slot = Slot {
    mode: CLAUDE_CODE_MODE,
    account,
  };
  let reading = match windows.read(slot).await {
    Ok(reading) => reading,
    Err(fault) => {
      eprintln!("claude account {account}: its windows could not be read: {fault}");
      return None;
    }
  };

  reading
    .windows
    .iter()
    .filter_map(|kept| kept.window.resets_at)
    .filter(|reset| *reset > now)
    .min()
}

/// Whether an account answers nothing until its own window resets.
pub async fn resting(judging: Judging<'_>, account: &str, now: i64) -> Result<bool, Refused> {
  Ok(rest_until(judging, account).await?.is_some_and(|until| until > now.div_euclid(1000)))
}

// Every moment a credential file states under a name that speaks of expiring and that `named` takes, read as
// seconds or milliseconds by its own size.
fn expiries(value: &Value, found: &mut Vec<f64>, named: &dyn Fn(&str) -> bool) {
  if let Value::Object(fields) = value {
    for (name, held) in fields {
      let name = name.to_lowercase();

      if let Some(number) = held.as_f64()
        && name.contains("expir")
        && named(&name)
        && number > 1_000_000_000.0
      {
        found.push(if number > 1_000_000_000_000.0 { number } else { number * 1000.0 });
      }

      expiries(held, found, named);
    }
  } else if let Value::Array(items) = value {
    for held in items {
      expiries(held, found, named);
    }
  }
}

/// The moment a credential states: the earliest expiry in its file, or for a token that states none,
/// a year from when its file was written. Nothing where the file cannot be read or states nothing, and
/// nothing for an endpoint account, whose key states no ending.
pub fn credential_expiry(account: &Account) -> Option<f64> {
  let path = account.credential_file()?;
  let text = std::fs::read_to_string(&path).ok()?;
  let said = serde_json::from_str::<Value>(&text).ok().and_then(|held| {
    let mut found = Vec::new();

    expiries(&held, &mut found, &|_| true);
    found.into_iter().reduce(f64::min)
  });

  if !account.credential.stands_a_year() || said.is_some() {
    return said;
  }

  written_ms(&path).map(|written| written + token_life_ms())
}

/// Whether a login is waiting for a person. One marked so is asked once more of its own credential:
/// signing in again writes a new moment there, or a new key is written into an endpoint account's file,
/// and the mark goes.
async fn needs_login(judging: Judging<'_>, account: &Account, now: i64) -> Result<bool, Refused> {
  let db = judging.db;

  if account.needs_login_at.is_none() {
    return Ok(false);
  }

  if !account.login_back_here(judging.data_root, now) {
    return Ok(true);
  }

  run(
    db,
    "UPDATE claude_accounts SET needs_login_at = NULL WHERE id = $1",
    vec![account.id.clone().into()],
  )
  .await?;
  // The login signed in again may be another one than the login read before it went.
  forget_windows(db, &account.id).await?;
  eprintln!("claude account {}: its login is standing again", account.id);

  Ok(false)
}

/// An account's kept windows and its unread mark let go, as its login changes under it: the login that
/// answers next is read afresh before the account reports a window again (docs/usage-windows.md, 「What
/// reads the windows」).
pub async fn forget_windows(db: &DatabaseConnection, account: &str) -> Result<(), Refused> {
  let keeping = Keeping::new(db.clone(), vec![]);
  let slot = Slot {
    mode: CLAUDE_CODE_MODE,
    account,
  };

  keeping.replace(slot, &[], now_ms().div_euclid(1000)).await?;
  keeping.lift_unread(account).await?;

  Ok(())
}

// The logins that are on, in the order the road takes them: the account the owner chose where one was
// chosen, so it carries the next run whenever it can answer; then the login in use; then the owner's order.
async fn road_order(db: &DatabaseConnection) -> Result<Vec<Account>, Refused> {
  let enabled: Vec<Account> = accounts(db).await?.into_iter().filter(|one| one.enabled).collect();
  let mut heads = Vec::new();

  for key in [CHOSEN_KEY, IN_USE_KEY] {
    if let Some(id) = setting(db, key).await?
      && enabled.iter().any(|one| one.id == id)
      && !heads.contains(&id)
    {
      heads.push(id);
    }
  }

  let (mut first, rest): (Vec<Account>, Vec<Account>) =
    enabled.into_iter().partition(|one| heads.contains(&one.id));

  first.sort_by_key(|one| heads.iter().position(|head| *head == one.id));

  Ok(first.into_iter().chain(rest).collect())
}

/// Why a login cannot answer now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginClosed {
  /// The owner turned it off.
  Off,
  /// An endpoint account whose key file is missing or empty.
  Key,
  /// It waits for a person to sign in again, or to settle its endpoint's key or balance.
  Person,
  /// It holds no login on this machine: it was never signed in, or its login was taken away.
  NoLogin,
  /// A window of its own is full, or a rest stands under it, until this epoch second.
  Resting(i64),
  /// The model the run asks for rests on it, until this epoch second; it stays open to every other model.
  ModelResting { model: String, until: i64 },
  /// An endpoint account whose balance, as last read, is at or below zero.
  Balance,
}

impl LoginClosed {
  /// The reason in the road's words, naming the account.
  pub fn reason(&self, account: &Account) -> String {
    let words = &policy::roads::ROADS.logins;
    let line = match self {
      Self::Off => &words.off,
      Self::Key => &words.key,
      Self::Person => &words.person,
      Self::NoLogin => &words.no_login,
      Self::Resting(_) => &words.resting,
      Self::ModelResting { .. } => &words.model_resting,
      Self::Balance => &words.balance,
    };
    let (until, model) = match self {
      Self::Resting(until) => (moment_words(*until), ""),
      Self::ModelResting { model, until } => (moment_words(*until), model.as_str()),
      _ => (String::new(), ""),
    };

    policy::fill_marks(line, &[("account", &account.name), ("until", &until), ("model", model)])
  }

  /// What a person does to open it again, where a person can: the sign-in command with the account's own
  /// configuration directory, as the login notice words it, or where the balance of the provider whose
  /// endpoint the account stands on is topped up. None where it opens on its own or by the owner's switch.
  pub fn command(&self, account: &Account, machine: &Machine) -> Option<String> {
    let wording = &CLAUDE_CODE.login;

    match (self, &account.credential) {
      (Self::Person | Self::NoLogin, Credential::Directory { .. }) => Some(
        wording
          .directory_command
          .replace("{path}", &account.credential.path().to_string_lossy()),
      ),
      (Self::Person | Self::NoLogin, Credential::Token { .. }) => {
        Some(wording.token_command.replace("{path}", &account.credential.path().to_string_lossy()))
      }
      (Self::Person | Self::Balance, Credential::Endpoint { url, .. }) => harness_providers::providers()
        .into_iter()
        .filter_map(|provider| provider.balance())
        .find(|offers| offers.serves(url, machine))
        .map(|offers| offers.declared().top_up.clone()),
      _ => None,
    }
  }
}

/// An epoch second as the road's words say a moment: its RFC 3339 form.
pub fn moment_words(at: i64) -> String {
  chrono::DateTime::from_timestamp(at, 0).map_or_else(|| at.to_string(), |moment| moment.to_rfc3339())
}

/// Why a login cannot answer now a run on `model`, or on no model in particular where none is named; none
/// where it can: it is on, an endpoint account's key file is there and not empty, a directory or token
/// account holds a login here (a person's login on their node is theirs to hold), its login has not gone, no
/// window of its own is full, an endpoint account's balance as last read is above zero, and the model named
/// does not rest on it. This is the one judgement every run of every line is offered a login by.
pub async fn closed_why(
  judging: Judging<'_>,
  account: &Account,
  model: Option<&str>,
  now: i64,
) -> Result<Option<LoginClosed>, Refused> {
  if !account.enabled {
    return Ok(Some(LoginClosed::Off));
  }

  if !account.credential.key_held() {
    return Ok(Some(LoginClosed::Key));
  }

  if !account.on_own_node() && !account.credential.login_held() {
    return Ok(Some(LoginClosed::NoLogin));
  }

  // A directory account that carries on by its token for the work nodes answers on it, mark and all.
  if needs_login(judging, account, now).await? && account.carrying_token(judging.data_root, now).is_none() {
    return Ok(Some(LoginClosed::Person));
  }

  if let Some(until) = rest_until(judging, &account.id).await?.filter(|until| *until > now.div_euclid(1000))
  {
    return Ok(Some(LoginClosed::Resting(until)));
  }

  if matches!(account.credential, Credential::Endpoint { .. }) {
    match judging.windows.balance_of_account(&account.id).await {
      Ok(Some(kept)) if kept.balance.amount <= 0.0 => return Ok(Some(LoginClosed::Balance)),
      Ok(_) => {}
      Err(fault) => eprintln!("claude account {}: its balance could not be read: {fault}", account.id),
    }
  }

  if let Some(model) = model
    && let Some(until) =
      crate::rest::model_rest_until(judging, &account.id, model, now.div_euclid(1000)).await?
  {
    return Ok(Some(LoginClosed::ModelResting {
      model: model.to_owned(),
      until,
    }));
  }

  Ok(None)
}

async fn open_now(
  judging: Judging<'_>,
  account: &Account,
  model: Option<&str>,
  now: i64,
) -> Result<bool, Refused> {
  Ok(closed_why(judging, account, model, now).await?.is_none())
}

/// The logins the road offers a run pinned to none, in order: the account the owner chose where one was
/// chosen and it can answer, then the login in use while it can, then the open ones in the owner's order.
/// A login that cannot answer is never offered, so where none is open none is.
pub async fn open_accounts(judging: Judging<'_>) -> Result<Vec<Account>, Refused> {
  let now = now_ms();
  let mut open = Vec::new();

  for account in road_order(judging.db).await? {
    if open_now(judging, &account, None, now).await? {
      open.push(judged(account, judging.data_root, now));
    }
  }

  Ok(open)
}

/// An account as the judgement leaves it: a mark its own credential lifted is gone from it too, so the run it
/// is offered to is handed the login a person signed in to rather than the token it carried on by.
pub fn judged(mut account: Account, data_root: &Path, now: i64) -> Account {
  if account.needs_login_at.is_some() && account.login_back_here(data_root, now) {
    account.needs_login_at = None;
  }

  account
}

/// The account the owner chose to run on now, where the one chosen is on.
pub async fn chosen_account(db: &DatabaseConnection) -> Result<Option<Account>, Refused> {
  let Some(id) = setting(db, CHOSEN_KEY).await? else {
    return Ok(None);
  };

  Ok(accounts(db).await?.into_iter().find(|one| one.enabled && one.id == id))
}

/// The user who administers the platform, where the multi-user model names one.
pub async fn administrator(db: &DatabaseConnection) -> Result<Option<i64>, Refused> {
  Ok(
    setting(db, store::settings::ADMINISTRATOR)
      .await?
      .and_then(|value| leading_integer(&value)),
  )
}

/// Whether an account is the platform's own, given the user who administers it: one of no person's and no
/// organisation's, or of the administrator's, and every account of no organisation while the multi-user model
/// names no administrator.
pub fn platform_owned(account: &Account, administrator: Option<i64>) -> bool {
  account.owner_unit.is_none()
    && (account.owner_account.is_none()
      || administrator.is_none()
      || account.owner_account == administrator)
}

/// The logins a run may go out on (docs/cache-fallback.md, 「The Claude Code road is a list of accounts」).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carrying {
  /// The platform's own work's: the platform's accounts and the administrator's, lent or not.
  Platform,
  /// One person's own accounts alone.
  Own(i64),
  /// The accounts marked lendable, whoever owns them, save the borrowing person's own, which that person's
  /// run on a platform source walks (docs/supply.md, 「The lendable accounts」).
  Lent(i64),
  /// One organisation unit's own accounts alone, which a run of a person under it on organisation supply walks
  /// (docs/supply.md, 「Organisation supply」).
  Organisation(i64),
}

impl From<Option<i64>> for Carrying {
  fn from(user: Option<i64>) -> Self {
    match user {
      Some(user) => Self::Own(user),
      None => Self::Platform,
    }
  }
}

/// The logins a run may go out on for whom it carries, whether or not they answer now, in the order the
/// road takes them: one person's own logins alone, the lendable ones of others — none for a person whose node
/// other accounts share (docs/supply.md, 「The lendable accounts」) — or for no person the platform's own work's.
pub async fn carried_for_user(
  db: &DatabaseConnection,
  user: impl Into<Carrying>,
) -> Result<Vec<Account>, Refused> {
  let carrying = user.into();

  // A login lent to a person whose node others share would stand where their programs read it, so none is.
  if let Carrying::Lent(borrower) = carrying
    && crate::person_nodes::shared_node(db, &borrower.to_string()).await?.is_some()
  {
    return Ok(Vec::new());
  }

  let administrator = if carrying == Carrying::Platform { administrator(db).await? } else { None };
  let carries = |account: &Account| match carrying {
    Carrying::Own(user) => account.owner_account == Some(user),
    Carrying::Platform => platform_owned(account, administrator),
    Carrying::Lent(borrower) => account.lendable && account.owner_account != Some(borrower),
    Carrying::Organisation(unit) => account.owner_unit == Some(unit),
  };

  Ok(road_order(db).await?.into_iter().filter(|one| carries(one)).collect())
}

/// The accounts marked lendable that `borrower`'s run on a platform source may go out on, in the order the
/// road takes them (docs/supply.md, 「The lendable accounts」).
pub async fn lent_accounts(db: &DatabaseConnection, borrower: i64) -> Result<Vec<Account>, Refused> {
  carried_for_user(db, Carrying::Lent(borrower)).await
}

/// The logins a run on `model` is offered for whom it carries: of the logins it may go out on, those that
/// answer that model now, or any model where none is named.
pub async fn walk_for_user(
  judging: Judging<'_>,
  user: impl Into<Carrying>,
  model: Option<&str>,
) -> Result<Vec<Account>, Refused> {
  let now = now_ms();
  let mut open = Vec::new();

  for account in carried_for_user(judging.db, user).await? {
    if open_now(judging, &account, model, now).await? {
      open.push(judged(account, judging.data_root, now));
    }
  }

  Ok(open)
}

/// The logins the platform's own work is offered, in order.
pub async fn walk_for(judging: Judging<'_>) -> Result<Vec<Account>, Refused> {
  walk_for_user(judging, None, None).await
}

/// The moment, in epoch seconds, the first of these logins that rests on its limit, or whose `model` rests on
/// it, answers that model again; none where none of them rests, every one of them waiting for a person
/// instead.
pub async fn reopens_at(
  judging: Judging<'_>,
  closed: &[Account],
  model: Option<&str>,
) -> Result<Option<i64>, Refused> {
  let now = now_ms().div_euclid(1000);
  let mut first = None;

  for account in closed {
    let own = rest_until(judging, &account.id).await?.filter(|until| *until > now);
    let until = match (own, model) {
      (Some(until), _) => until,
      (None, Some(model)) => match crate::rest::model_rest_until(judging, &account.id, model, now).await? {
        Some(until) => until,
        None => continue,
      },
      (None, None) => continue,
    };

    first = Some(first.map_or(until, |first: i64| first.min(until)));
  }

  Ok(first)
}

/// Whether a login answers now: it is on, its own window is not full, and its login has not gone.
pub async fn account_open(judging: Judging<'_>, id: &str, now: i64) -> Result<bool, Refused> {
  let Some(account) = accounts(judging.db).await?.into_iter().find(|one| one.id == id) else {
    return Ok(false);
  };

  open_now(judging, &account, None, now).await
}

/// The login the road's runs go out on now, where one is held and is still on.
pub async fn in_use(db: &DatabaseConnection) -> Result<Option<String>, Refused> {
  let enabled = accounts(db).await?;

  Ok(
    setting(db, IN_USE_KEY)
      .await?
      .filter(|id| enabled.iter().any(|one| one.enabled && &one.id == id)),
  )
}

/// That the road's runs go out on this login now, written only where it moved.
pub async fn hold_in_use(db: &DatabaseConnection, id: &str) -> Result<(), Refused> {
  if setting(db, IN_USE_KEY).await?.as_deref() != Some(id) {
    set_setting(db, IN_USE_KEY, id).await?;
  }

  Ok(())
}

// Whether a credential file holds something to renew itself with: a name that speaks of refreshing
// with a value under it, which is never read.
fn renews(value: &Value) -> bool {
  match value {
    Value::Object(fields) => fields.iter().any(|(name, held)| {
      (name.to_lowercase().contains("refresh") && held.as_str().is_some_and(|text| !text.is_empty()))
        || renews(held)
    }),
    Value::Array(items) => items.iter().any(renews),
    _ => false,
  }
}

// The earliest moment a credential file states under a name that speaks of refreshing and of expiring: when
// its refresh itself stops being taken.
fn refresh_end(value: &Value) -> Option<f64> {
  let mut found = Vec::new();

  expiries(value, &mut found, &|name| name.contains("refresh"));
  found.into_iter().reduce(f64::min)
}

/// When a login ends in the only sense worth telling the owner: where its file holds a refresh, which the
/// CLI renews the login by for itself, the moment the file states that refresh stops being taken, and
/// nothing where it states none; the moment its credential states otherwise; nothing for an endpoint
/// account, which has no login to end.
pub fn login_end(account: &Account) -> Option<f64> {
  let held = account
    .credential
    .may_renew()
    .then(|| account.credential_file())
    .flatten()
    .and_then(|file| std::fs::read_to_string(file).ok())
    .and_then(|text| serde_json::from_str::<Value>(&text).ok());

  match held {
    Some(held) if renews(&held) => refresh_end(&held),
    _ => credential_expiry(account),
  }
}

// How long before its end a login is told as ending.
fn notice_ahead_ms() -> f64 {
  CLAUDE_CODE.login.notice_ahead_hours as f64 * 60.0 * 60_000.0
}

/// How a login stands: none where this machine holds no login for it (the account judgement's own test, so a
/// CLI never signed in is never told as standing; a person's login on their node is theirs), gone where it
/// waits for a person or is past its end, ending inside the notice window, standing otherwise, a login whose
/// end nothing can read included.
pub fn login_standing(account: &Account, now: i64) -> &'static str {
  if !account.on_own_node() && !account.credential.login_held() {
    return "none";
  }

  if account.needs_login_at.is_some() {
    return "gone";
  }

  match login_end(account) {
    None => "standing",
    Some(end) if end <= now as f64 => "gone",
    Some(end) if end - now as f64 <= notice_ahead_ms() => "ending",
    Some(_) => "standing",
  }
}

/// The accounts as the Claude road walks them, each with how its login stands, whether the owner
/// chose it, whether automatic runs go out on it now and how long a limit rest keeps it out of the
/// walk. The path is the whole of what is shown of a credential.
pub async fn views(judging: Judging<'_>) -> Result<Vec<Value>, Refused> {
  views_of(judging, |_| true).await
}

/// The views of the accounts `shown` takes, in the road's order: what a caller acting for one person, or for
/// the platform, reads of the list (docs/api.md, 「Who a caller acts for」).
pub async fn views_of(
  judging: Judging<'_>,
  shown: impl Fn(&Account) -> bool,
) -> Result<Vec<Value>, Refused> {
  let now = now_ms();
  let chosen = chosen_account(judging.db).await?.map(|one| one.id);
  let serving = open_accounts(judging).await?.first().map(|one| one.id.clone());
  let mut views = Vec::new();

  for account in accounts(judging.db).await?.into_iter().filter(|one| shown(one)) {
    let rest = rest_until(judging, &account.id).await?.filter(|until| until * 1000 > now);
    let (endpoint, _, models) = account.credential.endpoint_parts();
    let (session, week) = super::fallback::account_headlines(judging.windows, &account).await?;
    let balance = super::fallback::account_balance(judging.windows, &account).await?;

    views.push(json!({
      "id": account.id,
      "name": account.name,
      "place": account.place,
      "enabled": account.enabled,
      "credentialKind": account.credential.kind().word(),
      "credentialPath": account.credential.path().to_string_lossy(),
      "endpointUrl": endpoint,
      "keyFile": account.credential.key_file().map(|file| file.to_string_lossy()),
      "models": models,
      "basePrompt": account.base_prompt,
      "userId": account.owner_account,
      "unitId": account.owner_unit,
      "node": account.node,
      "lendable": account.lendable,
      "login": login_standing(&account, now),
      "onToken": account.carrying_token(judging.data_root, now).is_some(),
      "loginEndsAt": login_end(&account).map(|end| (end / 1000.0).floor() as i64),
      "chosen": chosen.as_deref() == Some(account.id.as_str()),
      "serving": serving.as_deref() == Some(account.id.as_str()),
      "restUntil": rest,
      "session": session,
      "week": week,
      "balance": balance,
    }));
  }

  Ok(views)
}

/// The directory whose catalogue answers: the records directory of the login the walk would take
/// first, or of the first login that is on where none answers now, or the machine's where no account is
/// registered (docs/model-tiers.md, 「The Claude catalogue」).
pub async fn catalogue_dir(judging: Judging<'_>, machine: &Machine) -> Result<PathBuf, Refused> {
  let open = open_accounts(judging).await?;
  let first = match open.into_iter().next() {
    Some(account) => Some(account),
    None => road_order(judging.db).await?.into_iter().next(),
  };

  Ok(first.map_or_else(|| machine_config_dir(machine), |account| account.records_dir(machine)))
}
