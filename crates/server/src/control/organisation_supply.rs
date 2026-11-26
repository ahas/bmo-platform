//! An organisation's own supply as the routes keep it (docs/supply.md, 「Organisation supply」; docs/api.md,
//! 「accounts, rest and usage」): the keys a unit registers for the gateway's roads and the Anthropic API keys it
//! registers for Claude Code, each the name of a secret in the unit's own space of the secret store, made, read,
//! changed and removed by a head of the unit or the owner; and the sources the order a person's runs walk holds,
//! which the console reads to ask a person for a login of their own where nothing else carries their runs.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use policy::control_words::{CONTROL_WORDS, OrganisationSupplyWords};
use policy::fill_marks;
use policy::organisation::ORGANISATION;
use sea_orm::TransactionTrait;
use serde_json::{Value, json};
use store::{all, one, run};

use super::claude_account_changes::{
  self as changes, BasePromptGiven, CredentialGiven, Owning, plain_secret,
};
use super::claude_accounts::{CredentialKind, accounts};
use super::personal_supply::road_served;
use super::{Call, Refused, Route, answer};
use crate::access::Acting;
use crate::chain::{self, Subject};
use crate::now_ms;
use crate::places::Ground;
use crate::supply::sources;

/// The road a unit's Anthropic API key serves, which stands as a Claude account of the unit's.
const CLAUDE_CODE_ROAD: &str = "claude-code";
/// The mark of a gateway key's id, and of a Claude account's, as an item names it.
const KEY: &str = "key-";
const ACCOUNT: &str = "account-";

fn words() -> &'static OrganisationSupplyWords {
  &CONTROL_WORDS.organisation_supply
}

fn unit_of(call: &Call) -> Result<i64, Refused> {
  call
    .param("unit")
    .and_then(|unit| unit.trim().parse::<i64>().ok())
    .filter(|unit| *unit > 0)
    .ok_or_else(|| Refused::new(StatusCode::NOT_FOUND, fill_marks(&words().unknown, &[("id", "")])))
}

/// The caller as a head of the unit, or the owner: a person is taken where the delegation chain answers them the
/// unit's link of organisation supply, which it answers its writers alone.
async fn head_of(ground: &Ground, acting: Acting, unit: i64) -> Result<(), Refused> {
  let Acting::Person(person) = acting else { return Ok(()) };
  let service = crate::apps::service(ground);
  let read =
    chain::entry(&service, &ORGANISATION.sources.organisation_kind, &format!("unit:{unit}"), person)
      .await
      .map_err(|_| {
        Refused::new(StatusCode::SERVICE_UNAVAILABLE, &words().unchecked).with_code("access-unchecked")
      })?;

  match read.status {
    status if (200..300).contains(&status) => Ok(()),
    403 | 404 => Err(Refused::new(StatusCode::FORBIDDEN, &words().not_head).with_code("forbidden")),
    _ => {
      Err(Refused::new(StatusCode::SERVICE_UNAVAILABLE, &words().unchecked).with_code("access-unchecked"))
    }
  }
}

/// Whether organisation supply stands open for the unit on the chain.
async fn open_for_unit(ground: &Ground, unit: i64) -> Result<bool, Refused> {
  let service = crate::apps::service(ground);
  let answer = chain::resolve(&service, &ORGANISATION.sources.organisation_kind, Subject::Unit(unit))
    .await
    .map_err(|_| {
      Refused::new(StatusCode::SERVICE_UNAVAILABLE, &words().unchecked).with_code("access-unchecked")
    })?;

  Ok(sources::organisation_open(answer.as_ref()))
}

fn unknown(id: &str) -> Refused {
  Refused::new(StatusCode::NOT_FOUND, fill_marks(&words().unknown, &[("id", id)]))
    .with_code("unknown-supply")
}

const COLUMNS: &str = r#""id", "road", "secret", "place", "enabled", "refused_at", "added_at""#;

// One gateway key as a head reads it; its secret is a name, never a value.
fn key_item(row: &sea_orm::QueryResult) -> Result<Value, Refused> {
  Ok(json!({
    "id": format!("{KEY}{}", row.try_get::<i64>("", "id")?),
    "road": row.try_get::<String>("", "road")?,
    "secret": row.try_get::<String>("", "secret")?,
    "enabled": row.try_get::<bool>("", "enabled")?,
    "refusedAt": row.try_get::<Option<i64>>("", "refused_at")?,
  }))
}

// The secret an `api-key` account's key file names, as the unit's space holds it.
fn secret_of(key_file: Option<&std::path::Path>) -> String {
  key_file
    .and_then(|file| file.file_name())
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_default()
}

