//! Who reaches the resources filed under an organisation unit (docs/api.md, 「Who a caller acts for」;
//! docs/repositories.md, 「An organisation's repository」): a row a unit owns is its heads' alone, and heading the
//! unit is the whole of who may, the same rule a unit's repository stands on. The chain's
//! `agency.unit-resources.manage` answers a unit's link to its writers, which is the head check; the platform's
//! call always reaches. Beside the check, the units below a unit, as the account service's subtree answers them
//! to a head of the unit or of a unit above.

use std::collections::HashMap;

use axum::http::StatusCode;
use policy::organisation::ORGANISATION;

use crate::access::Acting;
use crate::chain;
use crate::control::Refused;
use crate::places::Ground;

pub fn key() -> &'static str {
  &ORGANISATION.resources.key
}

/// Whether the caller heads the unit: the platform's call, or the person the chain answers this unit's link to.
pub async fn heads(ground: &Ground, acting: Acting, unit: i64) -> Result<bool, Refused> {
  super::heads(ground, acting, key(), unit).await
}

/// The refusal of a row a caller's unit does not reach.
pub fn refused(what: &str) -> Refused {
  Refused::new(
    StatusCode::FORBIDDEN,
    format!("The signed-in person may not change {what}: it is filed under a unit they do not head"),
  )
  .with_code("forbidden")
}

/// The refusal of a list parameter that is not a whole number or stands against its kind.
fn bad(message: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, message).with_code("bad-request")
}

/// A query value as a whole number.
fn whole(text: &str) -> Option<i64> {
  text.trim().parse::<i64>().ok()
}

/// The owner a list call's parameters name: none of its own, one account, or one unit — with the units under it
/// where the call asks for the subtree.
enum Kept {
  All,
  Account(i64),
  Units(Vec<i64>),
}

/// The caller's stand on the units rows are filed under, asked once per unit per call: whose rows they see, and
/// what they may change. The platform sees and changes every unit's; a person, their own account's rows and those
/// of the units they head, which a head above heads too, as the chain answers the link. A list call may narrow
/// the rows further by its owner parameters, which only stand over rows the caller reaches anyway.
pub struct Reach<'a> {
  ground: &'a Ground,
  acting: Acting,
  cache: HashMap<i64, bool>,
  kept: Kept,
}

impl<'a> Reach<'a> {
  pub fn new(ground: &'a Ground, acting: Acting) -> Self {
    Self {
      ground,
      acting,
      cache: HashMap::new(),
      kept: Kept::All,
    }
  }

  /// The reach of a list call, narrowed by its `ownerAccount`, `unit` and `unitSubtree=true` parameters
  /// (docs/api.md, 「Who a caller acts for」): a member may name only their own account and a unit they head,
  /// and the two axes never together; the rows kept are those the parameters name and the caller reaches.
  pub async fn listed(
    ground: &'a Ground,
    acting: Acting,
    account: Option<&str>,
    unit: Option<&str>,
    subtree: Option<&str>,
  ) -> Result<Self, Refused> {
    let mut reach = Self::new(ground, acting);

    let kept = match (account, unit) {
      (Some(_), Some(_)) => {
        return Err(bad("A list names its owner by one axis: ownerAccount, or unit with unitSubtree."));
      }
      (None, None) => Kept::All,
      (Some(account), None) => {
        let account = whole(account).ok_or_else(|| bad("ownerAccount: a login account's id."))?;

        if acting.person().is_some_and(|person| person != account) {
          return Err(
            Refused::new(
              StatusCode::FORBIDDEN,
              "The signed-in person may name only their own account in a list".to_owned(),
            )
            .with_code("forbidden"),
          );
        }

        Kept::Account(account)
      }
      (None, Some(unit)) => {
        let unit = whole(unit).ok_or_else(|| bad("unit: an organisation unit's id."))?;
        let below = match subtree {
          None => false,
          Some(word) if word.trim() == "true" => true,
          Some(_) => {
            return Err(bad("unitSubtree: true, to take the unit and the units under it as well."));
          }
        };

        if !reach.reaches_unit(Some(unit)).await? {
          return Err(Refused::new(
            StatusCode::FORBIDDEN,
            format!(
              "The signed-in person may name the unit {unit} in a list only where they head it or a unit above"
            ),
          )
          .with_code("forbidden"));
        }

        if !below {
          Kept::Units(vec![unit])
        } else {
          // The units under it are the account service's answer to the reader; where none answers, the named
          // unit's own rows stand alone, nothing guessed.
          let reader = acting.person();
          let mut units = crate::organisation::resources::subtree(ground, unit, reader).await;

          if !units.contains(&unit) {
            units.push(unit);
          }

          Kept::Units(units)
        }
      }
    };

    reach.kept = kept;

    Ok(reach)
  }

  /// Whether the caller reaches a row filed under `unit`, none being the platform's row, which a member does not
  /// reach.
  pub async fn reaches_unit(&mut self, unit: Option<i64>) -> Result<bool, Refused> {
    match (self.acting, unit) {
      (Acting::Platform, _) => Ok(true),
      (_, None) => Ok(false),
      (Acting::Person(person), Some(unit)) => {
        if let Some(held) = self.cache.get(&unit) {
          return Ok(*held);
        }

        let reached = chain::writes_unit(&crate::apps::service(self.ground), key(), unit, person)
          .await
          .map_err(|_| super::unchecked())?;

        self.cache.insert(unit, reached);
        Ok(reached)
      }
    }
  }

