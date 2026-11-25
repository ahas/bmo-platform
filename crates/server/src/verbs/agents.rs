//! The agents the bot runs and their settings, as a shell reads and changes them (docs/agent-control.md):
//! each supervisor's road and each role's agents' roads, and, inside the Claude Code road, the order of the
//! Claude accounts; the accounts
//! themselves; the registered models; a work key's road and login; a repository's support checkout count;
//! an agent's own thinking, effort and gates; and the listing of every agent. Each change is the one the
//! console makes, refused in the same sentence.
//!
//! Called with none of the order and account flags, both orders and the registered models print above
//! whatever else the verb does. A refusal is said on the error stream and ends the verb with 1; a call
//! the verb cannot read ends it with 2.

use std::path::{Path, PathBuf};

use bmo_agency_client::SupportCount;
use futures_util::future::BoxFuture;
use harness_providers::{ARCHIVE_PROVIDERS, Machine};
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use policy::verb_words::VERB_WORDS;
use policy::workers::WORKERS;
use sea_orm::DatabaseConnection;
use serde::Serialize;
use serde_json::{Value, json};

use super::VerbError;
use crate::control::Refused;
use crate::control::claude_account_changes::{
  self as changes, BasePromptGiven, CredentialGiven, account_named,
};
use crate::control::claude_accounts::{self, CredentialKind, EndpointModels, Judging, accounts};
use crate::control::harness::{provider_enabled, set_provider_enabled};
use crate::control::registry::{self, registrations};
use crate::now_ms;
use crate::places::Ground;
use crate::text::grouped;
use store::all;

/// What the verb is asked: every flag it reads.
#[derive(clap::Parser, Debug, Default, Clone)]
pub struct AgentsArgs {
  #[arg(long)]
  pub provider_on: Option<String>,
  #[arg(long)]
  pub provider_off: Option<String>,
  #[arg(long)]
  pub registered: bool,
  #[arg(long)]
  pub register: Option<String>,
  /// The registration's name, its node and its address.
  #[arg(long, num_args = 3, value_names = ["NAME", "NODE", "ADDRESS"])]
  pub register_address: Option<Vec<String>>,
  #[arg(long)]
  pub key_stdin: bool,
  #[arg(long)]
  pub no_key: bool,
  /// Whether the address's instance keeps role files, on or off; with `--register`, whether the card
  /// service's instances do.
  #[arg(long)]
  pub role_files: Option<String>,
  /// The arbiter service a registration's engine is, or `none`.
  #[arg(long)]
  pub card_service: Option<String>,
  /// The registration's name, and the node of the one address to take away.
  #[arg(long, num_args = 1..=2, value_names = ["NAME", "NODE"])]
  pub unregister: Option<Vec<String>>,
  #[arg(long)]
  pub context: Option<String>,
  #[arg(long)]
  pub answer: Option<String>,
  #[arg(long)]
  pub walked: Option<String>,
  #[arg(long)]
  pub research: Option<String>,
  /// The languages a registration writes results in, as a comma-separated list, or `none` for every language.
  #[arg(long, value_delimiter = ',')]
  pub languages: Option<Vec<String>>,
  #[arg(long)]
  pub account_order: Option<String>,
  #[arg(long)]
  pub account_on: Option<String>,
  #[arg(long)]
  pub account_off: Option<String>,
  #[arg(long)]
  pub account_reset: bool,
  #[arg(long)]
  pub account_use: Option<String>,
  #[arg(long)]
  pub account_add: Option<String>,
  #[arg(long)]
  pub account_set: Option<String>,
  #[arg(long)]
  pub account_remove: Option<String>,
  #[arg(long)]
  pub kind: Option<String>,
  /// An endpoint account's base URL.
  #[arg(long, allow_hyphen_values = true)]
  pub endpoint: Option<String>,
  /// An endpoint account's key file, or the secret an api-key account's key sits in under the data root's
  /// env/.
  #[arg(long, allow_hyphen_values = true)]
  pub key_file: Option<String>,
  /// An endpoint account's model ids by family word, as opus=<id>,sonnet=<id>,haiku=<id>.
  #[arg(long, allow_hyphen_values = true)]
  pub models: Option<String>,
  /// The base prompt a run on the account opens with, or none.
  #[arg(long, allow_hyphen_values = true)]
  pub base_prompt: Option<String>,
  #[arg(long)]
  pub name: Option<String>,
  #[arg(long)]
  pub route: Option<String>,
  #[arg(long)]
  pub account: Option<String>,
  /// The registration a route's key runs on at the registry's provider in place of its tier's cell, or
  /// `auto` to read the cell again.
  #[arg(long)]
  pub registration: Option<String>,
  #[arg(long)]
  pub provider: Option<String>,
  #[arg(long)]
  pub model: Option<String>,
  #[arg(long)]
  pub add: Option<String>,
  #[arg(long)]
  pub set: Option<String>,
  #[arg(long)]
  pub remove: Option<String>,
  #[arg(long, allow_hyphen_values = true)]
  pub label: Option<String>,
  #[arg(long, allow_hyphen_values = true)]
  pub path: Option<String>,
  #[arg(long, allow_hyphen_values = true)]
  pub preamble: Option<String>,
  #[arg(long)]
  pub preamble_file: Option<PathBuf>,
  #[arg(long, allow_hyphen_values = true)]
  pub dev_command: Option<String>,
  #[arg(long, allow_hyphen_values = true)]
  pub herdr_label: Option<String>,
  #[arg(long)]
  pub downgrade_heal: Option<String>,
  #[arg(long)]
  pub keepalive: Option<String>,
  /// A repository's support checkout count, `auto` or a number, with `--set` naming the repository.
  #[arg(long)]
  pub support: Option<String>,
  #[arg(long)]
  pub support_keepalive: Option<String>,
  #[arg(long)]
  pub node: Option<String>,
  #[arg(long)]
  pub rest: Option<String>,
  #[arg(long)]
  pub node_checkout: Option<String>,
  #[arg(long)]
  pub drop_node_checkout: Option<String>,
  #[arg(long)]
  pub reasoning: Option<String>,
  #[arg(long)]
  pub thinking: Option<String>,
  #[arg(long)]
  pub effort: Option<String>,
  #[arg(long)]
  pub gates: Option<String>,
  /// The gate, and on or off.
  #[arg(long, num_args = 1..=2, value_names = ["GATE", "SWITCH"])]
  pub gate: Option<Vec<String>>,
  #[arg(long)]
  pub pause_keepalive: Option<String>,
}

