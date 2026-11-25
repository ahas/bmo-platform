//! A person's own supply as the routes keep it (docs/supply.md, 「What a person brings」; docs/api.md, 「accounts,
//! rest and usage」): the person's keys for the gateway's roads, each the name of a secret in their own space of
//! the secret store, made, read, changed and removed by that person alone; of each person's supply the platform
//! reads only the roads it serves; and a person off-boarded, their supply turned off and their call key revoked.

use std::collections::BTreeMap;

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use policy::control_words::{CONTROL_WORDS, PersonalSupplyWords};
use policy::fill_marks;
use sea_orm::{DatabaseConnection, TransactionTrait};
use serde_json::{Value, json};
use store::{all, one, run};

use super::claude_account_changes::plain_secret;
use super::{Call, Refused, Route, answer, whole};
use crate::access::Acting;
use crate::now_ms;

/// The road a person's Claude account serves, as the people's roads name it.
const CLAUDE_CODE_ROAD: &str = "claude-code";

fn words() -> &'static PersonalSupplyWords {
  &CONTROL_WORDS.personal_supply
}

fn forbidden(why: &str) -> Refused {
  Refused::new(StatusCode::FORBIDDEN, why).with_code("forbidden")
}

// The person a call on their own keys acts for, or the refusal of a call naming none.
fn person(call: &Call) -> Result<i64, Refused> {
  Acting::of(call)?.person().ok_or_else(|| forbidden(&words().no_person))
}

fn unknown(id: &str) -> Refused {
  Refused::new(StatusCode::NOT_FOUND, fill_marks(&words().unknown, &[("id", id)]))
    .with_code("unknown-supply")
}

const COLUMNS: &str = r#""id", "road", "secret", "place", "enabled", "refused_at", "added_at""#;

// One key as a person reads it; its secret is a name, never a value.
fn item(row: &sea_orm::QueryResult) -> Result<Value, Refused> {
  Ok(json!({
    "id": row.try_get::<i64>("", "id")?,
    "road": row.try_get::<String>("", "road")?,
    "secret": row.try_get::<String>("", "secret")?,
    "place": row.try_get::<i64>("", "place")?,
    "enabled": row.try_get::<bool>("", "enabled")?,
    "refusedAt": row.try_get::<Option<i64>>("", "refused_at")?,
    "addedAt": row.try_get::<i64>("", "added_at")?,
  }))
}

// One of the person's keys by the id the path names, none where it is no key of theirs.
async fn own_key(
  db: &impl sea_orm::ConnectionTrait,
  user: i64,
  id: &str,
) -> Result<Option<sea_orm::QueryResult>, Refused> {
  let Ok(id) = id.trim().parse::<i64>() else { return Ok(None) };

  Ok(
    one(
      db,
      &format!(r#"SELECT {COLUMNS} FROM "personal_supply" WHERE "id" = $1 AND "user_id" = $2"#),
      vec![id.into(), user.into()],
    )
    .await?,
  )
}

/// Each person other than the administrator whose supply serves a road now, with the roads it serves:
/// `claude-code` where they hold a Claude account that is on, and the road of each key of theirs that is on.
/// No key's secret is in it.
pub async fn people(db: &DatabaseConnection) -> Result<Vec<Value>, Refused> {
  let administrator = super::claude_accounts::administrator(db).await?;
  let mut roads: BTreeMap<i64, Vec<String>> = BTreeMap::new();

  for row in all(
    db,
    r#"SELECT DISTINCT "user_id" FROM "claude_accounts" WHERE "user_id" IS NOT NULL AND "enabled" = 1"#,
    vec![],
  )
  .await?
  {
    roads.entry(row.try_get("", "user_id")?).or_default().push(CLAUDE_CODE_ROAD.to_owned());
  }

  for row in all(
    db,
    r#"SELECT DISTINCT "user_id", "road" FROM "personal_supply" WHERE "enabled" ORDER BY "user_id", "road""#,
    vec![],
  )
  .await?
  {
    roads.entry(row.try_get("", "user_id")?).or_default().push(row.try_get("", "road")?);
  }

  Ok(
    roads
      .into_iter()
      .filter(|(user, _)| Some(*user) != administrator)
      .map(|(user, roads)| json!({ "user": user, "roads": roads }))
      .collect(),
  )
}

/// `GET /personal-supply`: a person's own keys, or for the platform each person's roads.
pub struct PersonalSupplyList;

impl Route for PersonalSupplyList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/personal-supply";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let db = call.store();
    let Acting::Person(user) = Acting::of(&call)? else {
      return Ok(answer(StatusCode::OK, json!({ "people": people(db).await? })));
    };
    let rows = all(
      db,
      &format!(
        r#"SELECT {COLUMNS} FROM "personal_supply" WHERE "user_id" = $1 ORDER BY "road", "place", "id""#
      ),
      vec![user.into()],
    )
    .await?;
    let items = rows.iter().map(item).collect::<Result<Vec<_>, _>>()?;

    Ok(answer(StatusCode::OK, json!({ "items": items })))
  }
}