/// Every item of the unit's own supply: its gateway keys by road and place, then its Claude accounts.
async fn items(ground: &Ground, unit: i64) -> Result<Vec<Value>, Refused> {
  let db = &ground.db;
  let mut items = all(
    db,
    &format!(
      r#"SELECT {COLUMNS} FROM "personal_supply" WHERE "owner_unit" = $1 ORDER BY "road", "place", "id""#
    ),
    vec![unit.into()],
  )
  .await?
  .iter()
  .map(key_item)
  .collect::<Result<Vec<_>, _>>()?;

  for account in accounts(db).await?.into_iter().filter(|one| one.owner_unit == Some(unit)) {
    items.push(json!({
      "id": format!("{ACCOUNT}{}", account.id),
      "road": CLAUDE_CODE_ROAD,
      "secret": secret_of(account.credential.key_file()),
      "name": account.name,
      "enabled": account.enabled,
      "refusedAt": account.needs_login_at,
    }));
  }

  Ok(items)
}

/// `GET /organisation-supply/{unit}`: whether organisation supply stands open for the unit, and its own keys and
/// Claude accounts, for a head of the unit and the owner.
pub struct OrganisationSupplyList;

impl Route for OrganisationSupplyList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/organisation-supply/{unit}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let unit = unit_of(&call)?;
    let (ground, acting) = (call.shared_ground(), Acting::of(&call)?);

    head_of(&ground, acting, unit).await?;

    let open = open_for_unit(&ground, unit).await?;

    Ok(answer(
      StatusCode::OK,
      json!({ "unit": unit, "open": open, "items": items(&ground, unit).await? }),
    ))
  }
}

crate::moved!(OrganisationSupplyList);

/// `POST /organisation-supply/{unit}`: a key of the unit's added, last of its keys of the road: for a gateway road a
/// key the gateway lays as the unit's own member, for `claude-code` an Anthropic API key standing as a Claude
/// account of the unit's.
pub struct OrganisationSupplyAdd;

impl Route for OrganisationSupplyAdd {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/organisation-supply/{unit}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let unit = unit_of(&call)?;
    let (ground, acting) = (call.shared_ground(), Acting::of(&call)?);
    let body = call.body().await?;

    head_of(&ground, acting, unit).await?;

    if !open_for_unit(&ground, unit).await? {
      return Err(
        Refused::new(StatusCode::CONFLICT, &words().closed).with_code("organisation-supply-closed"),
      );
    }

    let road = body
      .get("road")
      .and_then(Value::as_str)
      .map(str::trim)
      .filter(|road| !road.is_empty())
      .ok_or_else(|| Refused::failed(&CONTROL_WORDS.personal_supply.road_needed).with_field("road"))?
      .to_owned();
    let secret = body
      .get("secret")
      .and_then(Value::as_str)
      .and_then(plain_secret)
      .ok_or_else(|| Refused::failed(&CONTROL_WORDS.accounts.secret_name).with_field("secret"))?
      .to_owned();

    if road == CLAUDE_CODE_ROAD {
      return add_account(&ground, unit, &secret).await;
    }

    if !road_served(&ground.roots.data, &road)? {
      return Err(
        Refused::new(
          StatusCode::UNPROCESSABLE_ENTITY,
          fill_marks(&CONTROL_WORDS.personal_supply.unknown_road, &[("road", &road)]),
        )
        .with_code("unknown-road"),
      );
    }

    let written = ground.db.begin().await?;
    let taken = one(
      &written,
      r#"SELECT 1 AS "taken" FROM "personal_supply" WHERE "owner_unit" = $1 AND "road" = $2 AND "secret" = $3"#,
      vec![unit.into(), road.clone().into(), secret.clone().into()],
    )
    .await?;

    if taken.is_some() {
      return Err(taken_refusal(&road));
    }

    let added = one(
      &written,
      &format!(
        r#"INSERT INTO "personal_supply" ("owner_unit", "road", "secret", "place", "added_at")
           VALUES ($1, $2, $3,
             (SELECT COALESCE(MAX("place"), 0) + 1 FROM "personal_supply" WHERE "owner_unit" = $1 AND "road" = $2),
             $4)
           RETURNING {COLUMNS}"#
      ),
      vec![unit.into(), road.into(), secret.into(), now_ms().into()],
    )
    .await?
    .ok_or_else(|| Refused::failed("the key was not written"))?;
    let key = key_item(&added)?;

    written.commit().await?;

    Ok(answer(StatusCode::CREATED, key))
  }
}

crate::moved!(OrganisationSupplyAdd);

fn taken_refusal(road: &str) -> Refused {
  Refused::new(StatusCode::CONFLICT, fill_marks(&words().taken, &[("road", road)])).with_code("conflict")
}

