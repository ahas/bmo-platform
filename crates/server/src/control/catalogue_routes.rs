//! A company's catalogue of shared personal apps across the boundary (docs/api.md, 「catalogue」): read, shared,
//! installed, moved, dropped, adopted, turned on, recalled, withdrawn, approved and put forward, each through
//! `crate::catalogue`. A caller naming a person acts for that person's account alone.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use bmo_agency_client::{
  CatalogueApprove, CataloguePromote, CataloguePromotions, CatalogueRecall, CatalogueSeen, CatalogueShare,
  CatalogueText, CatalogueUnitSwitch,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::{Call, Refused, Route, answer};
use crate::access::Acting;
use crate::catalogue;

// The account the path names, refused where a caller naming a person names another.
fn account_of(call: &Call) -> Result<String, Refused> {
  let account = call
    .param("ownerAccount")
    .map(str::trim)
    .filter(|account| !account.is_empty())
    .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "an account is required"))?
    .to_owned();

  match Acting::of(call)? {
    Acting::Person(person) if person.to_string() != account => Err(
      Refused::new(
        StatusCode::FORBIDDEN,
        "The signed-in person acts on their own account's catalogue alone",
      )
      .with_code("forbidden"),
    ),
    _ => Ok(account),
  }
}

// What the call names before anything is awaited: the account, the company the administrator asks for, and the
// ground, so no reference to the call is held across an await.
fn asked_of(call: &Call) -> Result<(String, Option<i64>, std::sync::Arc<crate::places::Ground>), Refused> {
  let account = account_of(call)?;
  let company = match call.query("company") {
    Some(company) => Some(company.trim().parse().map_err(|_| {
      Refused::new(StatusCode::BAD_REQUEST, "company is a unit's id").with_field("company")
    })?),
    None => None,
  };

  Ok((account, company, call.shared_ground()))
}

fn app_of(call: &Call) -> String {
  call.param("appId").unwrap_or_default().trim().to_owned()
}

// The body read as the type the route takes, refused where it does not read.
async fn body_of<T: DeserializeOwned>(call: &mut Call) -> Result<T, Refused> {
  let body = call.body().await.unwrap_or(Value::Object(Default::default()));

  serde_json::from_value(body)
    .map_err(|why| Refused::new(StatusCode::BAD_REQUEST, format!("the body does not read: {why}")))
}

fn json<T: serde::Serialize>(value: &T) -> Value {
  serde_json::to_value(value).unwrap_or(Value::Null)
}

/// `GET /catalogue/{ownerAccount}`: the company's catalogue as the account sees it.
pub struct ListCatalogue;

impl Route for ListCatalogue {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/catalogue/{ownerAccount}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let (account, company, ground) = asked_of(&call)?;
    let reader = catalogue::reader(&ground, &account, company).await?;

    Ok(answer(StatusCode::OK, json(&catalogue::list(&ground, &reader).await?)))
  }
}

crate::moved!(ListCatalogue);

/// `GET /catalogue/{ownerAccount}/apps/{appId}/source`: a version's source, for a reader who sees the app.
pub struct CatalogueAppSource;

impl Route for CatalogueAppSource {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/catalogue/{ownerAccount}/apps/{appId}/source";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let (account, company, ground) = asked_of(&call)?;
    let (version, id) = (call.query("version"), app_of(&call));
    let reader = catalogue::reader(&ground, &account, company).await?;
    let source = catalogue::source(&ground, &reader, &id, version.as_deref()).await?;

    Ok(answer(StatusCode::OK, json(&source)))
  }
}

crate::moved!(CatalogueAppSource);

/// `POST /catalogue/{ownerAccount}/apps`: one of the account's personal apps shared.
pub struct ShareCatalogueApp;

impl Route for ShareCatalogueApp {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/catalogue/{ownerAccount}/apps";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let (account, company, ground) = asked_of(&call)?;
    let asked: CatalogueShare = body_of(&mut call).await?;
    let reader = catalogue::reader(&ground, &account, company).await?;
    let shared = catalogue::share(
      &ground,
      &reader,
      asked.name.trim(),
      &asked.targets,
      (asked.kind.as_deref(), asked.language.as_deref()),
    )
    .await?;