crate::moved!(PersonalSupplyList);

/// Whether a road is one some member of the gateway serves, as the deployment's members file names them.
pub fn road_served(data_root: &std::path::Path, road: &str) -> Result<bool, Refused> {
  let members = harness_machine::members::read(&harness_machine::members::path(data_root))
    .map_err(|why| Refused::new(StatusCode::SERVICE_UNAVAILABLE, why))?;

  Ok(members.iter().any(|member| member.road.as_deref() == Some(road)))
}

/// `POST /personal-supply`: a key of the signed-in person's added for a gateway road, last of their keys of it.
pub struct PersonalSupplyAdd;

impl Route for PersonalSupplyAdd {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/personal-supply";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let user = person(&call)?;
    let body = call.body().await?;
    let road = body
      .get("road")
      .and_then(Value::as_str)
      .map(str::trim)
      .filter(|road| !road.is_empty())
      .ok_or_else(|| Refused::failed(&words().road_needed).with_field("road"))?;
    let secret = body
      .get("secret")
      .and_then(Value::as_str)
      .and_then(plain_secret)
      .ok_or_else(|| Refused::failed(&CONTROL_WORDS.accounts.secret_name).with_field("secret"))?;

    if !road_served(&call.data_root(), road)? {
      return Err(
        Refused::new(
          StatusCode::UNPROCESSABLE_ENTITY,
          fill_marks(&words().unknown_road, &[("road", road)]),
        )
        .with_code("unknown-road"),
      );
    }

    let db = call.store();
    let written = db.begin().await?;
    let taken = one(
      &written,
      r#"SELECT 1 AS "taken" FROM "personal_supply" WHERE "user_id" = $1 AND "road" = $2 AND "secret" = $3"#,
      vec![user.into(), road.into(), secret.into()],
    )
    .await?;

    if taken.is_some() {
      return Err(
        Refused::new(StatusCode::CONFLICT, fill_marks(&words().taken, &[("road", road)]))
          .with_code("conflict"),
      );
    }

    let added = one(
      &written,
      &format!(
        r#"INSERT INTO "personal_supply" ("user_id", "road", "secret", "place", "added_at")
           VALUES ($1, $2, $3,
             (SELECT COALESCE(MAX("place"), 0) + 1 FROM "personal_supply" WHERE "user_id" = $1 AND "road" = $2),
             $4)
           RETURNING {COLUMNS}"#
      ),
      vec![user.into(), road.into(), secret.into(), now_ms().into()],
    )
    .await?
    .ok_or_else(|| Refused::failed("the key was not written"))?;
    let key = item(&added)?;

    written.commit().await?;

    Ok(answer(StatusCode::CREATED, key))
  }
}

crate::moved!(PersonalSupplyAdd);

/// `PATCH /personal-supply/{supplyId}`: one of the person's keys moved to another place among their keys of its
/// road, or turned on or off; a refused key turned on is no longer refused.
pub struct PersonalSupplyChange;

impl Route for PersonalSupplyChange {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/personal-supply/{supplyId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let user = person(&call)?;
    let id = call.param("supplyId").unwrap_or_default().to_owned();
    let body = call.body().await?;
    let place = match body.get("place") {
      None | Some(Value::Null) => None,
      Some(place) => Some(
        whole(place)
          .filter(|place| *place >= 1)
          .ok_or_else(|| Refused::failed(&words().place).with_field("place"))?,
      ),
    };
    let enabled = match body.get("enabled") {
      None | Some(Value::Null) => None,
      Some(Value::Bool(enabled)) => Some(*enabled),
      Some(_) => return Err(Refused::failed(&words().enabled).with_field("enabled")),
    };

    if place.is_none() && enabled.is_none() {
      return Err(Refused::failed(&words().nothing));
    }

