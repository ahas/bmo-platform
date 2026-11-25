//! What agency reads off the account service's delegation chain for an organisation beside the prompt layers and
//! the supply sources: the agents an account is given as it is made ready (docs/role-agents.md, 「The agents an
//! account is given」), and who may add and change repositories and which a unit owns (docs/repositories.md,
//! 「Who may add and change a repository」 and 「An organisation's repository」); and a person's own changes over the
//! agents their organisation supplies (docs/role-agents.md, 「A person's own agents」).

pub mod composition;
pub mod own_agents;
pub mod repositories;
pub mod resources;

use axum::http::StatusCode;

use crate::control::Refused;

/// The refusal of a check the account service did not answer: nothing is guessed.
pub fn unchecked() -> Refused {
  Refused::new(
    StatusCode::SERVICE_UNAVAILABLE,
    &policy::control_words::CONTROL_WORDS.organisation_supply.unchecked,
  )
  .with_code("access-unchecked")
}

/// Whether the caller heads the unit, as the delegation chain answers the unit's link of `key` to its writers
/// alone; the platform's call always does.
pub async fn heads(
  ground: &crate::places::Ground,
  acting: crate::access::Acting,
  key: &str,
  unit: i64,
) -> Result<bool, Refused> {
  let crate::access::Acting::Person(person) = acting else { return Ok(true) };
  let read = crate::chain::entry(&crate::apps::service(ground), key, &format!("unit:{unit}"), person)
    .await
    .map_err(|_| unchecked())?;

  match read.status {
    status if (200..300).contains(&status) => Ok(true),
    403 | 404 => Ok(false),
    _ => Err(unchecked()),
  }
}