    Ok(answer(StatusCode::OK, json(&shared)))
  }
}

crate::moved!(ShareCatalogueApp);

// One change of an app named by the path, answered with the app as it then stands.
macro_rules! app_route {
  ($name:ident, $method:expr, $path:literal, $doc:literal, |$call:ident, $ground:ident, $reader:ident, $id:ident, $a:ident| $asked:block => $body:expr) => {
    #[doc = $doc]
    pub struct $name;

    impl Route for $name {
      const METHOD: Method = $method;
      const PATH: &'static str = $path;

      #[allow(unused_mut)]
      async fn answer(&self, mut $call: Call) -> Result<Response, Refused> {
        let (account, company, $ground) = asked_of(&$call)?;
        let $id = app_of(&$call);
        #[allow(clippy::let_unit_value, unused_variables)]
        let $a = $asked;
        let $reader = catalogue::reader(&$ground, &account, company).await?;
        let done = $body;

        Ok(answer(StatusCode::OK, json(&done)))
      }
    }

    crate::moved!($name);
  };
}

app_route!(
  InstallCatalogueApp,
  Method::POST,
  "/catalogue/{ownerAccount}/apps/{appId}/install",
  "`POST /catalogue/{ownerAccount}/apps/{appId}/install`: the app's newest version installed for the account.",
  |call, ground, reader, id, asked| {} => catalogue::install(&ground, &reader, &id).await?
);

app_route!(
  UpdateCatalogueApp,
  Method::POST,
  "/catalogue/{ownerAccount}/apps/{appId}/update",
  "`POST /catalogue/{ownerAccount}/apps/{appId}/update`: the account's copy moved onto the newest version.",
  |call, ground, reader, id, asked| {} => catalogue::update(&ground, &reader, &id).await?
);

app_route!(
  UninstallCatalogueApp,
  Method::DELETE,
  "/catalogue/{ownerAccount}/apps/{appId}/install",
  "`DELETE /catalogue/{ownerAccount}/apps/{appId}/install`: the account's copy dropped.",
  |call, ground, reader, id, asked| {} => catalogue::uninstall(&ground, &reader, &id).await?
);

app_route!(
  CloneCatalogueApp,
  Method::POST,
  "/catalogue/{ownerAccount}/apps/{appId}/clone",
  "`POST /catalogue/{ownerAccount}/apps/{appId}/clone`: the account's copy adopted as its own personal app.",
  |call, ground, reader, id, asked| {} => catalogue::adopt(&ground, &reader, &id).await?
);

app_route!(
  SwitchCatalogueAppUnit,
  Method::POST,
  "/catalogue/{ownerAccount}/apps/{appId}/units/{unit}",
  "`POST /catalogue/{ownerAccount}/apps/{appId}/units/{unit}`: the app turned on or off for a unit the account heads.",
  |call, ground, reader, id, asked| {
    let unit: i64 = call
      .param("unit")
      .unwrap_or_default()
      .trim()
      .parse()
      .map_err(|_| Refused::new(StatusCode::BAD_REQUEST, "unit is a unit's id").with_field("unit"))?;
    let switch: CatalogueUnitSwitch = body_of(&mut call).await?;

    (unit, switch)
  } => catalogue::switch_unit(&ground, &reader, &id, asked.0, asked.1.on, asked.1.lock).await?
);

app_route!(
  RecallCatalogueApp,
  Method::POST,
  "/catalogue/{ownerAccount}/apps/{appId}/recall",
  "`POST /catalogue/{ownerAccount}/apps/{appId}/recall`: the app recalled at a unit the account heads, or at the platform.",
  |call, ground, reader, id, asked| {
    let recall: CatalogueRecall = body_of(&mut call).await?;

    recall
  } => catalogue::recall(&ground, &reader, &id, asked.unit).await?
);