/// What the verb printed, and where it ended otherwise than well: the sentence on the error stream and
/// the code.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Said {
  pub out: Vec<String>,
  pub ended: Option<(u8, String)>,
}

impl Said {
  fn line(&mut self, line: impl Into<String>) {
    self.out.push(line.into());
  }

  fn refused(mut self, why: impl Into<String>) -> Self {
    self.ended = Some((1, why.into()));
    self
  }

  fn misread(mut self, usage: impl Into<String>) -> Self {
    self.ended = Some((2, usage.into()));
    self
  }
}

/// What the verb does beyond the archive, held apart so a test stands in for it: a worker's workbench and
/// dev server brought to its row, which workers a job holds now, and a key read from standing input.
pub trait AgentsHands: Send + Sync {
  fn settle<'a>(&'a self, worker: &'a str) -> BoxFuture<'a, ()>;
  /// The workers with something at work, or none where nobody could say.
  fn running(&self) -> BoxFuture<'_, Option<Vec<String>>>;
  fn standing_input(&self) -> String;
}

/// The hands of the verb run from a shell, on the ground its process stands on.
pub struct ShellHands {
  ground: crate::places::Ground,
}

impl AgentsHands for ShellHands {
  fn settle<'a>(&'a self, worker: &'a str) -> BoxFuture<'a, ()> {
    Box::pin(crate::places::dev::settle(&self.ground, worker))
  }

  fn running(&self) -> BoxFuture<'_, Option<Vec<String>>> {
    Box::pin(crate::control::sessions::running_workers(&self.ground))
  }

  fn standing_input(&self) -> String {
    std::io::read_to_string(std::io::stdin()).unwrap_or_default()
  }
}

