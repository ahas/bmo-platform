//! A consumer's hooks registered across the boundary (docs/api.md, 「Hooks」): at a point named, an address
//! agency posts each ask to, under a token where one is given; taken away again, the point answers its
//! default. A point that outlives a restart keeps its registration in the store and its token in a file. The
//! points a consumer may register at are listed with whether one is registered.

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::hooks::{Remote, wired, wired_named};

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

fn point_named(name: &str) -> Result<&'static crate::hooks::Wired, Refused> {
  wired_named(name)
    .ok_or_else(|| Refused::new(StatusCode::NOT_FOUND, format!("no hook point is named {name}")))
}

/// A consumer's address registered at a point.
pub struct HookRegister;

impl Route for HookRegister {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/hooks/{point}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let name = call.param("point").unwrap_or_default().to_owned();
    let point = point_named(&name)?;
    let url = body.get("url").and_then(Value::as_str).ok_or_else(|| bad("url (string) is required"))?;

    reqwest::Url::parse(url).map_err(|why| bad(format!("{url} is not an address: {why}")))?;

    let token = body.get("token").and_then(Value::as_str).filter(|token| !token.is_empty());

    let remote = Remote {
      url: url.to_owned(),
      token: token.map(str::to_owned),
    };

    if point.kept {
      let ground = call.ground();

      crate::hooks::keep(&ground.db, &ground.roots.data, point.name, &remote).await?;
    }

    (point.register)(&call.ground().hooks, remote);

    Ok(answer(StatusCode::OK, json!({ "point": point.name, "url": url, "token": token })))
  }
}

crate::moved!(HookRegister);

/// A point's registration taken away.
pub struct HookRemove;

impl Route for HookRemove {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/hooks/{point}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("point").unwrap_or_default();
    let point = point_named(name)?;

    if point.kept {
      let ground = call.ground();

      crate::hooks::forget(&ground.db, &ground.roots.data, point.name).await?;
    }

    (point.unregister)(&call.ground().hooks);

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(HookRemove);

/// The points a consumer may register at.
pub struct HookPoints;

impl Route for HookPoints {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/hooks";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let mut points: Vec<Value> = wired()
      .map(|point| {
        json!({
          "point": point.name,
          "url": if (point.registered)(&call.ground().hooks) { "registered" } else { "" },
          "token": Value::Null,
        })
      })
      .collect();

    points.sort_by(|a, b| a["point"].as_str().cmp(&b["point"].as_str()));

    Ok(answer(StatusCode::OK, json!({ "items": points, "more": false })))
  }
}

crate::moved!(HookPoints);