/// A unit's Anthropic API key added as an `api-key` Claude account of the unit's: its configuration home a folder
/// of its own under the data root, its key the secret in the unit's space.
async fn add_account(ground: &Ground, unit: i64, secret: &str) -> Result<Response, Refused> {
  let data_root = &ground.roots.data;
  let db = &ground.db;
  let key_file = data_root.join("env").join("units").join(unit.to_string()).join(secret);

  if accounts(db).await?.iter().any(|one| {
    one.owner_unit == Some(unit) && one.credential.key_file().is_some_and(|file| file == key_file.as_path())
  }) {
    return Err(taken_refusal(CLAUDE_CODE_ROAD));
  }

  let name = format!("unit-{unit}-{secret}");
  let home = data_root.join("organisation-homes").join(&name);

  std::fs::create_dir_all(&home)
    .map_err(|why| Refused::failed(format!("the account's home was not made: {why}")))?;

  let given = CredentialGiven {
    kind: Some(CredentialKind::ApiKey),
    path: Some(home.to_string_lossy().into_owned()),
    key_file: Some(key_file.to_string_lossy().into_owned()),
    ..CredentialGiven::default()
  };
  let owning = Owning {
    owner_account: None,
    owner_unit: Some(unit),
    node: None,
  };
  let id = changes::add_owned(db, &name, &given, BasePromptGiven::Keep, &owning).await?;
  let added = items(ground, unit)
    .await?
    .into_iter()
    .find(|item| item["id"] == format!("{ACCOUNT}{id}"))
    .unwrap_or(Value::Null);

  Ok(answer(StatusCode::CREATED, added))
}

// One item of the unit's by the id the path names: a gateway key's row id, or a Claude account's id.
enum Item {
  Key(i64),
  Account(String),
}

async fn item_of(ground: &Ground, named: &str, unit: i64) -> Result<Item, Refused> {
  if let Some(id) = named.strip_prefix(KEY).and_then(|id| id.parse::<i64>().ok()) {
    let held = one(
      &ground.db,
      r#"SELECT 1 AS "held" FROM "personal_supply" WHERE "id" = $1 AND "owner_unit" = $2"#,
      vec![id.into(), unit.into()],
    )
    .await?;

    return held.map(|_| Item::Key(id)).ok_or_else(|| unknown(named));
  }

  if let Some(id) = named.strip_prefix(ACCOUNT)
    && accounts(&ground.db)
      .await?
      .iter()
      .any(|one| one.id == id && one.owner_unit == Some(unit))
  {
    return Ok(Item::Account(id.to_owned()));
  }

  Err(unknown(named))
}

/// `PATCH /organisation-supply/{unit}/{supplyId}`: one of the unit's keys or accounts turned on or off; a refused
/// key turned on is no longer refused.
pub struct OrganisationSupplyChange;

impl Route for OrganisationSupplyChange {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/organisation-supply/{unit}/{supplyId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let unit = unit_of(&call)?;
    let named = call.param("supplyId").unwrap_or_default().to_owned();
    let (ground, acting) = (call.shared_ground(), Acting::of(&call)?);
    let body = call.body().await?;

    head_of(&ground, acting, unit).await?;

    let item = item_of(&ground, &named, unit).await?;
    let enabled = body
      .get("enabled")
      .and_then(Value::as_bool)
      .ok_or_else(|| Refused::failed(&words().enabled).with_field("enabled"))?;

    match item {
      Item::Key(id) => {
        run(
          &ground.db,
          r#"UPDATE "personal_supply" SET "enabled" = $2,
               "refused_at" = CASE WHEN $2 THEN NULL ELSE "refused_at" END WHERE "id" = $1"#,
          vec![id.into(), enabled.into()],
        )
        .await?;
      }
      Item::Account(id) => {
        changes::update(&ground.db, &id, None, None, Some(enabled), BasePromptGiven::Keep).await?;
      }
    }

    Ok(answer(StatusCode::OK, json!({ "unit": unit, "items": items(&ground, unit).await? })))
  }
}

crate::moved!(OrganisationSupplyChange);

/// `DELETE /organisation-supply/{unit}/{supplyId}`: one of the unit's keys or accounts removed.
pub struct OrganisationSupplyRemove;

impl Route for OrganisationSupplyRemove {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/organisation-supply/{unit}/{supplyId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let unit = unit_of(&call)?;
    let named = call.param("supplyId").unwrap_or_default().to_owned();
    let (ground, acting) = (call.shared_ground(), Acting::of(&call)?);

    head_of(&ground, acting, unit).await?;