/// Where the verb stands: agency's ground — its store, its node service, its events, the data root a
/// worker with no directory of its own is given one under and the source root the built-in worker works in
/// — the machine its providers are judged on, and its hands.
pub struct Stand<'a> {
  pub ground: &'a Ground,
  pub machine: &'a Machine,
  pub hands: &'a dyn AgentsHands,
  /// The home a path on a node is written from, `~` standing for it.
  pub home: &'a Path,
}

/// The verb run from a shell: what it prints on standard output first, then the refusal it ends with.
pub async fn run(data_root: &Path, args: AgentsArgs) -> Result<Said, VerbError> {
  let hands = ShellHands {
    ground: super::ground(data_root).await?,
  };
  let machine = Machine::of_process(data_root);
  let home = super::home();
  let stand = Stand {
    ground: &hands.ground,
    machine: &machine,
    hands: &hands,
    home: &home,
  };

  agents(&stand, &args).await
}

fn failed(refused: Refused) -> VerbError {
  VerbError::Refused(refused.error)
}

/// The verb itself: the first group of flags given, in the order below, decides what it does; with none, the
/// supervisors, the roles and the repositories are listed under the header.
pub async fn agents(stand: &Stand<'_>, args: &AgentsArgs) -> Result<Said, VerbError> {
  if args.provider_on.is_some() || args.provider_off.is_some() {
    return provider_switches(&stand.ground.db, args).await;
  }

  if args.registered
    || args.register.is_some()
    || args.register_address.is_some()
    || args.unregister.is_some()
  {
    return registry_change(stand, args).await;
  }

  if args.account_order.is_some()
    || args.account_on.is_some()
    || args.account_off.is_some()
    || args.account_reset
    || args.account_use.is_some()
  {
    return account_order(Judging::of(stand.ground), args).await;
  }

  if args.account_add.is_some() || args.account_set.is_some() || args.account_remove.is_some() {
    return account_change(&stand.ground.db, &stand.ground.roots.data, args).await;
  }

  if args.support.is_some() {
    return repository_support(&stand.ground.db, args).await;
  }

  let mut said = Said {
    out: header(Judging::of(stand.ground)).await.map_err(failed)?,
    ended: None,
  };

  said.out.extend(listing(&stand.ground.db).await?);

  Ok(said)
}

// One provider switched on or off (docs/provider-order.md, 「The owner's switch on a provider」), every
// provider's switch printed as it then stands.
async fn provider_switches(db: &DatabaseConnection, args: &AgentsArgs) -> Result<Said, VerbError> {
  let said = Said::default();
  let names = build_providers().join(", ");

  for (named, enabled) in [(&args.provider_on, true), (&args.provider_off, false)] {
    let Some(named) = named else { continue };

    if !ARCHIVE_PROVIDERS.contains(&named.as_str()) {
      return Ok(said.refused(fill_marks(&VERB_WORDS.agents.provider_unknown, &[("names", &names)])));
    }

    if let Err(refused) = set_provider_enabled(db, named, enabled).await {
      return Ok(said.refused(refused.error));
    }
  }

  let mut said = said;

  said.line(switches_line(db).await.map_err(failed)?);

  Ok(said)
}

// Every provider's switch, in the order this build carries them.
async fn switches_line(db: &DatabaseConnection) -> Result<String, Refused> {
  let mut switches = vec![];

  for name in build_providers() {
    let on = provider_enabled(db, name).await?;

    switches.push(format!("{name} ({})", if on { "on" } else { "off" }));
  }

  Ok(format!("providers: {}", switches.join(", ")))
}

// The providers this build carries, in the order the build declares them.
fn build_providers() -> Vec<&'static str> {
  harness_providers::providers().iter().map(|provider| provider.name()).collect()
}

// A switch given on the command line, read as on or off; any other word refused with the flag named.
fn on_off(value: Option<&str>, flag: &str) -> Result<Option<bool>, String> {
  match value {
    None => Ok(None),
    Some("on") => Ok(Some(true)),
    Some("off") => Ok(Some(false)),
    Some(_) => Err(fill_marks(&VERB_WORDS.agents.on_off, &[("flag", flag)])),
  }
}

