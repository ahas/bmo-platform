//! The seeds agency's server settles its store with (docs/store.md, 「Its seeds」): the values agency's
//! policy declares — the account, the catalogue's entries, the base prompts and the roles — with the
//! account's credential the machine's own Claude Code login directory, each base prompt's text its seed file
//! in the code tree, and each role's prompt and provider prompts their files there and its description the
//! policy's.

use std::path::Path;

use harness_providers::Machine;
use harness_providers::claude_code::ClaudeCode;
use policy::base_prompts::BASE_PROMPTS;
use policy::roles::{ROLES, Role};
use policy::seeds::SEEDS;
use store::seed::{AccountSeed, BasePromptSeed, CatalogueSeed, RoleSeed, Seeds};

use crate::control::claude_accounts::Credential;
use crate::order::{Entry, Order};
use crate::places::machine_code_root;

/// The seeds the deployment's store is settled with on this machine.
pub fn deployment(machine: &Machine) -> Seeds {
  // The seeded account's login is the machine's own Claude Code login directory.
  let login = Credential::Directory {
    home: ClaudeCode::machine_config_dir(machine),
  };

  Seeds {
    account: Some(AccountSeed {
      id: SEEDS.account.id.clone(),
      name: SEEDS.account.name.clone(),
      credential_kind: login.kind().word().to_owned(),
      credential_path: login.path().to_string_lossy().into_owned(),
    }),
    catalogue: SEEDS
      .catalogue
      .iter()
      .map(|entry| CatalogueSeed {
        name: entry.name.clone(),
        label: entry.label.clone(),
        file: entry.file.clone(),
        bytes: entry.bytes,
        sha256: entry.sha256.clone(),
        source: entry.source.clone(),
        engine: entry.engine.clone(),
        registration: entry.registration.clone(),
        thinking: entry.thinking,
        temperature: entry.temperature,
        top_p: entry.top_p,
        presence_penalty: entry.presence_penalty,
        max_tokens: entry.max_tokens,
        note: entry.note.clone(),
      })
      .collect(),
    base_prompts: base_prompt_seeds(&machine_code_root(machine)),
    roles: role_seeds(&machine_code_root(machine)),
  }
}

/// The roles the policy declares, each with the text of the prompt file and the provider prompt files it
/// names under the code tree. One whose file cannot be read is said and left out, so it is given by the first
/// settling that can read it.
pub fn role_seeds(code: &Path) -> Vec<RoleSeed> {
  ROLES
    .roles
    .iter()
    .filter_map(|role| match role_texts(code, role) {
      Ok(texts) => Some(role_seed(role, texts)),
      Err(why) => {
        eprintln!("agency: the role seed {} is not given: {why}", role.name);

        None
      }
    })
    .collect()
}

// A seeded role's row: its order, and its compaction threshold its first provider's
// declared default.
fn role_seed(role: &Role, texts: RoleTexts) -> RoleSeed {
  let order = Order {
    entries: role
      .order
      .iter()
      .map(|entry| Entry {
        provider: entry.provider.clone(),
        model: entry.model.clone(),
        effort: entry.effort.clone(),
        thinking: entry.thinking.map(|on| if on { "on" } else { "off" }.to_owned()),
        answer_tokens: None,
        advisor: None,
      })
      .collect(),
    forced: None,
  };

  RoleSeed {
    name: role.name.clone(),
    label: role.label.clone(),
    bundles: role.bundles.clone(),
    line: role.line.clone(),
    order: order.entries_text(),
    max_concurrent: role.max_concurrent,
    prompt: texts.prompt,
    description: role.description.clone(),
    sessions: role.sessions.word().to_owned(),
    fold_ratio: order.default_fold_ratio(),
    provider_prompts: texts.provider_prompts,
    follows_asker: role.follows_asker,
  }
}

// A role's texts under the code tree: its prompt file's, where it names one, and each provider prompt file's
// by the provider it is for.
struct RoleTexts {
  prompt: Option<String>,
  provider_prompts: Vec<(String, String)>,
}

fn role_texts(code: &Path, role: &Role) -> Result<RoleTexts, String> {
  let read = |file: &str| {
    let path = code.join("prompts").join("roles").join(file);

    std::fs::read_to_string(&path).map_err(|why| format!("{}: {why}", path.display()))
  };
  let prompt = role.prompt.as_deref().map(read).transpose()?;
  let provider_prompts = role
    .provider_prompts
    .iter()
    .map(|(provider, file)| Ok((provider.clone(), read(file)?)))
    .collect::<Result<_, String>>()?;

  Ok(RoleTexts {
    prompt,
    provider_prompts,
  })
}

/// The base prompts the policy declares, each with its seed file's text under the code tree. One whose
/// file cannot be read is said and left out, so it is given by the first settling that can read it.
pub fn base_prompt_seeds(code: &Path) -> Vec<BasePromptSeed> {
  BASE_PROMPTS
    .seed
    .iter()
    .filter_map(|seed| {
      let file = code.join("prompts").join("base").join(&seed.file);

      match std::fs::read_to_string(&file) {
        Ok(text) => Some(BasePromptSeed {
          name: seed.name.clone(),
          label: seed.label.clone(),
          text,
          agents: seed.agents.clone(),
        }),
        Err(why) => {
          eprintln!("agency: the base prompt seed {} is not given: {}: {why}", seed.name, file.display());

          None
        }
      }
    })
    .collect()
}
