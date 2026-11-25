//! A person's own local AIs as the routes keep them (docs/supply.md, 「A person's local AI」; docs/api.md,
//! 「accounts, rest and usage」): each a registration the person owns, its one address on their PC, added, read,
//! switched and removed by that person alone, while their chain lets them hold one.

use std::sync::LazyLock;

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use policy::fill_marks;
use policy::registry::REGISTRY;
use policy::supply::{LocalAiWords, SUPPLY};
use regex::Regex;
use sea_orm::TransactionTrait;
use serde_json::{Value, json};
use store::{one, run};

use super::{Call, Refused, Route, answer};
use crate::access::Acting;
use crate::local_ai::{self, LocalAi};
use crate::places::Ground;

fn words() -> &'static LocalAiWords {
  &SUPPLY.local_ai.words
}

static MODEL: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(&REGISTRY.model_pattern).expect("the registry's model pattern reads"));
static NAME: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(&REGISTRY.name_pattern).expect("the registry's name pattern reads"));

// The person a call on their own local AIs acts for, or the refusal of a call naming none.
fn person(call: &Call) -> Result<i64, Refused> {
  Acting::of(call)?
    .person()
    .ok_or_else(|| Refused::new(StatusCode::FORBIDDEN, &words().no_person).with_code("forbidden"))
}

fn unknown(name: &str) -> Refused {
  Refused::new(StatusCode::NOT_FOUND, fill_marks(&words().unknown, &[("name", name)]))
    .with_code("unknown-model")
}

/// An address a person's local AI takes: plain http, ending in the chat completions path, with no query, fragment
/// or credentials; its text as the URL parser writes it.
pub fn checked_url(address: &str) -> Result<String, Refused> {
  let refused = || Refused::failed(&words().url).with_field("url");
  let parsed = url::Url::parse(address.trim()).map_err(|_| refused())?;

  if parsed.scheme() != "http"
    || parsed.host_str().is_none_or(str::is_empty)
    || !parsed.path().ends_with("/chat/completions")
    || parsed.query().is_some_and(|query| !query.is_empty())
    || parsed.fragment().is_some_and(|fragment| !fragment.is_empty())
    || !parsed.username().is_empty()
    || parsed.password().is_some()
  {
    return Err(refused());
  }

  Ok(parsed.to_string())
}

/// A secret's name in the person's own space: one plain file name.
pub fn checked_secret(secret: &str) -> Result<String, Refused> {
  super::claude_account_changes::plain_secret(secret.trim())
    .map(str::to_owned)
    .ok_or_else(|| Refused::failed(&words().key_name).with_field("keyName"))
}

// The registration's name a person's given name makes, where both rules hold: the name a lowercase letter, then
// lowercase letters, digits and hyphens, within the policy's length, and the registration's the registry's.
fn checked_name(account: i64, name: &str) -> Result<String, Refused> {
  let registration = local_ai::registration_name(account, name);
  let plain = name.starts_with(|first: char| first.is_ascii_lowercase())
    && name.chars().all(|one| one.is_ascii_lowercase() || one.is_ascii_digit() || one == '-')
    && name.len() <= SUPPLY.local_ai.name_most;

  if !plain || !NAME.is_match(&registration) {
    let most = SUPPLY.local_ai.name_most.to_string();

    return Err(Refused::failed(fill_marks(&words().name, &[("most", &most)])).with_field("name"));
  }

  Ok(registration)
}

// Whether the chain lets the person hold a local AI of their own, refused 403 where it does not and 503 where the
// account service did not answer.
async fn may_hold(ground: &Ground, account: i64) -> Result<(), Refused> {
  match local_ai::allowed(ground, account).await {
    Ok(true) => Ok(()),
    Ok(false) => Err(
      Refused::new(StatusCode::FORBIDDEN, &SUPPLY.test.words.withheld)
        .with_code("personal-local-ai-withheld"),
    ),
    Err(_) => {
      Err(Refused::new(StatusCode::SERVICE_UNAVAILABLE, &words().unchecked).with_code("access-unchecked"))
    }
  }
}