/// A count given on the command line, its grouping commas let go; a word that reads as no number is
/// handed on as it stands, for the registry to refuse.
pub fn count(value: Option<&str>) -> Option<Value> {
  let value = value?;
  let digits = value.replace(',', "");

  Some(match digits.trim().parse::<f64>() {
    Ok(number) if number.fract() == 0.0 && number.abs() < 9e15 => json!(number as i64),
    Ok(number) if number.is_finite() => json!(number),
    _ => json!(value),
  })
}

// The registry's change, and the registry as it then stands.
async fn registry_change(stand: &Stand<'_>, args: &AgentsArgs) -> Result<Said, VerbError> {
  let mut said = Said::default();
  let body = if let Some(name) = &args.register {
    let mut body = json!({ "action": "save", "name": name });

    for (field, value) in [
      ("label", args.label.as_ref().map(|label| json!(label))),
      ("model", args.model.as_ref().map(|model| json!(model))),
      ("contextTokens", count(args.context.as_deref())),
      ("answerTokens", count(args.answer.as_deref())),
    ] {
      if let Some(value) = value {
        body[field] = value;
      }
    }

    if let Some(service) = &args.card_service {
      body["cardService"] = if service.trim() == "none" { Value::Null } else { json!(service) };
    }

    if let Some(languages) = &args.languages {
      body["languages"] = match languages.as_slice() {
        [none] if none.trim() == "none" => Value::Null,
        named => json!(named),
      };
    }

    for (field, value, flag) in [
      ("thinking", &args.thinking, "--thinking"),
      ("walked", &args.walked, "--walked"),
      ("keepsResearch", &args.research, "--research"),
      ("roleFiles", &args.role_files, "--role-files"),
    ] {
      match on_off(value.as_deref(), flag) {
        Ok(Some(on)) => body[field] = json!(on),
        Ok(None) => {}
        Err(why) => return Ok(said.refused(why)),
      }
    }

    Some(body)
  } else if let Some(address) = &args.register_address {
    let mut body =
      json!({ "action": "set-address", "name": address[0], "node": address[1], "url": address[2] });

    if args.key_stdin {
      body["key"] = json!(stand.hands.standing_input());
    } else if args.no_key {
      body["key"] = Value::Null;
    }

    match on_off(args.role_files.as_deref(), "--role-files") {
      Ok(Some(on)) => body["roleFiles"] = json!(on),
      Ok(None) => {}
      Err(why) => return Ok(said.refused(why)),
    }

    Some(body)
  } else {
    args
      .unregister
      .as_ref()
      .map(|named| match named.get(1).filter(|node| !node.starts_with("--")) {
        Some(node) => json!({ "action": "remove-address", "name": named[0], "node": node }),
        None => json!({ "action": "remove", "name": named[0] }),
      })
  };

  if let Some(body) = body
    && let Err(why) = registry::change(&stand.ground.db, &stand.ground.roots.data, &body)
      .await
      .map_err(failed)?
  {
    return Ok(said.refused(why));
  }

  let view = registry::view(&stand.ground.db, &stand.ground.roots.data).await.map_err(failed)?;

  for one in view["registrations"].as_array().into_iter().flatten() {
    let number = |field: &str| grouped(one[field].as_u64().unwrap_or_default() as usize);
    let flag = |field: &str| one[field].as_bool().unwrap_or_default();

    said.line(format!(
      "{} ({}): {}, context {}, answer {}, thinking {}, {}, {}",
      text(&one["name"]),
      text(&one["label"]),
      text(&one["model"]),
      number("contextTokens"),
      number("answerTokens"),
      if flag("thinking") { "on" } else { "off" },
      if flag("walked") { "walked" } else { "not walked" },
      if flag("keepsResearch") { "keeps its research" } else { "research elsewhere" },
    ));

    let languages: Vec<String> = one["languages"].as_array().into_iter().flatten().map(text).collect();

    if !languages.is_empty() {
      said.line(format!("  writes results in {} alone", crate::text::listed(&languages)));
    }

    if let Some(service) = one["cardService"].as_str() {
      let kept = if flag("roleFiles") { ", role files" } else { "" };

      said.line(format!("  card service {service}, standing where the arbiter grants it{kept}"));
    }

    for address in one["addresses"].as_array().into_iter().flatten() {
      let key = match address["key"].as_str() {
        Some("usable") => &VERB_WORDS.agents.key_usable,
        Some("none") => &VERB_WORDS.agents.key_none,
        _ => &VERB_WORDS.agents.key_unusable,
      };

      let kept = if address["roleFiles"].as_bool().unwrap_or_default() { ", role files" } else { "" };

      said.line(format!("  {} {} {key}{kept}", text(&address["node"]), text(&address["url"])));
    }

    let naming: Vec<String> = one["referrers"].as_array().into_iter().flatten().map(text).collect();

    if !naming.is_empty() {
      said.line(format!("  named by: {}", naming.join(", ")));
    }
  }

  let gateway = &view["gateway"];

  said.line(if gateway.is_null() {
    "gateway: no status written".to_owned()
  } else {
    let serving = if gateway["serving"].is_null() {
      "nothing".to_owned()
    } else {
      text(&gateway["serving"])
    };
    let starting = if gateway["starting"].is_null() {
      String::new()
    } else {
      format!(", starting {}", text(&gateway["starting"]))
    };
    let draining = gateway["draining"].as_f64().unwrap_or_default();
    let draining = if draining > 0.0 {
      format!(", {} draining", gateway["draining"])
    } else {
      String::new()
    };

    format!("gateway: serving {serving}{starting}{draining}")
  });

  Ok(said)
}