  /// Whether the caller reaches a row by its two owners: filed under their own account, or under a unit they
  /// reach, or the platform's for the platform's call alone — and, for a narrowed list, named by its parameters.
  pub async fn reaches_row(
    &mut self,
    owner_account: Option<i64>,
    owner_unit: Option<i64>,
  ) -> Result<bool, Refused> {
    let named = match &self.kept {
      Kept::All => true,
      Kept::Account(account) => owner_account == Some(*account),
      Kept::Units(units) => owner_unit.is_some_and(|unit| units.contains(&unit)),
    };

    if !named {
      return Ok(false);
    }

    match self.acting {
      Acting::Platform => Ok(true),
      Acting::Person(person) => {
        if owner_account == Some(person) {
          return Ok(true);
        }

        if owner_account.is_some() {
          return Ok(false);
        }

        self.reaches_unit(owner_unit).await
      }
    }
  }

  /// The agents whose log a member reaches, from the names a log read stands on: a name's owner is the owners
  /// of its agent row where the agency still holds one, else the owners a ledger run of that agent was opened
  /// on, else neither, which is the platform's own and stands in no member's log. The platform's call keeps
  /// every name (docs/api.md, 「Who a caller acts for」).
  pub async fn reached_agents(&mut self, names: &[String]) -> Result<Vec<String>, Refused> {
    if names.is_empty() {
      return Ok(Vec::new());
    }

    if matches!(self.acting, Acting::Platform) {
      return Ok(names.to_vec());
    }

    let held = store::all(
      &self.ground.db,
      r#"SELECT 1 AS "first", "name", "owner_account", "owner_unit" FROM "supervisors"
           WHERE "name" IN (SELECT jsonb_array_elements_text($1::jsonb))
         UNION ALL
         SELECT 2, "name", "owner_account", "owner_unit" FROM "role_agents"
           WHERE "name" IN (SELECT jsonb_array_elements_text($1::jsonb))
         UNION ALL
         SELECT DISTINCT 3, "agent", "owner_account", "owner_unit" FROM "runs"
           WHERE "agent" IN (SELECT jsonb_array_elements_text($1::jsonb))
         ORDER BY 2, 1"#,
      vec![serde_json::to_string(names).unwrap_or_else(|_| "[]".to_owned()).into()],
    )
    .await?;
    let mut owners: HashMap<String, (Option<i64>, Option<i64>)> = HashMap::new();

    for row in held.iter() {
      let name: String = row.try_get("", "name")?;
      let pair = (
        row.try_get::<Option<i64>>("", "owner_account")?,
        row.try_get::<Option<i64>>("", "owner_unit")?,
      );

      owners.entry(name).or_insert(pair);
    }

    let mut reached = Vec::new();

    for name in names {
      let (account, unit) = owners.get(name).copied().unwrap_or((None, None));

      if self.reaches_row(account, unit).await? {
        reached.push(name.clone());
      }
    }

    Ok(reached)
  }

  /// The owner pairs a member's aggregate read keeps, from the pairs the data shows: the accounts and units
  /// the caller reaches, written for the read's queries to filter by. The platform's call narrows nothing and
  /// answers none here, which its reads read as no filter at all (docs/api.md, 「Who a caller acts for」).
  pub async fn owners_among(
    &mut self,
    pairs: &[(Option<i64>, Option<i64>)],
  ) -> Result<Option<Owners>, Refused> {
    if matches!(self.acting, Acting::Platform) {
      return Ok(None);
    }

    let mut accounts = Vec::new();
    let mut units = Vec::new();

    for (account, unit) in pairs.iter().copied() {
      if !self.reaches_row(account, unit).await? {
        continue;
      }

      match (account, unit) {
        (Some(account), _) if !accounts.contains(&account) => accounts.push(account),
        (None, Some(unit)) if !units.contains(&unit) => units.push(unit),
        _ => {}
      }
    }

    Ok(Some(Owners { accounts, units }))
  }
}

/// The owners a member's aggregate read keeps: the accounts and the units standing under the pairs the data
/// shows that the caller reaches. The predicate is written into a query as whole numbers only, which is why
/// these carry no parameters of their own.
pub struct Owners {
  accounts: Vec<i64>,
  units: Vec<i64>,
}

impl Owners {
  /// The rows kept, as SQL over the two owner columns named by `account` and `unit`: one's own account's, or a
  /// unit's the caller heads; the platform's rows, filed under neither, stand for the platform's call alone.
  pub fn predicate(&self, account: &str, unit: &str) -> String {
    let accounts = owned(&self.accounts);
    let units = owned(&self.units);

    format!(r#"({account} = ANY({accounts}) OR ({account} IS NULL AND {unit} = ANY({units})))"#)
  }
}

// A list of whole numbers as a SQL array; an empty one is typed, so the comparison still reads.
fn owned(ids: &[i64]) -> String {
  match ids.is_empty() {
    true => "ARRAY[]::bigint[]".to_owned(),
    false => format!("ARRAY[{}]", ids.iter().map(i64::to_string).collect::<Vec<_>>().join(", ")),
  }
}

/// The unit and every unit under it, as the account service's subtree answers them to the reader; empty where
/// the reader does not reach the unit or nothing answers.
pub async fn subtree(ground: &Ground, unit: i64, reader: Option<i64>) -> Vec<i64> {
  let Ok(read) =
    chain::read(&crate::apps::service(ground), &format!("/units/{unit}/subtree"), reader).await
  else {
    return Vec::new();
  };

  if !read.ok() {
    return Vec::new();
  }

  read.body["units"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|one| one["id"].as_i64())
    .collect()
}