    match item_of(&ground, &named, unit).await? {
      Item::Key(id) => {
        run(&ground.db, r#"DELETE FROM "personal_supply" WHERE "id" = $1"#, vec![id.into()]).await?;
      }
      Item::Account(id) => changes::remove(&ground.db, &id).await?,
    }

    Ok(answer(StatusCode::NO_CONTENT, Value::Null))
  }
}

crate::moved!(OrganisationSupplyRemove);

/// Whether any unit of `units` holds supply of its own that is on: a gateway key, a Claude account, a member
/// of a gateway road or a registration (`docs/supply.md`, 「Organisation supply」).
async fn any_supply(ground: &Ground, units: &[i64]) -> Result<bool, Refused> {
  if units.is_empty() {
    return Ok(false);
  }

  let members =
    harness_machine::members::read(&harness_machine::members::path(&ground.roots.data)).unwrap_or_default();

  for unit in units {
    let keyed = one(
      &ground.db,
      r#"SELECT 1 AS "held" FROM "personal_supply" WHERE "owner_unit" = $1 AND "enabled" LIMIT 1"#,
      vec![(*unit).into()],
    )
    .await?
    .is_some();

    let registered = one(
      &ground.db,
      r#"SELECT 1 AS "held" FROM "registered_models" WHERE "owner_unit" = $1 LIMIT 1"#,
      vec![(*unit).into()],
    )
    .await?
    .is_some();

    if keyed
      || registered
      || members.iter().any(|member| member.unit == Some(*unit))
      || accounts(&ground.db)
        .await?
        .iter()
        .any(|one| one.owner_unit == Some(*unit) && one.enabled)
    {
      return Ok(true);
    }
  }

  Ok(false)
}

/// Whether the person holds a credential of their own that is on: a Claude login or key, a gateway key, or a local
/// AI.
async fn own_supply(ground: &Ground, person: i64) -> Result<bool, Refused> {
  let keyed = one(
    &ground.db,
    r#"SELECT 1 AS "held" FROM "personal_supply" WHERE "owner_account" = $1 AND "enabled" LIMIT 1"#,
    vec![person.into()],
  )
  .await?
  .is_some();

  let local = crate::local_ai::of_person(&ground.db, person).await?.iter().any(|one| one.enabled);

  Ok(
    keyed
      || local
      || accounts(&ground.db)
        .await?
        .iter()
        .any(|one| one.owner_account == Some(person) && one.enabled),
  )
}

/// `GET /supply-sources`: the sources the order the signed-in person's runs walk holds — their own, their
/// organisation's and the platform's — whether a unit on their path holds supply of its own, whether their own is
/// all that can carry their runs, whether they hold one of their own that is on, and every provider the build
/// carries as it stands for them, closed providers named with what closes them
/// (`docs/supply.md`, 「A closed provider is read as closed, not as absent」).
pub struct SupplySources;

impl Route for SupplySources {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/supply-sources";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let Acting::Person(person) = Acting::of(&call)? else {
      return Err(Refused::new(StatusCode::FORBIDDEN, &words().no_person).with_code("forbidden"));
    };
    let ground = call.shared_ground();
    let ask = crate::supply::hook::OrderAsk {
      run: format!("supply-sources-{person}-{}", now_ms()),
      account: person,
      provider: String::new(),
      model: String::new(),
      kind: crate::supply::Kind::Turn,
      job: None,
      supervisor: None,
      room: None,
    };
    let order = crate::supply::hook::order(&ground.hooks, ask)
      .await
      .unwrap_or_else(|| vec![crate::supply::PERSONAL.to_owned()]);
    let organisation = crate::supply::organisation_word();
    let personal = order.iter().any(|source| source == crate::supply::PERSONAL);
    let platform = order.iter().any(|source| source != crate::supply::PERSONAL && source != organisation);
    let units = sources::units_for(&ground, person, &order).await;
    let organisation_supply = any_supply(&ground, &units).await?;
    let own_supply = own_supply(&ground, person).await?;
    // A local AI may be added where the chain allows it and a PC of the person's own reaches its address; a chain
    // that does not answer offers none.
    let local_ai = crate::local_ai::allowed(&ground, person).await.unwrap_or(false)
      && crate::local_ai::pc_of(&ground, person).await?.is_some();
    let mut providers = Vec::new();

    for name in harness_providers::ARCHIVE_PROVIDERS {
      let off = crate::supply::provider_state(&ground, name, person).await?;

      providers.push(json!({
        "provider": name,
        "off": off.as_ref().map(|off| off.word()),
        "from": off.as_ref().and_then(|off| off.link()),
      }));
    }

    Ok(answer(
      StatusCode::OK,
      json!({
        "personal": personal,
        "platform": platform,
        "organisation": !units.is_empty(),
        "organisationSupply": organisation_supply,
        "personalOnly": personal && !platform && !organisation_supply,
        "ownSupply": own_supply,
        "localAi": local_ai,
        "providers": providers,
      }),
    ))
  }
}

crate::moved!(SupplySources);
