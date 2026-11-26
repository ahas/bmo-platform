//! The gateway's table, written from the registry (docs/own-llm.md, 「The gateway」): what LiteLLM is
//! started on. Every registration is served under its name with one entry for each of its addresses,
//! and a registration of several addresses once more under `<name>@<node>` for each, one address
//! alone, so a session placed on a node asks that node only. A registration naming a card service is served
//! the same way over the instances the arbiter's master reports standing, each under `<name>@<node>` even
//! where one stands alone, since a run names the entry of the card its demand was granted. The table names
//! each key by a variable and never a key; the launcher sets the variables from the key files in its
//! child's environment. A person's own key for a road is laid as a personal member beside each member of
//! that road (docs/own-llm.md, 「Personal members」), and every name a person's member or registration is
//! served under is known with its owner, whom alone it answers beside the gateway's own key (「The gateway's
//! members」).

use std::collections::{BTreeMap, BTreeSet};

use harness_machine::card_master::standing_in;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use store::registry::{RegisteredAddress, Registration};

use crate::supply::{Payer, PersonalKey};

pub use harness_machine::members::Member;

/// The hooks LiteLLM loads, each from a file of its name beside the table: the one that carries a
/// Messages request's thinking to the chat template, the one that writes a run's sampling settings
/// from its header into the request, the one that folds a system turn inside the conversation into
/// the person's turn beside it, the one that copies a run's long tools from their header into the
/// field the engines read, the one that starts a request from its role's saved prefix on an instance
/// that keeps role files, the one that takes the wait off every request, and the one that hands the front
/// what each request cost.
pub const HOOKS: [&str; 7] =
  ["thinking", "sampling", "reminders", "long_tools", "roles", "unclocked", "costs"];

/// The gateway's own key, as the variable the table names it by.
pub const GATEWAY_KEY_VARIABLE: &str = "LLM_GATEWAY_KEY";

/// The table as LiteLLM reads it, with the key files its variables are read from.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
  /// What LiteLLM reads: JSON, which its YAML reader takes as it stands.
  pub text: String,
  /// Each variable the table names, with the key file under the data root's `env/` it is read from.
  pub key_files: BTreeMap<String, String>,
  /// Every model name the table holds an entry under.
  pub names: BTreeSet<String>,
}

/// Whom a model name answers where it is not the platform's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Owner {
  /// A personal member, laid on the account's own key: that account's call key alone.
  Personal(i64),
  /// A member or a registration the account owns: its call key, and the gateway's own key where the account
  /// is the administrator, whose supply the deployment's own runs go out on.
  Owned(i64),
  /// A member laid on an organisation unit's own key: a call key whose account holds an open reservation on
  /// that unit's supply alone, which the front judges itself (docs/own-llm.md, 「Personal members」).
  Organisation(i64),
}

impl Owner {
  /// Whether a request its payer sends reaches the name, the platform administered by `administrator`.
  pub fn admits(self, payer: Payer, administrator: Option<i64>) -> bool {
    match (self, payer) {
      (Owner::Organisation(_), _) => false,
      (Owner::Personal(account) | Owner::Owned(account), Payer::Account(paying)) => paying == account,
      (Owner::Personal(_), Payer::Platform) => false,
      (Owner::Owned(account), Payer::Platform) => administrator == Some(account),
    }
  }
}

/// A member laid for one person, or for one organisation unit, on their own key (docs/own-llm.md, 「Personal
/// members」).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Personal {
  pub member: Member,
  /// Whose key it goes out on: `Owner::Personal` with the person, the only account whose call key reaches it,
  /// or `Owner::Organisation` with the unit.
  pub owner: Owner,
}

// Whether a secret's name names one file of the person's folder and nothing outside it.
fn plain_name(secret: &str) -> bool {
  !secret.is_empty() && secret != "." && secret != ".." && !secret.contains(['/', '\0'])
}

