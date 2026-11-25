//! What an item's `model`, `role` and `agent` labels may name, published to the job service, which refuses an
//! item filed or a label changed naming anything else (docs/agent-control.md, 「A label agency does not hold is
//! refused as it is set」). Each key's values are published where they differ from what agency last published,
//! kept in its settings table under `label_values:<key>`, so a job service that did not answer is brought up to
//! date at the next try.

use std::collections::{BTreeMap, BTreeSet};

use sea_orm::DatabaseConnection;
use serde_json::json;

use crate::queue::orderer::JobService;
use crate::queue::settings;
use crate::repositories::{AGENT_LABEL, AUTO, MODEL_LABEL, ROLE_LABEL};
use crate::role_agents::RoleAgent;

/// The settings rows, one a label key, of the values last published for it, as a JSON list.
pub const PUBLISHED_PREFIX: &str = "label_values:";

/// The values each label may name: every role the store holds for `role`; every agent, switched on or off, for
/// `agent`; every model an agent's road names, by its own name and as `<provider>/<model>`, for `model`; and
/// `auto`, which names none, under each.
pub fn values(roles: &[String], agents: &[RoleAgent]) -> BTreeMap<&'static str, Vec<String>> {
  let with_auto = |names: BTreeSet<String>| -> Vec<String> {
    let mut names = names;

    names.insert(AUTO.to_owned());
    names.into_iter().collect()
  };
  let models: BTreeSet<String> = agents
    .iter()
    .filter_map(|agent| Some((agent.provider.as_deref()?, agent.model.as_deref()?)))
    .flat_map(|(provider, model)| [model.to_owned(), format!("{provider}/{model}")])
    .collect();

  BTreeMap::from([
    (ROLE_LABEL, with_auto(roles.iter().cloned().collect())),
    (AGENT_LABEL, with_auto(agents.iter().map(|agent| agent.name.clone()).collect())),
    (MODEL_LABEL, with_auto(models)),
  ])
}

/// Each label's values published to the job service where they differ from what was last published; what could
/// not be read or published, said.
pub async fn publish(db: &DatabaseConnection, service: &JobService) -> Result<(), String> {
  let fault = |why: crate::Refusal| why.detail;
  let roles = crate::agent_roles::names(db).await.map_err(|why| why.detail)?;
  let agents = crate::role_agents::every(db).await.map_err(|why| why.detail)?;

  for (key, values) in values(&roles, &agents) {
    let kept = format!("{PUBLISHED_PREFIX}{key}");
    let listed = json!(values).to_string();

    if settings::get(db, &kept).await.map_err(fault)?.as_deref() == Some(listed.as_str()) {
      continue;
    }

    put(service, key, &values).await?;
    settings::set(db, &kept, &listed).await.map_err(fault)?;
    println!("label values: {key} published, {} values", values.len());
  }

  Ok(())
}

// One key's values put to the job service's `PUT /label-values/{key}`.
async fn put(service: &JobService, key: &str, values: &[String]) -> Result<(), String> {
  let answer = reqwest::Client::new()
    .put(format!("{}/label-values/{key}", service.url))
    .bearer_auth(&service.token)
    .json(&json!({ "values": values }))
    .send()
    .await
    .map_err(|why| why.to_string())?;

  if answer.status().is_success() {
    Ok(())
  } else {
    Err(format!(
      "the job service answered {} to the label values of {key}",
      answer.status().as_u16()
    ))
  }
}