    let written = call.store().begin().await?;
    let key = own_key(&written, user, &id).await?.ok_or_else(|| unknown(&id))?;
    let key_id: i64 = key.try_get("", "id")?;

    if let Some(enabled) = enabled {
      run(
        &written,
        r#"UPDATE "personal_supply" SET "enabled" = $2,
             "refused_at" = CASE WHEN $2 THEN NULL ELSE "refused_at" END
           WHERE "id" = $1"#,
        vec![key_id.into(), enabled.into()],
      )
      .await?;
    }

    if let Some(place) = place {
      let road: String = key.try_get("", "road")?;
      let mut order: Vec<i64> = all(
        &written,
        r#"SELECT "id" FROM "personal_supply" WHERE "user_id" = $1 AND "road" = $2 AND "id" <> $3
           ORDER BY "place", "id""#,
        vec![user.into(), road.into(), key_id.into()],
      )
      .await?
      .iter()
      .map(|row| row.try_get("", "id"))
      .collect::<Result<_, _>>()?;
      let at = usize::try_from(place - 1).unwrap_or(usize::MAX).min(order.len());

      order.insert(at, key_id);

      for (place, id) in order.iter().enumerate() {
        run(
          &written,
          r#"UPDATE "personal_supply" SET "place" = $2 WHERE "id" = $1"#,
          vec![(*id).into(), (place as i64 + 1).into()],
        )
        .await?;
      }
    }

    let changed = own_key(&written, user, &id).await?.ok_or_else(|| unknown(&id))?;
    let changed = item(&changed)?;

    written.commit().await?;

    Ok(answer(StatusCode::OK, changed))
  }
}

crate::moved!(PersonalSupplyChange);

/// `DELETE /personal-supply/{supplyId}`: one of the person's keys removed; its secret stays where the console
/// keeps it.
pub struct PersonalSupplyRemove;

impl Route for PersonalSupplyRemove {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/personal-supply/{supplyId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let user = person(&call)?;
    let id = call.param("supplyId").unwrap_or_default().to_owned();
    let db = call.store();
    let key = own_key(db, user, &id).await?.ok_or_else(|| unknown(&id))?;

    run(
      db,
      r#"DELETE FROM "personal_supply" WHERE "id" = $1"#,
      vec![key.try_get::<i64>("", "id")?.into()],
    )
    .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(PersonalSupplyRemove);

/// What off-boarding turned off: the person's Claude accounts and their keys that were on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OffBoarded {
  pub accounts: u64,
  pub keys: u64,
  pub local_ais: u64,
}

/// A person's supply switched off (docs/supply.md, 「What a person brings」): each of their Claude accounts, keys
/// and local AIs turned off and their gateway call key revoked; their secrets stay sealed where the console keeps them.
pub async fn off_board(
  db: &DatabaseConnection,
  data_root: &std::path::Path,
  user: i64,
) -> Result<OffBoarded, Refused> {
  let written = db.begin().await?;
  let accounts = run(
    &written,
    r#"UPDATE "claude_accounts" SET "enabled" = 0 WHERE "user_id" = $1 AND "enabled" <> 0"#,
    vec![user.into()],
  )
  .await?;
  let keys = run(
    &written,
    r#"UPDATE "personal_supply" SET "enabled" = false WHERE "user_id" = $1 AND "enabled""#,
    vec![user.into()],
  )
  .await?;
  let local_ais = super::personal_models::off_board(&written, user).await?;

  written.commit().await?;
  super::claude_account_changes::let_choice_go_if_off(db).await?;
  crate::call_keys::revoke(db, data_root, user).await?;

  Ok(OffBoarded {
    accounts,
    keys,
    local_ais,
  })
}

/// `POST /people/{user}/off-boarding`: a person's supply switched off, the deployment's own call alone.
pub struct OffBoarding;

impl Route for OffBoarding {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/people/{user}/off-boarding";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    if Acting::of(&call)? != Acting::Platform {
      return Err(forbidden(&words().off_boarding));
    }

    let named = call.param("user").unwrap_or_default();
    let user = named
      .trim()
      .parse::<i64>()
      .map_err(|_| Refused::failed(format!("{named} is no user id")).with_field("user"))?;
    let done = off_board(call.store(), &call.data_root(), user).await?;

    Ok(answer(
      StatusCode::OK,
      json!({ "accounts": done.accounts, "keys": done.keys, "localAis": done.local_ais }),
    ))
  }
}

crate::moved!(OffBoarding);
