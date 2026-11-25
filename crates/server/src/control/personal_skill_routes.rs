//! An account's skills at home across the boundary (docs/api.md, 「personal-skills」): listed, read, saved and
//! removed, each through `crate::skills`. A caller naming a person acts on that person's account alone.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use bmo_agency_client::{PersonalSkillFiles, PersonalSkillList, PersonalSkillRemoved, PersonalSkillSave};

use super::{Call, Refused, Route, answer};
use crate::access::Acting;
use crate::skills;

// The account the path names, refused where a caller naming a person names another.
pub(super) fn account_of(call: &Call) -> Result<String, Refused> {
  let account = call
    .param("ownerAccount")
    .map(str::trim)
    .filter(|account| !account.is_empty())
    .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "an account is required"))?
    .to_owned();

  match Acting::of(call)? {
    Acting::Person(person) if person.to_string() != account => Err(
      Refused::new(StatusCode::FORBIDDEN, "The signed-in person acts on their own account alone")
        .with_code("forbidden"),
    ),
    _ => Ok(account),
  }
}

fn name_of(call: &Call) -> String {
  call.param("name").unwrap_or_default().trim().to_owned()
}

fn json<T: serde::Serialize>(value: &T) -> serde_json::Value {
  serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

/// `GET /personal-skills/{ownerAccount}`: the account's skills at home.
pub struct ListPersonalSkills;

impl Route for ListPersonalSkills {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/personal-skills/{ownerAccount}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;
    let ground = call.shared_ground();
    let skills = skills::list(&ground, &account).await?;

    Ok(answer(StatusCode::OK, json(&PersonalSkillList { skills })))
  }
}

crate::moved!(ListPersonalSkills);

/// `GET /personal-skills/{ownerAccount}/{name}`: one skill's files.
pub struct GetPersonalSkill;

impl Route for GetPersonalSkill {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/personal-skills/{ownerAccount}/{name}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;
    let name = name_of(&call);
    let (held, files) = skills::files(&call.ground().roots.data, &account, &name)?;

    Ok(answer(
      StatusCode::OK,
      json(&PersonalSkillFiles {
        name: held.name.clone(),
        version: held.version.clone(),
        origin: held.origin().to_owned(),
        files,
      }),
    ))
  }
}

crate::moved!(GetPersonalSkill);

/// `PUT /personal-skills/{ownerAccount}/{name}`: one of the account's own skills saved whole.
pub struct PutPersonalSkill;

impl Route for PutPersonalSkill {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/personal-skills/{ownerAccount}/{name}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;
    let name = name_of(&call);
    let ground = call.shared_ground();
    let asked: PersonalSkillSave = serde_json::from_value(call.body().await?)
      .map_err(|why| Refused::new(StatusCode::BAD_REQUEST, format!("the body does not read: {why}")))?;
    let saved = skills::save(&ground, &account, &name, &asked.files).await?;

    Ok(answer(StatusCode::OK, json(&saved)))
  }
}

crate::moved!(PutPersonalSkill);

/// `DELETE /personal-skills/{ownerAccount}/{name}`: one of the account's own skills removed, with its attachments.
pub struct RemovePersonalSkill;

impl Route for RemovePersonalSkill {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/personal-skills/{ownerAccount}/{name}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;
    let name = name_of(&call);
    let ground = call.shared_ground();

    skills::remove(&ground, &account, &name).await?;

    Ok(answer(StatusCode::OK, json(&PersonalSkillRemoved { name })))
  }
}

crate::moved!(RemovePersonalSkill);
