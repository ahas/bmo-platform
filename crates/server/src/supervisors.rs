//! The supervisors as the store keeps them (docs/supervisors.md, 「What a supervisor row holds」): every field
//! of a supervisor's row is read off it at every use.

use std::collections::BTreeMap;

use sea_orm::{ConnectionTrait, QueryResult};
use store::{Fault, all, one};

use crate::order::{Entry, Order};
use crate::rooms::Stays;

crate::room_stays!(Stays {
  table: "supervisors",
  stays: "TRUE",
  why: "a supervisor is its account's, never a room's: its row names only the room its session last ran in",
});

/// The mark a supervisor of an account leads its system prompt with (docs/agent-ownership.md, 「What the
/// supervisor's row carries」): a short digest of the account, which says nothing of it.
pub fn account_mark(account: i64) -> String {
  use sha2::{Digest, Sha256};

  let digest = Sha256::digest(format!("account:{account}").as_bytes());
  let short: String = digest.iter().take(6).map(|byte| format!("{byte:02x}")).collect();

  format!("[tenant {short}]")
}

/// A system prompt led by its supervisor's account mark, where the supervisor belongs to an account.
pub async fn account_led<C: ConnectionTrait>(db: &C, agent: &str, system: String) -> String {
  match named(db, agent).await {
    Ok(Some(supervisor)) => match supervisor.owner_account {
      Some(account) => format!("{}\n{system}", account_mark(account)),
      None => system,
    },
    _ => system,
  }
}

/// The variable a supervisor of an account's program holds that account in, the user's id as a decimal number,
/// which what it writes for that person names them by (docs/agent-ownership.md, 「What the supervisor's row
/// carries」). A supervisor of no account's program holds none.
pub const ACCOUNT_VARIABLE: &str = "BMO_OWNER_ACCOUNT";

/// The variables a supervisor's program holds of its own row: its owner account, where it belongs to one.
pub async fn own_variables<C: ConnectionTrait>(db: &C, agent: &str) -> Vec<(String, String)> {
  match named(db, agent).await {
    Ok(Some(Supervisor {
      owner_account: Some(account),
      ..
    })) => vec![(ACCOUNT_VARIABLE.to_owned(), account.to_string())],
    _ => Vec::new(),
  }
}

/// One supervisor as its row holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Supervisor {
  pub name: String,
  pub label: Option<String>,
  pub owner_account: Option<i64>,
  /// The organisation unit it is filed under, none where it is no unit's.
  pub owner_unit: Option<i64>,
  /// Its call names as the row keeps them, one line joined by commas (docs/supervisors.md, 「Its call names」).
  pub aliases: Option<String>,
  pub icon: Option<String>,
  pub prompts: Option<String>,
  pub brain_folder: Option<String>,
  pub voice: Option<String>,
  pub session_scope: String,
  pub turn_shape: String,
  /// The repository its TODO items go to.
  pub repository: Option<String>,
  /// The one road its turns go out on, where its row names one (docs/provider-order.md, 「What a road is」).
  pub road: Option<Entry>,
  /// For each provider, whether its prefix is kept warm while idle there.
  pub warming: BTreeMap<String, bool>,
  pub fold_ratio: f64,
  /// The rank of the demands of the runs for it, where it holds one.
  pub card_rank: Option<u32>,
  pub builtin: bool,
  /// What its last turn went out on.
  pub last_mode: Option<String>,
  pub last_model: Option<String>,
  pub last_effort: Option<String>,
  pub session_id: Option<String>,
}