app_route!(
  WithdrawCatalogueApp,
  Method::POST,
  "/catalogue/{ownerAccount}/apps/{appId}/withdraw",
  "`POST /catalogue/{ownerAccount}/apps/{appId}/withdraw`: the app withdrawn by its owner or maker.",
  |call, ground, reader, id, asked| {} => catalogue::withdraw(&ground, &reader, &id).await?
);

app_route!(
  ApproveCatalogueApp,
  Method::POST,
  "/catalogue/{ownerAccount}/apps/{appId}/approve",
  "`POST /catalogue/{ownerAccount}/apps/{appId}/approve`: a version approved by a head of the maker's home unit.",
  |call, ground, reader, id, asked| {
    let approve: CatalogueApprove = body_of(&mut call).await?;

    approve
  } => catalogue::approve(&ground, &reader, &id, asked.version.trim()).await?
);

app_route!(
  PromoteCatalogueApp,
  Method::POST,
  "/catalogue/{ownerAccount}/apps/{appId}/promote",
  "`POST /catalogue/{ownerAccount}/apps/{appId}/promote`: the request to make the app a platform app.",
  |call, ground, reader, id, asked| {
    let promote: CataloguePromote = body_of(&mut call).await?;

    promote
  } => catalogue::promote(&ground, &reader, &id, asked.note.trim()).await?
);

app_route!(
  TextCatalogueApp,
  Method::POST,
  "/catalogue/{ownerAccount}/apps/{appId}/text",
  "`POST /catalogue/{ownerAccount}/apps/{appId}/text`: one locale's text set or cleared by the maker or the owner.",
  |call, ground, reader, id, asked| {
    let text: CatalogueText = body_of(&mut call).await?;

    text
  } => catalogue::text(
    &ground,
    &reader,
    &id,
    asked.locale.trim(),
    asked.title.as_deref(),
    asked.description.as_deref(),
  )
  .await?
);

app_route!(
  TranslateCatalogueApp,
  Method::POST,
  "/catalogue/{ownerAccount}/apps/{appId}/translate",
  "`POST /catalogue/{ownerAccount}/apps/{appId}/translate`: every locale holding no text filled by the translator role.",
  |call, ground, reader, id, asked| {} => catalogue::fill_languages(&ground, &reader, &id, |language, text| {
    let ground = ground.clone();

    async move {
      crate::translate::translate(&ground, "document", language, (None, &[]), &text)
        .await
        .map(|done| done.text)
        .map_err(|refused| format!("{refused:?}"))
    }
  })
  .await?
);

/// `POST /catalogue/{ownerAccount}/seen`: the account's looking at what was shared with it kept.
pub struct SeenCatalogue;

impl Route for SeenCatalogue {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/catalogue/{ownerAccount}/seen";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let (account, company, ground) = asked_of(&call)?;
    let reader = catalogue::reader(&ground, &account, company).await?;
    let at = catalogue::seen(&ground, &reader).await?;

    Ok(answer(StatusCode::OK, json(&CatalogueSeen { at })))
  }
}

crate::moved!(SeenCatalogue);

/// `GET /catalogue-promotions`: every company's requests to make an app a platform app, for the administrator.
pub struct ListCataloguePromotions;

impl Route for ListCataloguePromotions {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/catalogue-promotions";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let acting = Acting::of(&call)?;
    let ground = call.shared_ground();

    if let Acting::Person(person) = acting {
      let administrator = crate::account_isolation::owners_own(&ground.db, &person.to_string())
        .await
        .map_err(|why| Refused::new(StatusCode::SERVICE_UNAVAILABLE, why.detail))?;

      if !administrator {
        return Err(
          Refused::new(
            StatusCode::FORBIDDEN,
            "The requests to make an app a platform app are the administrator's",
          )
          .with_code("forbidden"),
        );
      }
    }

    let items = catalogue::promotions(&ground).await?;
    let items = serde_json::from_value(items).unwrap_or_default();

    Ok(answer(StatusCode::OK, json(&CataloguePromotions { items })))
  }
}

crate::moved!(ListCataloguePromotions);