/// The personal members the rows of personal supply lay: for each person and road, the open row with the
/// lowest place, and beside each member of that road one more, `<member>@u<account>`, on the person's
/// secret `users/<account>/<secret>` under the data root's `env/`, of no road of its own, so no road's model
/// list names it; and for each organisation unit's rows the same, `<member>@o<unit>` on the unit's secret
/// `units/<unit>/<secret>`. A row turned off, found refused or naming a secret outside its owner's folder lays
/// none.
pub fn personal_members(members: &[Member], keys: &[PersonalKey]) -> Vec<Personal> {
  let mut first: BTreeMap<(Owner, &str), &PersonalKey> = BTreeMap::new();

  for key in keys.iter().filter(|key| key.open() && plain_name(&key.secret)) {
    let Some(owner) = key.owner() else { continue };
    let held = first.entry((owner, key.road.as_str())).or_insert(key);
    if (key.place, key.id) < (held.place, held.id) {
      *held = key;
    }
  }

  first
    .into_iter()
    .flat_map(|((owner, road), key)| {
      let (mark, folder, account) = match owner {
        Owner::Organisation(unit) => (format!("o{unit}"), format!("units/{unit}"), None),
        Owner::Personal(account) | Owner::Owned(account) => {
          (format!("u{account}"), format!("users/{account}"), Some(account))
        }
      };

      members
        .iter()
        .filter(move |member| member.road.as_deref() == Some(road))
        .map(move |member| Personal {
          member: Member {
            name: format!("{}@{mark}", member.name),
            road: None,
            key_file: format!("{folder}/{}", key.secret),
            account,
            ..member.clone()
          },
          owner,
        })
    })
    .collect()
}

/// Each name a person's own member or registration is served under, with whom it answers: a registration
/// and each `<name>@<node>` of it, and a member, by the account each names; a personal member by its person.
pub fn owners(
  registrations: &[Registration],
  members: &[Member],
  personal: &[Personal],
) -> BTreeMap<String, Owner> {
  let mut owners: BTreeMap<String, Owner> = BTreeMap::new();

  for registration in registrations {
    let owner = match (registration.owner_account, registration.owner_unit) {
      (Some(account), _) => Some(Owner::Owned(account)),
      (None, Some(unit)) => Some(Owner::Organisation(unit)),
      (None, None) => None,
    };

    if let Some(owner) = owner {
      owners.insert(registration.name.clone(), owner);
      for address in &registration.addresses {
        owners.insert(format!("{}@{}", registration.name, address.node), owner);
      }
    }
  }
  for member in members {
    match (member.account, member.unit) {
      (Some(account), _) => {
        owners.insert(member.name.clone(), Owner::Owned(account));
      }
      (None, Some(unit)) => {
        owners.insert(member.name.clone(), Owner::Organisation(unit));
      }
      (None, None) => {}
    }
  }
  for one in personal {
    owners.insert(one.member.name.clone(), one.owner);
  }

  owners
}

/// The variable one key file is handed under.
pub fn key_variable(key_file: &str) -> String {
  let named: String = key_file
    .chars()
    .map(|one| if one.is_ascii_alphanumeric() { one.to_ascii_uppercase() } else { '_' })
    .collect();
  format!("BMO_KEY_{named}")
}

/// The chat completions address without its path's last two parts, which the OpenAI client adds; an address
/// reached through its owner's PC asked at its port of this machine's loopback in place of its own host and port
/// (docs/own-llm.md, 「A person's local AI」).
fn api_base(address: &str, reach_port: Option<i64>) -> String {
  let Ok(mut parsed) = url::Url::parse(address) else {
    return address.trim_end_matches('/').to_owned();
  };
  if let Some(port) = reach_port.and_then(|port| u16::try_from(port).ok()) {
    let _ = parsed.set_scheme("http");
    let _ = parsed.set_host(Some("127.0.0.1"));
    let _ = parsed.set_port(Some(port));
  }
  let path = parsed.path().strip_suffix("/chat/completions").unwrap_or(parsed.path()).to_owned();
  parsed.set_path(&path);
  parsed.as_str().trim_end_matches('/').to_owned()
}