// The person's PC's link spec written again, so a local AI added, switched or removed is listened for at once.
async fn respec(ground: &Ground, account: i64) {
  if let Ok(Some(node)) = crate::person_nodes::node_of(&ground.db, &account.to_string()).await
    && let Err(why) =
      crate::watches::links::write_node_spec(&ground.db, ground.peers.node.as_ref(), &ground.roots, &node)
        .await
  {
    eprintln!("{node}: its link spec was not written again after a local AI moved: {why}");
  }
}

/// `GET /personal-models`: the person's own local AIs, and whether their chain lets them add one.
pub struct PersonalModelList;

impl Route for PersonalModelList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/personal-models";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = person(&call)?;
    let items: Vec<Value> = local_ai::of_person(call.store(), account)
      .await?
      .iter()
      .map(|one| one.view(account))
      .collect();
    // A chain that does not answer leaves the add closed; what the person holds stands all the same.
    let allowed = local_ai::allowed(&call.shared_ground(), account).await.unwrap_or(false);

    Ok(answer(StatusCode::OK, json!({ "allowed": allowed, "items": items })))
  }
}

crate::moved!(PersonalModelList);

/// `PUT /personal-models/{name}`: a local AI of the signed-in person's own made, or the one of that name changed.
pub struct PersonalModelPut;

impl Route for PersonalModelPut {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/personal-models/{name}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let account = person(&call)?;
    let name = call.param("name").unwrap_or_default().trim().to_owned();
    let registration = checked_name(account, &name)?;
    let body = call.body().await?;
    let url = checked_url(body.get("url").and_then(Value::as_str).unwrap_or_default())?;
    let model = body
      .get("modelName")
      .and_then(Value::as_str)
      .map(str::trim)
      .filter(|model| MODEL.is_match(model))
      .ok_or_else(|| Refused::failed(&words().model).with_field("modelName"))?
      .to_owned();
    let key_file = match body.get("keyName") {
      None | Some(Value::Null) => None,
      Some(Value::String(secret)) if secret.trim().is_empty() => None,
      Some(Value::String(secret)) => Some(local_ai::key_file(account, &checked_secret(secret)?)),
      Some(_) => return Err(Refused::failed(&words().key_name).with_field("keyName")),
    };
    let held = local_ai::named(call.store(), account, &name).await?;
    let answer_tokens = SUPPLY.local_ai.answer_tokens;
    let context_tokens = match body.get("contextTokens") {
      None | Some(Value::Null) => {
        held.as_ref().map_or(SUPPLY.local_ai.context_tokens, |held| held.context_tokens)
      }
      Some(named) => super::whole(named)
        .filter(|tokens| REGISTRY.context_tokens.holds(*tokens) && *tokens > answer_tokens)
        .ok_or_else(|| {
          Refused::failed(fill_marks(
            &policy::control_words::CONTROL_WORDS.registry.whole,
            &[
              ("label", &policy::control_words::CONTROL_WORDS.registry.context_label),
              ("min", &REGISTRY.context_tokens.min.to_string()),
              ("max", &REGISTRY.context_tokens.max.to_string()),
            ],
          ))
          .with_field("contextTokens")
        })?,
    };

    let ground = call.shared_ground();

    may_hold(&ground, account).await?;

    if local_ai::pc_of(&ground, account).await?.is_none() {
      return Err(Refused::new(StatusCode::CONFLICT, &SUPPLY.test.words.no_pc).with_code("no-pc"));
    }

    let written = call.store().begin().await?;
    let standing = one(
      &written,
      r#"SELECT "owner_account" FROM "registered_models" WHERE "name" = $1"#,
      vec![registration.clone().into()],
    )
    .await?;

    if let Some(standing) = &standing
      && standing.try_get::<Option<i64>>("", "owner_account")? != Some(account)
    {
      return Err(unknown(&name));
    }

    let node = SUPPLY.local_ai.node.as_str();