impl Supervisor {
  pub fn of(row: &QueryResult) -> Result<Self, Fault> {
    let warming: String = row.try_get("", "warming")?;
    let provider: Option<String> = row.try_get("", "provider")?;
    let model: Option<String> = row.try_get("", "model")?;
    let road = provider
      .filter(|provider| !provider.is_empty())
      .zip(model)
      .map(|(provider, model)| Entry {
        provider,
        model,
        effort: row
          .try_get::<Option<String>>("", "effort")
          .ok()
          .flatten()
          .filter(|effort| !effort.is_empty()),
        thinking: row
          .try_get::<Option<String>>("", "thinking")
          .ok()
          .flatten()
          .filter(|thinking| !thinking.is_empty()),
        answer_tokens: row.try_get("", "answer_tokens").ok().flatten(),
        advisor: row
          .try_get::<Option<String>>("", "advisor")
          .ok()
          .flatten()
          .filter(|advisor| !advisor.is_empty()),
      });

    Ok(Self {
      name: row.try_get("", "name")?,
      label: row.try_get("", "label")?,
      owner_account: row.try_get("", "owner_account")?,
      owner_unit: row.try_get("", "owner_unit")?,
      aliases: row.try_get("", "aliases")?,
      icon: row.try_get("", "icon")?,
      prompts: row.try_get("", "prompts")?,
      brain_folder: row.try_get("", "brain_folder")?,
      voice: row.try_get("", "voice")?,
      session_scope: row.try_get("", "session_scope")?,
      turn_shape: row.try_get("", "turn_shape")?,
      repository: row.try_get("", "repository")?,
      road,
      warming: serde_json::from_str(&warming).unwrap_or_default(),
      fold_ratio: row.try_get("", "fold_ratio")?,
      card_rank: row.try_get::<Option<i32>>("", "card_rank")?.and_then(|rank| u32::try_from(rank).ok()),
      builtin: row.try_get::<i64>("", "builtin")? != 0,
      last_mode: row.try_get("", "last_mode")?,
      last_model: row.try_get("", "last_model")?,
      last_effort: row.try_get("", "last_effort")?,
      session_id: row.try_get("", "session_id")?,
    })
  }

  /// The words beside its label that call it in a room, in the order they were written: the line its row keeps
  /// them in, split at its commas, since no call name holds one.
  pub fn call_names(&self) -> Vec<String> {
    self
      .aliases
      .iter()
      .flat_map(|line| line.split(','))
      .map(str::trim)
      .filter(|name| !name.is_empty())
      .map(str::to_owned)
      .collect()
  }

  /// Whether its prefix is kept warm while it stands idle on this provider: the row's switch for it, off
  /// where the row names none.
  pub fn warms_on(&self, provider: &str) -> bool {
    self.warming.get(provider).copied().unwrap_or(false)
  }

  /// The road it stands on, which its turns and a run that follows it go out on; none where its row names no
  /// road (docs/provider-order.md, 「Runs on another's road」).
  pub fn road(&self) -> Option<Entry> {
    self.road.clone()
  }

  /// Its road as the road judgement takes it: the one road, nothing forced over it and nothing after it (docs/provider-order.md, 「Whether a road can answer」).
  pub fn order(&self) -> Order {
    Order {
      entries: self.road.iter().cloned().collect(),
      forced: None,
    }
  }
}

/// The line a row keeps these call names in, joined by a comma and a space, or none for no name.
pub fn call_names_line(names: &[String]) -> Option<String> {
  (!names.is_empty()).then(|| names.join(", "))
}

/// Every supervisor the store holds, by name.
pub async fn every<C: ConnectionTrait>(db: &C) -> Result<Vec<Supervisor>, Fault> {
  all(db, r#"SELECT * FROM "supervisors" ORDER BY "name""#, vec![])
    .await?
    .iter()
    .map(Supervisor::of)
    .collect()
}

/// The supervisor of this name, where the store holds one.
pub async fn named<C: ConnectionTrait>(db: &C, name: &str) -> Result<Option<Supervisor>, Fault> {
  one(db, r#"SELECT * FROM "supervisors" WHERE "name" = $1"#, vec![name.into()])
    .await?
    .as_ref()
    .map(Supervisor::of)
    .transpose()
}
