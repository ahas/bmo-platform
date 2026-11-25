//! A loopback stand-in a test points a process at, a listener a body is posted to, which records every
//! request it hears and answers each `{ "ok": true }`.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tokio::net::TcpListener;

/// The token a caller of the stand-in presents.
pub const TOKEN: &str = "fixture";

/// A loopback stand-in: each request it hears with the token, in order, by its path and body.
#[derive(Clone, Default)]
pub struct StandIn {
  asked: Arc<Mutex<Vec<(String, Value)>>>,
}

impl StandIn {
  /// Each request heard, by its path and body.
  pub fn heard(&self) -> Vec<(String, Value)> {
    self.asked.lock().unwrap().clone()
  }

  /// The stand-in listening on a port of its own; the port.
  pub async fn stand(&self) -> u16 {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = Router::new().fallback(heard).with_state(self.clone());

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    port
  }
}

async fn heard(State(stand_in): State<StandIn>, request: Request) -> Response {
  let path = request.uri().path().to_owned();
  let authorised =
    request.headers().get("authorization").and_then(|value| value.to_str().ok()) == Some("Bearer fixture");
  let body: Value =
    serde_json::from_slice(&request.into_body().collect().await.unwrap().to_bytes()).unwrap_or(Value::Null);

  if !authorised {
    return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "unauthorized" }))).into_response();
  }

  stand_in.asked.lock().unwrap().push((path.clone(), body));

  Json(json!({ "ok": true, "heard": path })).into_response()
}