// A value as a template literal writes it: a string as it stands, anything else as its JSON.
fn text(value: &Value) -> String {
  match value {
    Value::String(text) => text.clone(),
    other => other.to_string(),
  }
}

/// One account as the order prints it.
#[derive(Serialize)]
struct OrderedAccount<'a> {
  id: &'a str,
  name: &'a str,
  enabled: bool,
}

// The account order put back, set, switched, or one chosen to run on; printed as it then stands with
// the account runs go out on now.
async fn account_order(judging: Judging<'_>, args: &AgentsArgs) -> Result<Said, VerbError> {
  let db = judging.db;
  let mut said = Said::default();
  let no_account = |named: &str| fill_marks(&VERB_WORDS.agents.no_account, &[("named", named)]);
  let changed: Result<Result<(), String>, Refused> = async {
    if args.account_reset {
      let order: Vec<(String, bool)> = all(db, "SELECT id FROM claude_accounts ORDER BY rowid", vec![])
        .await?
        .iter()
        .map(|row| row.try_get::<String>("", "id").map(|id| (id, true)))
        .collect::<Result<_, _>>()?;

      changes::set_order(db, &order).await?;
      changes::choose(db, None).await?;
    }

    if let Some(order) = &args.account_order {
      let mut ids = Vec::new();

      // Each part is whichever of the two the owner typed; the order itself is always by id.
      for named in order.split(',') {
        match account_named(db, named.trim()).await? {
          Some(account) => ids.push(json!(account.id)),
          None => return Ok(Err(no_account(named.trim()))),
        }
      }

      let held = accounts(db).await?;
      let Some(order) = changes::order_to_save(&json!(ids), &held) else {
        return Ok(Err(CONTROL_WORDS.accounts.order.clone()));
      };

      changes::set_order(db, &order).await?;
    }

    for (named, enabled) in [(&args.account_on, true), (&args.account_off, false)] {
      let Some(named) = named else { continue };
      let Some(account) = account_named(db, named).await? else {
        return Ok(Err(no_account(named)));
      };

      changes::update(db, &account.id, None, None, Some(enabled), BasePromptGiven::Keep).await?;
    }

    if let Some(asked) = &args.account_use {
      let id = if asked == "auto" {
        None
      } else {
        match account_named(db, asked).await? {
          Some(account) => Some(account.id),
          None => return Ok(Err(no_account(asked))),
        }
      };

      changes::choose(db, id.as_deref()).await?;
    }

    Ok(Ok(()))
  }
  .await;

  match changed {
    Ok(Err(why)) => return Ok(said.refused(why)),
    Err(refused) => return Ok(said.refused(refused.error)),
    Ok(Ok(())) => {}
  }

  let held = accounts(db).await.map_err(failed)?;
  let order: Vec<OrderedAccount> = held
    .iter()
    .map(|one| OrderedAccount {
      id: &one.id,
      name: &one.name,
      enabled: one.enabled,
    })
    .collect();

  said.line(serde_json::to_string(&order).unwrap_or_default());
  said.line(account_use_line(judging).await.map_err(failed)?);

  Ok(said)
}

