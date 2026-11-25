//! The layers of a supervisor's system prompt that stand between its harness and its role, and the persona's
//! choice (docs/prompt-layers.md): the platform's rules, attached to the platform administrator's supervisors
//! alone; the organisation's instructions, the persona's choice and the person's own instructions, read off the
//! account service's delegation chain for the person a supervisor serves. Agency declares its settings on the chain as its server stands, and
//! again where a resolution finds one undeclared (`crate::chain`).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use harness_machine::supervisor_prompt::{Layers, Personal, organisation};
use policy::prompt_layers::{Kind, PROMPT_LAYERS};
use sea_orm::ConnectionTrait;
use serde_json::Value;
use tool_call::account_service::AccountService;

/// The body the account service takes a kind's declaration in.
pub fn declaration(kind: &Kind) -> Value {
  crate::chain::declaration(kind)
}

/// Every kind agency declares, declared on the account service (`crate::chain::declare`).
pub async fn declare(service: &AccountService) -> Result<(), String> {
  crate::chain::declare(service).await
}

// The last layers this process read off the chain for each account, which a turn stands on while the account
// service does not answer.
static LAST_READ: LazyLock<Mutex<HashMap<i64, Layers>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

// One kind resolved for a person, the kinds declared again where it stood undeclared.
async fn resolved(service: &AccountService, key: &str, person: i64) -> Result<Option<Value>, String> {
  crate::chain::resolve(service, key, crate::chain::Subject::Person(person)).await
}

/// What the chain resolves for `person`, read afresh: the organisation's instructions, the persona's choice and the
/// person's own instructions.
pub struct Read {
  pub organisation: Option<String>,
  pub persona_none: bool,
  pub personal: Personal,
}

/// The person's own instructions as their link keeps them, `{ all, agents }`; none where the value does not read.
pub fn personal_of(value: Option<&Value>) -> Personal {
  value.and_then(|value| serde_json::from_value(value.clone()).ok()).unwrap_or_default()
}

/// The organisation's instructions, the persona's choice and the person's own instructions the chain resolves for
/// `person`, read afresh. The person's own stand only where the chain lets them give any.
pub async fn read_chain(service: &AccountService, person: i64) -> Result<Read, String> {
  let words = &PROMPT_LAYERS;
  let (instructions, persona, allowed, personal) = futures_util::join!(
    resolved(service, &words.organisation.kind.key, person),
    resolved(service, &words.persona.kind.key, person),
    resolved(service, &words.personal.allowed.key, person),
    resolved(service, &words.personal.text.key, person),
  );
  let texts: Vec<String> = instructions?
    .as_ref()
    .and_then(|answer| answer["layers"].as_array())
    .into_iter()
    .flatten()
    .filter_map(|layer| layer["value"].as_str().map(str::to_owned))
    .collect();
  let persona_none = persona?
    .as_ref()
    .and_then(|answer| answer["value"].as_array())
    .is_some_and(|keys| keys.iter().any(|key| key == words.persona.none.as_str()));
  let personal = match crate::chain::flag(allowed?.as_ref(), true) {
    true => personal_of(personal?.as_ref().map(|answer| &answer["value"])),
    false => Personal::default(),
  };

  Ok(Read {
    organisation: organisation(&texts),
    persona_none,
    personal,
  })
}

/// The layers a supervisor owned by `owner` is composed with, or none for one of no account. The platform's
/// rules are attached to a supervisor of no account or of the platform administrator's account; a supervisor
/// of an account has its organisation's instructions, its persona's choice and the person's own instructions,
/// which each agent takes its part of, read off the chain for that person. Where the account service is named and
/// does not answer, the layers this process last read for the account stand; with none read yet, the sentence of
/// why is the error, so no turn goes out without its organisation's instructions.
pub async fn of<C: ConnectionTrait>(
  db: &C,
  service: &AccountService,
  owner: Option<i64>,
) -> Result<Layers, String> {
  let Some(account) = owner else {
    return Ok(Layers {
      platform: true,
      ..Layers::default()
    });
  };
  let platform = crate::account_isolation::owners_own(db, &account.to_string())
    .await
    .map_err(|refusal| refusal.to_string())?;

  if service.address.is_empty() {
    return Ok(Layers {
      platform,
      ..Layers::default()
    });
  }

  match read_chain(service, account).await {
    Ok(read) => {
      let layers = Layers {
        platform,
        organisation: read.organisation,
        persona_none: read.persona_none,
        personal: read.personal,
      };

      if let Ok(mut kept) = LAST_READ.lock() {
        kept.insert(account, layers.clone());
      }

      Ok(layers)
    }
    Err(why) => {
      let kept = LAST_READ.lock().ok().and_then(|kept| kept.get(&account).cloned());

      kept.ok_or_else(|| {
        format!("the organisation's instructions for account {account} could not be read: {why}")
      })
    }
  }
}
