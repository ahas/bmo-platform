//! Who may add and change repositories, and a repository an organisation unit owns (docs/repositories.md, 「Who
//! may add and change a repository」 and 「An organisation's repository」): the chain's `repositories.manage`,
//! resolved for the person a member's write is made by and for a unit a repository is filed under, and the units
//! on a person's path, which a unit's repository takes the work of alone.

use axum::http::StatusCode;
use policy::organisation::ORGANISATION;

use crate::access::Acting;
use crate::chain::{self, Subject};
use crate::control::Refused;
use crate::places::Ground;

fn key() -> &'static str {
  &ORGANISATION.repositories.key
}

fn withheld() -> Refused {
  Refused::new(
    StatusCode::FORBIDDEN,
    "The signed-in person's organisation does not let them add or change repositories",
  )
  .with_code("repositories-withheld")
}

/// Whether the caller may add and change repositories at all: the platform's call always; a person where the
/// chain's `repositories.manage` resolves true for them, which it does where nothing says otherwise.
pub async fn may_manage(ground: &Ground, acting: Acting) -> Result<(), Refused> {
  let Acting::Person(person) = acting else { return Ok(()) };
  let answer = chain::resolve(&crate::apps::service(ground), key(), Subject::Person(person))
    .await
    .map_err(|_| super::unchecked())?;

  if chain::flag(answer.as_ref(), true) { Ok(()) } else { Err(withheld()) }
}

/// Whether the caller may change a repository the unit owns: the platform's call, or a head of the unit.
pub async fn heads(ground: &Ground, acting: Acting, unit: i64, name: &str) -> Result<(), Refused> {
  if super::heads(ground, acting, key(), unit).await? {
    return Ok(());
  }

  Err(
    Refused::new(
      StatusCode::FORBIDDEN,
      format!("The signed-in person may not change the repository {name}: it is an organisation's they do not head"),
    )
    .with_code("forbidden"),
  )
}

/// Whether the caller may file a repository under the unit: the platform's call, or a head of the unit where
/// `repositories.manage` resolves true for the unit.
pub async fn may_file_under_unit(ground: &Ground, acting: Acting, unit: i64) -> Result<(), Refused> {
  if acting == Acting::Platform {
    return Ok(());
  }

  heads(ground, acting, unit, "being added").await?;

  let answer = chain::resolve(&crate::apps::service(ground), key(), Subject::Unit(unit))
    .await
    .map_err(|_| super::unchecked())?;

  if chain::flag(answer.as_ref(), true) { Ok(()) } else { Err(withheld()) }
}

/// Whether work held by an account may stand on a repository the unit owns: only a person whose path runs
/// through the unit, so the unit's people's agents alone work there. Refused 403 `foreign-repository` otherwise.
pub async fn works_on_unit(
  ground: &Ground,
  account: Option<i64>,
  unit: i64,
  repository: &str,
) -> Result<(), Refused> {
  let foreign = || {
    Refused::new(
      StatusCode::FORBIDDEN,
      format!(
        "The repository {repository} is an organisation's: work of no one under it does not stand on it"
      ),
    )
    .with_code("foreign-repository")
  };
  let Some(account) = account else { return Err(foreign()) };
  let answer = chain::resolve(&crate::apps::service(ground), key(), Subject::Person(account))
    .await
    .map_err(|_| super::unchecked())?;

  if crate::supply::sources::path_units(answer.as_ref()).contains(&unit) {
    Ok(())
  } else {
    Err(foreign())
  }
}