// The line an account is printed in after a change: where its credential is reached, never the
// credential.
fn account_line(account: &claude_accounts::Account) -> String {
  format!(
    "account {} ({}) {} at {}",
    account.id,
    account.name,
    account.credential.kind().word(),
    account.credential.path().display()
  )
}

// Registering an account, renaming one, pointing it at another credential and removing one — the same
// things the console's own panel does, for a hand that has a shell and no browser.
async fn account_change(
  db: &DatabaseConnection,
  data_root: &Path,
  args: &AgentsArgs,
) -> Result<Said, VerbError> {
  let mut said = Said::default();
  let kind = match args.kind.as_deref() {
    None => None,
    Some(word) => match CredentialKind::named(word) {
      Some(kind) => Some(kind),
      None => {
        let words = fill_marks(&VERB_WORDS.agents.credential_kind, &[("kinds", &CredentialKind::words())]);

        return Ok(said.misread(words));
      }
    },
  };

  let no_account = |named: &str| fill_marks(&VERB_WORDS.agents.no_account, &[("named", named)]);
  let by_id = async |id: &str| -> Result<claude_accounts::Account, VerbError> {
    accounts(db)
      .await
      .map_err(failed)?
      .into_iter()
      .find(|one| one.id == id)
      .ok_or_else(|| VerbError::Refused(no_account(id)))
  };
  let models = match args.models.as_deref() {
    None => None,
    Some(words) => match EndpointModels::of_words(words) {
      Some(models) => Some(models),
      None => return Ok(said.misread(VERB_WORDS.agents.models_shape.clone())),
    },
  };
  let base = match args.base_prompt.as_deref() {
    None => BasePromptGiven::Keep,
    Some("none") => BasePromptGiven::None,
    Some(name) => BasePromptGiven::Named(name.to_owned()),
  };
  // A credential is given where any of its parts is. Each part not given keeps the one the account
  // holds, and an account added starts from the kind it names or a directory.
  let credential = || CredentialGiven {
    kind,
    path: args.path.clone(),
    endpoint: args.endpoint.clone(),
    key_file: args.key_file.clone(),
    models: models.clone(),
  };
  let credential_given = !credential().is_empty();

  if let Some(name) = &args.account_add {
    if args.path.is_none() {
      return Ok(said.misread(VERB_WORDS.agents.account_add.clone()));
    }

    // An api-key account's key is named as the platform's secret, resolved under the data root's env/.
    let given = match credential().with_secret_under(None, data_root, None) {
      Ok(given) => given,
      Err(why) => return Ok(said.misread(why)),
    };

    if let Err(refused) = changes::add(db, name, &given, base.clone()).await {
      return Ok(said.refused(refused.error));
    }

    let held = accounts(db).await.map_err(failed)?;
    let added = held.iter().rev().find(|one| one.name == name.trim());

    if let Some(added) = added {
      said.line(account_line(added));
    }
  }

  if let Some(named) = &args.account_set {
    let Some(held) = account_named(db, named).await.map_err(failed)? else {
      return Ok(said.refused(no_account(named)));
    };

    if args.name.is_none() && !credential_given && base == BasePromptGiven::Keep {
      return Ok(said.misread(VERB_WORDS.agents.account_set.clone()));
    }

    // A credential given in part keeps the parts the account already holds; the id never moves. An api-key
    // account's secret is resolved in its owner's space under the data root's env/.
    let given = match credential_given
      .then(|| credential().with_secret_under(Some(held.credential.kind()), data_root, held.user_id))
      .transpose()
    {
      Ok(given) => given,
      Err(why) => return Ok(said.misread(why)),
    };

    if let Err(refused) =
      changes::update(db, &held.id, args.name.as_deref(), given.as_ref(), None, base).await
    {
      return Ok(said.refused(refused.error));
    }

    said.line(account_line(&by_id(&held.id).await?));
  }

  if let Some(named) = &args.account_remove {
    let Some(held) = account_named(db, named).await.map_err(failed)? else {
      return Ok(said.refused(no_account(named)));
    };

    if let Err(refused) = changes::remove(db, &held.id).await {
      return Ok(said.refused(refused.error));
    }

    said.line(format!("account {} ({}) removed", held.id, held.name));
  }

  Ok(said)
}