#[derive(Serialize)]
struct Config {
  model_list: Vec<Entry>,
  litellm_settings: Settings,
  general_settings: General,
}

#[derive(Serialize)]
struct Entry {
  model_name: String,
  litellm_params: Params,
  /// What the hooks read of the address behind the entry, which LiteLLM hands no backend; none where the
  /// address says nothing.
  #[serde(skip_serializing_if = "Option::is_none")]
  model_info: Option<Info>,
}

#[derive(Clone, Serialize)]
struct Info {
  /// The instance behind the address keeps role files (docs/own-llm.md, 「Role files」).
  role_files: bool,
}

#[derive(Clone, Serialize)]
struct Params {
  model: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  api_base: Option<String>,
  /// An address that takes no key is still handed one, since the OpenAI client will not go without;
  /// the backend reads none.
  api_key: String,
  /// Thinking off where a request names nothing: the template's own default thinks for minutes on a
  /// slow card, even on a turn that asked for none.
  chat_template_kwargs: TemplateSettings,
}

#[derive(Clone, Serialize)]
struct TemplateSettings {
  enable_thinking: bool,
}

#[derive(Serialize)]
struct Settings {
  /// A parameter a backend does not take is dropped rather than refused.
  drop_params: bool,
  /// An `openai/` backend is asked at its chat completions path; left to itself the gateway asks the
  /// Responses API, which the backends do not serve.
  use_chat_completions_url_for_anthropic_messages: bool,
  callbacks: Vec<String>,
  /// A failed request is answered once and not retried here: the run that asked decides. No entry holds a
  /// wait, and the gateway-wide `request_timeout` is not read on the Messages path, so none is set: a hook
  /// takes the wait off every request (docs/own-llm.md, 「The gateway」).
  num_retries: u32,
}

#[derive(Serialize)]
struct General {
  master_key: String,
}

/// The registry with each registration naming a card service served by the instances the master's picture
/// reports standing, an instance's role files the registration's own. With no picture — no master named —
/// or a picture placing no such service, it is served nowhere.
pub fn laid_instances(mut registrations: Vec<Registration>, picture: Option<&Value>) -> Vec<Registration> {
  for registration in &mut registrations {
    let Some(service) = registration.card_service.clone() else {
      continue;
    };
    let instances = picture.and_then(|picture| standing_in(picture, &service)).map(|one| one.instances);

    registration.addresses = instances
      .into_iter()
      .flatten()
      .filter_map(|instance| {
        Some(RegisteredAddress {
          node: instance.node_name,
          url: instance.address?,
          key_file: instance.key_file,
          key_owned: false,
          placed_at: None,
          role_files: registration.role_files,
          reach_port: None,
        })
      })
      .collect();
  }

  registrations
}

/// The table of the registry alone: no member of a deployment's own.
pub fn gateway_table(registrations: &[Registration]) -> Table {
  gateway_table_with_members(registrations, &[]).expect("registrations hold unique names")
}

/// The table of the registry and the members beside it. A name a registration or another member already
/// holds refuses the whole table, so nothing answers under a name that would stand for another model.
pub fn gateway_table_with_members(
  registrations: &[Registration],
  members: &[Member],
) -> Result<Table, String> {
  gateway_table_with_personal(registrations, members, &[])
}

