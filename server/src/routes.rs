//! The service's routes (docs/accounts.md): sumida's own as sumida mounts them, and the deployment's own.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use sumida::api::Sumida;
use sumida::api::authz::Authz;

use crate::checks::{check_message, check_object, check_room};
use crate::data_move::{MoveAsk, run_move};
use crate::grants::{GrantAsk, RevokeAsk, grant, revoke};
use crate::receiver::Receiver;
use crate::rules::RULES;
use crate::users::user_subject;
use crate::{Refusal, Service, all, one, phones};

/// sumida's rules and the deployment's, read and held to the database: every table a rule reads stands,
/// and every relation they declare is written where a grant names it. A database the rules do not fit is
/// refused before anything is answered.
pub async fn stand(db: DatabaseConnection, receiver: Arc<dyn Receiver>) -> Result<Service, Refusal> {
  Authz::install_rules(RULES)?;
  Authz::boot_or_abort(&db).await?;
  Authz::sync_schema(&db).await?;

  Ok(Service { db, receiver })
}

/// Every route, with sumida's sessions on Valkey under them.
pub async fn in_force(service: Service) -> Result<Router, Refusal> {
  let settings = sumida::infra::settings::get();
  let store = sumida::infra::session::new_store(&settings.session)
    .await
    .map_err(|why| Refusal::new(StatusCode::SERVICE_UNAVAILABLE, format!("the session store: {why}")))?;
  // A session's idle window is extended at every request it is read on, so a console in use stays signed
  // in, as `docs/web.md` 「Access」 has it.
  let sessions = sumida::api::layer::session::new(store)
    .await
    .map_err(|why| Refusal::new(StatusCode::SERVICE_UNAVAILABLE, format!("the session layer: {why}")))?
    .with_always_save(true);

  Ok(
    Router::new()
      .merge(Sumida::new(RULES).set_state(&service).router())
      .merge(own())
      .layer(sessions)
      .with_state(service)
      .layer(settings.session.ip_source.clone().into_extension()),
  )
}

/// The routes while the model is not in force: the state, saying why, and a refusal naming it for
/// everything else.
pub fn not_in_force(reason: String) -> Router {
  let state = json!({ "inForce": false, "reason": reason });
  let refusal = json!({ "detail": format!("the model is not in force: {reason}") });

  Router::new()
    .route("/bmo/state", get(move || async move { Json(state) }))
    .fallback(move || async move { (StatusCode::SERVICE_UNAVAILABLE, Json(refusal)) })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoomAsk {
  room: i64,
  sender: Option<i64>,
  relation: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageAsk {
  room: i64,
  message: i64,
  relation: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObjectAsk {
  kind: String,
  object: String,
  relation: String,
  user: i32,
}

#[derive(Deserialize)]
struct UserAsk {
  user: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdoptAsk {
  token: String,
  user: i32,
}

#[derive(Deserialize)]
struct KeyAsk {
  key: String,
}

type Answer = Result<Json<Value>, Refusal>;

fn answered(value: impl serde::Serialize) -> Answer {
  serde_json::to_value(value)
    .map(Json)
    .map_err(|why| Refusal::new(StatusCode::INTERNAL_SERVER_ERROR, why.to_string()))
}

fn own() -> Router<Service> {
  Router::new()
    .route("/bmo/state", get(|| async { Json(json!({ "inForce": true })) }))
    .route("/bmo/check", post(room_check))
    .route("/bmo/check-message", post(message_check))
    .route("/bmo/check-object", post(object_check))
    .route("/bmo/grants", post(grant_route).delete(revoke_route))
    .route("/bmo/move", post(move_route))
    .route("/bmo/users/{user}", get(user_route))
    .route("/bmo/users/{user}/password", post(password_route))
    .route("/bmo/users/{user}/name", post(name_route))
    .route("/bmo/phones", post(phone_make))
    .route("/bmo/phones/adopt", post(phone_adopt))
    .route("/bmo/phones/who", post(phone_who))
    .route("/bmo/phones/{hash}", delete(phone_delete))
}

async fn room_check(State(service): State<Service>, Json(ask): Json<RoomAsk>) -> Answer {
  answered(check_room(&service.db, ask.room, ask.sender, &ask.relation).await?)
}

async fn message_check(State(service): State<Service>, Json(ask): Json<MessageAsk>) -> Answer {
  answered(
    check_message(&service.db, service.receiver.as_ref(), ask.room, ask.message, &ask.relation).await?,
  )
}

async fn object_check(State(service): State<Service>, Json(ask): Json<ObjectAsk>) -> Answer {
  answered(check_object(&service.db, &ask.kind, &ask.object, &ask.relation, ask.user).await?)
}

async fn grant_route(State(service): State<Service>, Json(ask): Json<GrantAsk>) -> Answer {
  answered(grant(&service.db, service.receiver.as_ref(), ask).await?)
}

async fn revoke_route(State(service): State<Service>, Json(ask): Json<RevokeAsk>) -> Answer {
  answered(revoke(&service.db, service.receiver.as_ref(), ask).await?)
}

async fn move_route(State(service): State<Service>, Json(ask): Json<MoveAsk>) -> Answer {
  answered(run_move(&service.db, ask).await?)
}

/// A user as the console reads them: their name, whether they are the platform administrator, and the
/// sender ids linked to them.
async fn user_route(State(service): State<Service>, Path(user): Path<i32>) -> Answer {
  let db = &service.db;
  let row = one(db, r#"SELECT "username" FROM "users" WHERE "id" = $1"#, vec![user.into()])
    .await?
    .ok_or_else(|| Refusal::absent(format!("no user {user}")))?;
  let name: String = row.try_get("", "username")?;
  let administrator = Authz::is_superuser(&user_subject(db, user).await?, db).await?;
  let senders = all(
    db,
    r#"SELECT "sender_id" FROM "user_senders" WHERE "user_id" = $1 ORDER BY "sender_id""#,
    vec![i64::from(user).into()],
  )
  .await?
  .iter()
  .map(|row| row.try_get::<i64>("", "sender_id"))
  .collect::<Result<Vec<_>, _>>()?;

  answered(json!({ "id": user, "name": name, "administrator": administrator, "senders": senders }))
}

#[derive(Deserialize)]
struct PasswordAsk {
  current: String,
  next: String,
}

#[derive(Deserialize)]
struct NameAsk {
  name: String,
}

async fn password_route(
  State(service): State<Service>,
  Path(user): Path<i32>,
  Json(ask): Json<PasswordAsk>,
) -> Answer {
  crate::users::change_password(&service.db, user, &ask.current, &ask.next).await?;

  answered(json!({}))
}

async fn name_route(
  State(service): State<Service>,
  Path(user): Path<i32>,
  Json(ask): Json<NameAsk>,
) -> Answer {
  crate::users::rename(&service.db, user, &ask.name).await?;

  answered(json!({}))
}

async fn phone_make(State(service): State<Service>, Json(ask): Json<UserAsk>) -> Answer {
  answered(phones::make(&service.db, ask.user).await?)
}

async fn phone_adopt(State(service): State<Service>, Json(ask): Json<AdoptAsk>) -> Answer {
  answered(json!({ "hash": phones::adopt(&service.db, &ask.token, ask.user).await? }))
}

async fn phone_who(State(service): State<Service>, Json(ask): Json<KeyAsk>) -> Answer {
  answered(phones::holder(&service.db, &ask.key).await?)
}

async fn phone_delete(State(service): State<Service>, Path(hash): Path<String>) -> Answer {
  answered(json!({ "deleted": phones::delete(&service.db, &hash).await? }))
}