    if held.is_some() {
      run(
        &written,
        r#"UPDATE "registered_models" SET "model" = $1, "context_tokens" = $2 WHERE "name" = $3"#,
        vec![model.clone().into(), context_tokens.into(), registration.clone().into()],
      )
      .await?;
      run(
        &written,
        r#"UPDATE "registered_model_addresses" SET "url" = $1, "key_file" = $2, "key_owned" = 0
           WHERE "registration" = $3 AND "reach_port" IS NOT NULL"#,
        vec![url.clone().into(), key_file.clone().into(), registration.clone().into()],
      )
      .await?;
    } else {
      let Some(port) = local_ai::free_port(&written).await? else {
        return Err(Refused::new(StatusCode::CONFLICT, &words().no_port).with_code("no-port"));
      };

      run(
        &written,
        r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens", "thinking",
             "walked", "keeps_research", "added_at", "owner_account", "enabled")
           VALUES ($1, $2, $3, $4, $5, 0, 0, 0, $6, $7, 1)"#,
        vec![
          registration.clone().into(),
          name.clone().into(),
          model.clone().into(),
          context_tokens.into(),
          answer_tokens.into(),
          (crate::now_ms() / 1000).into(),
          account.into(),
        ],
      )
      .await?;
      run(
        &written,
        r#"INSERT INTO "registered_model_addresses" ("registration", "node", "position", "url", "key_file",
             "key_owned", "role_files", "reach_port")
           VALUES ($1, $2, 1, $3, $4, 0, 0, $5)"#,
        vec![
          registration.clone().into(),
          node.into(),
          url.clone().into(),
          key_file.clone().into(),
          port.into(),
        ],
      )
      .await?;
    }

    written.commit().await?;
    super::models::restand(&call);
    respec(&ground, account).await;

    let saved = local_ai::named(call.store(), account, &name).await?.ok_or_else(|| unknown(&name))?;
    let status = if held.is_some() { StatusCode::OK } else { StatusCode::CREATED };

    Ok(answer(status, saved.view(account)))
  }
}

crate::moved!(PersonalModelPut);

/// `PATCH /personal-models/{name}`: one of the person's local AIs turned on or off.
pub struct PersonalModelChange;

impl Route for PersonalModelChange {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/personal-models/{name}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let account = person(&call)?;
    let name = call.param("name").unwrap_or_default().trim().to_owned();
    let body = call.body().await?;
    let Some(enabled) = body.get("enabled").and_then(Value::as_bool) else {
      return Err(Refused::failed(&words().enabled).with_field("enabled"));
    };
    let held: LocalAi =
      local_ai::named(call.store(), account, &name).await?.ok_or_else(|| unknown(&name))?;

    let ground = call.shared_ground();

    if enabled {
      may_hold(&ground, account).await?;
    }

    run(
      call.store(),
      r#"UPDATE "registered_models" SET "enabled" = $1 WHERE "name" = $2"#,
      vec![i64::from(enabled).into(), held.registration.clone().into()],
    )
    .await?;
    respec(&ground, account).await;

    let changed = local_ai::named(call.store(), account, &name).await?.ok_or_else(|| unknown(&name))?;

    Ok(answer(StatusCode::OK, changed.view(account)))
  }
}

crate::moved!(PersonalModelChange);

/// `DELETE /personal-models/{name}`: one of the person's local AIs removed, as the registry removes a registration;
/// their secret stays where the console keeps it.
pub struct PersonalModelRemove;

impl Route for PersonalModelRemove {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/personal-models/{name}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = person(&call)?;
    let name = call.param("name").unwrap_or_default().trim().to_owned();
    let ground = call.shared_ground();
    let held = local_ai::named(call.store(), account, &name).await?.ok_or_else(|| unknown(&name))?;
    let removal = json!({ "action": "remove", "name": held.registration });

    if let Err(sentence) = super::registry::change(call.store(), &call.data_root(), &removal).await? {
      return Err(Refused::new(StatusCode::CONFLICT, sentence).with_code("conflict"));
    }

    super::models::restand(&call);
    respec(&ground, account).await;

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(PersonalModelRemove);

/// A person's local AIs switched off as they are off-boarded (docs/supply.md, 「What a person brings」): how many
/// were on.
pub async fn off_board(db: &impl sea_orm::ConnectionTrait, account: i64) -> Result<u64, Refused> {
  Ok(
    run(
      db,
      r#"UPDATE "registered_models" SET "enabled" = 0
         WHERE "owner_account" = $1 AND "enabled" <> 0
           AND "name" IN (SELECT "registration" FROM "registered_model_addresses" WHERE "reach_port" IS NOT NULL)"#,
      vec![account.into()],
    )
    .await?,
  )
}