/// The table of the registry, the members beside it and the personal members after them, each held to the
/// same unique names.
pub fn gateway_table_with_personal(
  registrations: &[Registration],
  members: &[Member],
  personal: &[Personal],
) -> Result<Table, String> {
  let members: Vec<&Member> = members.iter().chain(personal.iter().map(|one| &one.member)).collect();
  let mut key_files = BTreeMap::new();
  let mut models = Vec::new();
  let mut names: BTreeSet<String> = BTreeSet::new();

  for registration in registrations {
    names.insert(registration.name.clone());
    for address in &registration.addresses {
      names.insert(format!("{}@{}", registration.name, address.node));
    }
  }

  for registration in registrations {
    for address in &registration.addresses {
      let variable = address.key_file.as_deref().map(key_variable);
      if let (Some(variable), Some(key_file)) = (&variable, &address.key_file) {
        key_files.insert(variable.clone(), key_file.clone());
      }
      let params = Params {
        model: format!("openai/{}", registration.model),
        api_base: Some(api_base(&address.url, address.reach_port)),
        api_key: variable.map_or_else(|| "no-key".to_owned(), |variable| format!("os.environ/{variable}")),
        chat_template_kwargs: TemplateSettings {
          enable_thinking: false,
        },
      };
      let info = address.role_files.then_some(Info { role_files: true });
      if registration.addresses.len() > 1 || registration.card_service.is_some() {
        models.push(Entry {
          model_name: registration.name.clone(),
          litellm_params: params.clone(),
          model_info: info.clone(),
        });
        models.push(Entry {
          model_name: format!("{}@{}", registration.name, address.node),
          litellm_params: params,
          model_info: info,
        });
      } else {
        models.push(Entry {
          model_name: registration.name.clone(),
          litellm_params: params,
          model_info: info,
        });
      }
    }
  }

  for member in members {
    if !names.insert(member.name.clone()) {
      return Err(format!("the model name {} is held by a registration or another member", member.name));
    }

    let variable = key_variable(&member.key_file);
    key_files.insert(variable.clone(), member.key_file.clone());
    let provider = match member.provider.as_str() {
      "anthropic" | "deepseek" | "openrouter" | "openai" => member.provider.as_str(),
      other => {
        return Err(format!(
          "the member {} names the provider {other}, which is no member provider",
          member.name
        ));
      }
    };
    models.push(Entry {
      model_name: member.name.clone(),
      litellm_params: Params {
        model: format!("{provider}/{}", member.model),
        api_base: member.api_base.clone(),
        api_key: format!("os.environ/{variable}"),
        chat_template_kwargs: TemplateSettings {
          enable_thinking: false,
        },
      },
      model_info: None,
    });
  }

  let config = Config {
    model_list: models,
    litellm_settings: Settings {
      drop_params: true,
      use_chat_completions_url_for_anthropic_messages: true,
      callbacks: HOOKS.iter().map(|hook| format!("{hook}.handler")).collect(),
      num_retries: 0,
    },
    general_settings: General {
      master_key: format!("os.environ/{GATEWAY_KEY_VARIABLE}"),
    },
  };
  let text = serde_json::to_string_pretty(&config).expect("the table is plain data");
  let names = config.model_list.iter().map(|entry| entry.model_name.clone()).collect();

  Ok(Table {
    text: format!("{text}\n"),
    key_files,
    names,
  })
}

fn hex(bytes: &[u8]) -> String {
  format!("{:x}", Sha256::digest(bytes))
}

/// What decides whether a running child serves the table as it stands: the table itself, what each key
/// file it names holds, and the hooks it loads, so a key given again or a hook changed reaches the
/// gateway as a new table would. The keys are digested and never kept.
pub fn table_digest(
  table: &Table,
  read_key: impl Fn(&str) -> Option<String>,
  hooks: &[(String, Vec<u8>)],
) -> String {
  let mut hash = Sha256::new();
  hash.update(table.text.as_bytes());
  for (variable, key_file) in &table.key_files {
    hash.update(format!("\n{variable}=").as_bytes());
    hash.update(hex(read_key(key_file).unwrap_or_default().as_bytes()).as_bytes());
  }
  for (name, text) in hooks {
    hash.update(format!("\nhook {name}=").as_bytes());
    hash.update(hex(text).as_bytes());
  }
  format!("{:x}", hash.finalize())[..16].to_owned()
}