/// A support count as the verb takes it: `auto`, or a whole number from zero to the most a repository may have.
fn support_count(value: Option<&str>) -> Option<SupportCount> {
  let value = value?.trim();

  if value == crate::repositories::AUTO {
    return Some(SupportCount::AUTO);
  }

  let count = value.parse::<i64>().ok()?;

  (0..=WORKERS.support_most).contains(&count).then_some(SupportCount::Count(count))
}

// A repository's support checkout count (docs/worker-slots.md, 「The base checkout and the support
// checkouts」): the one setting a name and a count change, and no other field of the row.
async fn repository_support(db: &DatabaseConnection, args: &AgentsArgs) -> Result<Said, VerbError> {
  let mut said = Said::default();
  let Some(name) = args.set.as_deref() else {
    return Ok(said.misread(VERB_WORDS.agents.support_usage.clone()));
  };
  let Some(count) = support_count(args.support.as_deref()) else {
    let most = WORKERS.support_most.to_string();

    return Ok(said.refused(fill_marks(&VERB_WORDS.agents.support_count, &[("most", &most)])));
  };

  if crate::repositories::named(db, name).await?.is_none() {
    return Ok(said.refused(fill_marks(&VERB_WORDS.agents.no_repository, &[("name", name)])));
  }

  store::run(
    db,
    r#"UPDATE "repositories" SET "support_count" = $2, "updated_at" = $3 WHERE "name" = $1"#,
    vec![name.to_owned().into(), count.stored().into(), now_ms().into()],
  )
  .await?;

  said.line(match count.stored() {
    Some(count) => {
      format!("repository {name}: {count} support checkout{}", if count == 1 { "" } else { "s" })
    }
    None => format!("repository {name}: support checkouts grown as its launches need them"),
  });

  Ok(said)
}

// The four lines above everything but a change: the providers' switches, the registered models, the account
// order, and the account runs go out on now.
async fn header(judging: Judging<'_>) -> Result<Vec<String>, Refused> {
  let db = judging.db;
  let registered: Vec<String> = registrations(db)
    .await?
    .iter()
    .map(|one| {
      let addresses = match (&one.card_service, one.addresses.len()) {
        (Some(service), _) => format!("card service {service}"),
        (None, 1) => "1 address".to_owned(),
        (None, many) => format!("{many} addresses"),
      };

      format!("{} ({}, {addresses})", one.name, grouped(one.context_tokens.max(0) as usize))
    })
    .collect();
  let held = accounts(db).await?;
  let as_added: Vec<String> = all(db, "SELECT id FROM claude_accounts ORDER BY rowid", vec![])
    .await?
    .iter()
    .map(|row| row.try_get::<String>("", "id"))
    .collect::<Result<_, _>>()?;
  let default = held.iter().zip(&as_added).all(|(one, id)| one.enabled && &one.id == id);
  let order: Vec<String> = held
    .iter()
    .map(|one| format!("{} ({}, {})", one.id, one.name, if one.enabled { "on" } else { "off" }))
    .collect();

  Ok(vec![
    switches_line(db).await?,
    format!(
      "registered models: {}",
      if registered.is_empty() { "none".to_owned() } else { registered.join(", ") }
    ),
    format!("account order: {}{}", order.join(" → "), if default { " [default]" } else { "" }),
    account_use_line(judging).await?,
  ])
}

// Which account runs go out on now, and the one chosen to run on where there is one. Where the two
// differ the chosen one is resting or its login is gone, or the login in use is still being spent to its
// end, and the line says which.
async fn account_use_line(judging: Judging<'_>) -> Result<String, Refused> {
  let views = claude_accounts::views(judging).await?;
  let flag = |view: &&Value, field: &str| view[field].as_bool().unwrap_or_default();
  let serving = views.iter().find(|view| flag(view, "serving"));
  let chosen = views.iter().find(|view| flag(view, "chosen"));
  let using = match serving {
    None => "account in use: none".to_owned(),
    Some(serving) => format!("account in use: {} ({})", text(&serving["id"]), text(&serving["name"])),
  };
  let Some(chosen) = chosen else {
    return Ok(format!("{using}; chosen: none, walks the account order"));
  };

  if flag(&chosen, "serving") {
    return Ok(format!("{using}; chosen: {}", text(&chosen["id"])));
  }

  let why = if let Some(until) = chosen["restUntil"].as_i64() {
    let at = chrono::DateTime::from_timestamp(until, 0)
      .map(|at| at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
      .unwrap_or_default();

    format!("resting at its limit until {at}")
  } else if chosen["login"] == "gone" {
    "its login is gone".to_owned()
  } else {
    match serving {
      None => "not answering".to_owned(),
      Some(serving) => format!("next once {} is spent", text(&serving["id"])),
    }
  };

  Ok(format!("{using}; chosen: {} ({}), {why}", text(&chosen["id"]), text(&chosen["name"])))
}

/// by the node's, under the same parent, written from the home where the base's path is under it.
pub fn node_checkout_path(base_cwd: &str, node: &str, home: &Path) -> String {
  let base = crate::queue::paths::resolve(base_cwd);
  let folder = base.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
  let path = base.parent().unwrap_or(Path::new("/")).join(format!("{folder}-{node}"));
  let path = path.to_string_lossy().into_owned();
  let home = home.to_string_lossy();

  match path.strip_prefix(&format!("{home}/")) {
    Some(rest) => format!("~/{rest}"),
    None => path,
  }
}

/// word that reads as no number is no whole count.
pub fn number(value: &str) -> f64 {
  let trimmed = value.trim();

  if trimmed.is_empty() { 0.0 } else { trimmed.parse().unwrap_or(f64::NAN) }
}

/// How long ago a moment was, in minutes under the hour and hours above it.
pub fn ago(at: Option<i64>, now: i64) -> String {
  let Some(at) = at else { return "never".to_owned() };
  // Rounded a half up.
  let round = |value: f64| (value + 0.5).floor() as i64;
  let minutes = round((now - at) as f64 / 60.0);

  if minutes < 60 {
    format!("{minutes}m ago")
  } else {
    format!("{}h ago", round(minutes as f64 / 60.0))
  }
}

/// Every agent the bot runs, one line each, and the resting workers last, apart, since none of them is a
// The supervisors, the roles and the repositories, one line each.
async fn listing(db: &DatabaseConnection) -> Result<Vec<String>, VerbError> {
  let mut lines = vec![];
  let entries = |order: &crate::order::Order| {
    order
      .entries
      .iter()
      .map(|entry| format!("{}·{}", entry.provider, entry.model))
      .collect::<Vec<_>>()
      .join(" → ")
  };

  for supervisor in crate::supervisors::every(db).await.map_err(|why| VerbError::Refused(why.detail))? {
    lines.push(format!("supervisor {}: {}", supervisor.name, entries(&supervisor.order())));
  }

  for role in crate::agent_roles::every(db).await.map_err(|why| VerbError::Refused(why.detail))? {
    lines.push(format!("role {} (cap {}): {}", role.name, role.max_concurrent, entries(&role.order)));
  }

  for repository in crate::repositories::every(db).await.map_err(|why| VerbError::Refused(why.detail))? {
    let pin = repository
      .pin
      .map(|pin| format!(", pinned {}·{}", pin.provider, pin.model))
      .unwrap_or_default();

    lines.push(format!("repository {}: {}{pin}", repository.name, repository.path));
  }

  Ok(lines)
}
