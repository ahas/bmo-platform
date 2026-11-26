//! The role slots' routes (docs/api.md, 「slots, reservations and places」; docs/role-slots.md): a slot of a
//! role asked, let in at the head of its role's line, bound to the run it launched and given back with what
//! it came to; the slots an owner reserves and gives back; every role's slots; and the
//! standing checkouts a branch's place is taken, laid, recorded and taken down in. The labels and keys
//! of a consumer's work arrive with each request, and agency reads none of its orders.

use std::path::Path;

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use bmo_chat::{ErrorCode, WireError};
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer, whole};
use crate::agent_roles;
use crate::now_ms;
use crate::order_run::dispatch::{
  self, Asked, COWORK, Seats, Wanted, acquire, bind, finish, held_reservations, release_reservation,
};
use crate::order_run::places::{self, Held};
use crate::places::dev;
use crate::watches::links::checkout_shadow;

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

// A place named in a path: a checkout path is one segment of it, percent-encoded.
fn place_named(call: &Call) -> String {
  percent_encoding::percent_decode_str(call.param("placeId").unwrap_or_default())
    .decode_utf8_lossy()
    .into_owned()
}

// A slot step that did not go through, in the line's own words.
fn refused(fault: dispatch::Fault) -> Refused {
  Refused::new(StatusCode::UNPROCESSABLE_ENTITY, fault)
}

fn text<'a>(body: &'a Value, field: &str) -> Result<&'a str, Refused> {
  body
    .get(field)
    .and_then(Value::as_str)
    .filter(|value| !value.is_empty())
    .ok_or_else(|| bad(format!("{field} (string) is required")))
}

fn optional<'a>(body: &'a Value, field: &str) -> Option<&'a str> {
  body.get(field).and_then(Value::as_str).filter(|value| !value.is_empty())
}

fn held(body: &Value) -> Vec<Held> {
  body
    .get("held")
    .and_then(Value::as_array)
    .map(|held| held.iter().filter_map(|one| serde_json::from_value(one.clone()).ok()).collect())
    .unwrap_or_default()
}

fn shown(value: impl serde::Serialize) -> Result<Value, Refused> {
  serde_json::to_value(value).map_err(Refused::failed)
}

/// The reason a role session's turn cut because its slot was given back ends with (docs/api.md, 「sessions」).
pub const RELEASED: &str = "released";

/// The role session a request is bound to, where it is bound to one.
fn bound_session(request: &dispatch::Request) -> Option<String> {
  request.session.clone().filter(|session| request.child_id.as_ref() == Some(session))
}

/// The role sessions a given-back slot was bound to, set out to be ended: a turn running in one cut first and
/// waited for, so nothing goes on running on a slot given back (docs/role-slots.md, 「The line」). The ending is
/// carried on after the caller is answered, since a turn's end is its program's event and may be long in
/// coming; the ids of the sessions set out to end.
fn end_bound(
  backend: Option<crate::herdr_backend::Shared>,
  sessions: Vec<String>,
  owner: &str,
) -> Vec<String> {
  let Some(backend) = backend else {
    return vec![];
  };
  let owner = owner.to_owned();
  let ending = sessions.clone();

  tokio::spawn(async move {
    for session in sessions {
      if let Err(fault) =
        backend.end_under_turns(&agent_session::SessionId(session.clone()), RELEASED).await
      {
        eprintln!("{owner}: its role session {session} was not ended: {}", fault.message);
      }
    }
  });

  ending
}

/// `POST /slot-requests`: a slot of a role asked under the source, owner and request key its caller names, at
/// the path its run will work at; its provider and model are the role's order's unless the caller fixes them.
pub struct SlotAsk;

impl Route for SlotAsk {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/slot-requests";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let source = optional(&body, "source").unwrap_or(COWORK);
    let asked_by = optional(&body, "askedBy");
    let role = match optional(&body, "role") {
      Some(role) => role,
      None => return Err(bad("role (string) is required")),
    };

    if !agent_roles::holds(call.store(), role).await? {
      return Err(
        Refused::new(StatusCode::UNPROCESSABLE_ENTITY, agent_roles::unknown(role))
          .with_code("unknown-role"),
      );
    }

    let fixed = match (optional(&body, "provider"), optional(&body, "model")) {
      (Some(provider), Some(model)) => {
        Some(dispatch::named_road(call.store(), role, provider, model).await.map_err(refused)?)
      }
      (None, Some(model)) => {
        let provider = dispatch::provider_for_model(call.ground(), role, model).await.map_err(refused)?;

        Some(dispatch::named_road(call.store(), role, &provider, model).await.map_err(refused)?)
      }
      (None, None) => dispatch::fixed_road(call.store(), role, asked_by).await.map_err(refused)?,
      (Some(_), None) => return Err(bad("A slot's road names its provider together with its model")),
    };
    let asked = Asked {
      request_key: text(&body, "requestKey")?.to_owned(),
      owner: text(&body, "owner")?.to_owned(),
      wanted: Wanted {
        role,
        path: optional(&body, "path"),
        fixed,
        agent: None,
      },
      yielding: body.get("yielding").and_then(Value::as_bool).unwrap_or(false),
      token: optional(&body, "token"),
      place_key: optional(&body, "placeKey"),
      session: optional(&body, "session"),
      title: optional(&body, "title"),
      asked_by,
      account: None,
    };
    let request = dispatch::enqueue(call.ground(), &asked, source, async |_, _| Ok(()))
      .await
      .map_err(refused)?;

    Ok(answer(StatusCode::CREATED, shown(request)?))
  }
}

crate::moved!(SlotAsk);

/// `PATCH /slot-requests/{token}`: a slot request asked for its admission, bound to the run it launched, or
/// given back.
pub struct SlotMove;

impl Route for SlotMove {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/slot-requests/{token}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let token = call.param("token").unwrap_or_default().to_owned();
    let held = dispatch::request(call.store(), &token).await.map_err(refused)?.ok_or_else(|| {
      Refused::new(StatusCode::NOT_FOUND, format!("No slot request holds the token {token}"))
    })?;

    match body.get("state").and_then(Value::as_str) {
      Some("bound") => {
        let child = text(&body, "childId")?;
        let agent = optional(&body, "agentName");
        let mut bound = bind(call.store(), call.events(), &held, child, agent).await.map_err(refused)?;

        // A child naming a role session agency holds binds the lease to that session, which it ends with.
        let holds_session = match call.sessions() {
          Some(sessions) => sessions.held_scope(&agent_session::SessionId(child.to_owned())).await.is_ok(),
          None => false,
        };

        if holds_session && bound.child_id.as_deref() == Some(child) {
          dispatch::bound_to_session(call.store(), &token).await.map_err(refused)?;
          bound.session = Some(child.to_owned());
        }

        Ok(answer(StatusCode::OK, shown(bound)?))
      }
      Some("finished") => {
        let outcome = body.get("outcome").and_then(Value::as_str).unwrap_or_default();
        let cancel = body.get("cancel").and_then(Value::as_bool).unwrap_or(false);
        let session = bound_session(&held).filter(|_| held.state == "active");
        let held = finish(call.store(), call.events(), &token, outcome, cancel).await.map_err(refused)?;

        // The role session the lease was bound to ends with it, a turn running in it cut first; the
        // ending is carried on after the answer.
        if held && let Some(session) = session {
          end_bound(call.sessions(), vec![session], &token);
        }

        // The card the slot held goes back with it; where the asking run's own card was lent, the answer
        // waits until that run's engine is back and says so (docs/own-llm.md, 「An engine the arbiter
        // stands」).
        let note = crate::cards::give_back_seat(call.ground(), &token).await.map_err(Refused::from)?;

        Ok(answer(
          StatusCode::OK,
          json!({ "held": held, "stoodDown": note.is_some(), "note": note }),
        ))
      }
      Some(other) => Err(bad(format!("{other} is no state a slot request is moved to"))),
      None => {
        // The demands of the caller's own an admission names: one granted the slot stands on, or the run's
        // demand the slot's own is asked within.
        let demand = |field: &str| match body.get(field) {
          None | Some(Value::Null) => Ok(None),
          Some(value) => value
            .as_i64()
            .filter(|id| *id > 0)
            .map(Some)
            .ok_or_else(|| bad(format!("{field} is the id of a demand the arbiter's master granted"))),
        };
        let ask = crate::cards::Ask {
          own: demand("demandId")?,
          within: demand("within")?,
          terms: crate::cards::terms(
            super::session_ops::card_rank(&body)?,
            super::session_ops::card_pool(&body)?,
          ),
        };
        let admitted = acquire(call.ground(), &held).await.map_err(refused)?;
        // A slot on a registration whose engine the arbiter stands is let out once its card is granted; while
        // its demand waits it answers none and the demand says why, and the caller asks again.
        let card = match admitted.as_ref().filter(|seat| seat.state == "active") {
          Some(seat) => crate::cards::seat_card(call.ground(), seat, ask).await.map_err(Refused::from)?,
          None => None,
        };
        let request = match &card {
          Some(card) if !card.granted() => Value::Null,
          _ => shown(admitted.clone().filter(|seat| seat.state == "active"))?,
        };
        // What the request is now, so a caller tells one still waiting from one given back by anyone.
        let now = dispatch::request(call.store(), &token).await.map_err(refused)?.unwrap_or(held);

        Ok(answer(
          StatusCode::OK,
          json!({
            "request": request,
            "demand": card,
            "state": now.state,
            "endedBy": now.ended_by,
            "outcome": if now.state == "released" || now.state == "cancelled" { now.outcome } else { None },
          }),
        ))
      }
    }
  }
}

crate::moved!(SlotMove);

/// `POST /reservations`: slots of a role held for an owner.
pub struct SlotReserve;

impl Route for SlotReserve {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/reservations";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let seats = Seats {
      reservation_key: text(&body, "reservationKey")?.to_owned(),
      owner: text(&body, "owner")?.to_owned(),
      source: text(&body, "source")?,
      role: text(&body, "role")?,
      count: body
        .get("count")
        .and_then(whole)
        .ok_or_else(|| bad("count (a whole number) is required"))?,
    };
    let reservation = dispatch::reserve(call.store(), call.events(), &seats).await.map_err(refused)?;
    let slots = dispatch::seats(call.store(), &reservation.token).await.map_err(refused)?;

    Ok(answer(
      StatusCode::CREATED,
      json!({ "reservation": shown(reservation)?, "requests": shown(slots)? }),
    ))
  }
}

crate::moved!(SlotReserve);

/// `PATCH /reservations/{token}`: a reservation given back.
pub struct SlotReservationRelease;

impl Route for SlotReservationRelease {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/reservations/{token}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let token = call.param("token").unwrap_or_default().to_owned();
    let outcome = body.get("outcome").and_then(Value::as_str).unwrap_or_default();
    let held = release_reservation(call.store(), call.events(), &token, outcome)
      .await
      .map_err(refused)?;

    Ok(answer(StatusCode::OK, json!({ "held": held })))
  }
}

crate::moved!(SlotReservationRelease);

/// `GET /reservations`: the reservations an owner holds, or every one held where none is named.
pub struct SlotReservations;

impl Route for SlotReservations {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/reservations";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let owner = call.query("owner");
    let held = held_reservations(call.store(), owner.as_deref()).await.map_err(refused)?;

    Ok(answer(StatusCode::OK, json!({ "items": shown(held)?, "more": false })))
  }
}

crate::moved!(SlotReservations);

// The request states that are still open: asked, reserved, or let in.
const OPEN: [&str; 3] = ["waiting", "reserved", "active"];

/// `GET /slot-requests/{token}`: the request a token holds, whatever its state.
pub struct SlotRequestRead;

impl Route for SlotRequestRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/slot-requests/{token}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let token = call.param("token").unwrap_or_default();

    Ok(match dispatch::request(call.store(), token).await.map_err(refused)? {
      Some(request) => answer(StatusCode::OK, shown(request)?),
      None => WireError::new(ErrorCode::NotFound, format!("No slot request holds the token {token}"))
        .into_response(),
    })
  }
}

crate::moved!(SlotRequestRead);

/// `DELETE /slot-requests`: an owner's release — every request, reservation and bound role session the owner
/// holds given back as its owner's (docs/role-slots.md, 「An owner's release」).
pub struct SlotOwnerRelease;

impl Route for SlotOwnerRelease {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/slot-requests";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let owner = call
      .query("owner")
      .map(|owner| owner.trim().to_owned())
      .filter(|owner| !owner.is_empty())
      .ok_or_else(|| bad("owner is required"))?;
    let done = dispatch::release_owner(call.store(), call.events(), &owner).await.map_err(refused)?;
    let ending = end_bound(call.sessions(), done.sessions.clone(), &owner);

    Ok(answer(
      StatusCode::OK,
      json!({
        "cancelled": done.cancelled,
        "released": done.released,
        "reservations": done.reservations,
        "sessionsEnded": ending,
      }),
    ))
  }
}

crate::moved!(SlotOwnerRelease);

/// `GET /slot-requests`: the requests still open, in the order they came, narrowed by role, by owner, or to
/// the ones waiting.
pub struct SlotOpenRequests;

impl Route for SlotOpenRequests {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/slot-requests";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let owner = call.query("owner");
    let role = call.query("role");
    let waiting = call.query("waiting").as_deref() == Some("true");
    let open: Vec<_> = dispatch::requests(call.store(), &OPEN)
      .await
      .map_err(refused)?
      .into_iter()
      .filter(|request| owner.as_ref().is_none_or(|owner| &request.owner == owner))
      .filter(|request| role.as_ref().is_none_or(|role| &request.role == role))
      .filter(|request| !waiting || request.state == "waiting")
      .collect();

    Ok(answer(StatusCode::OK, json!({ "items": shown(open)?, "more": false })))
  }
}

crate::moved!(SlotOpenRequests);

/// `GET /slots`: every role's slots — its cap, the runs let in, reserved and waiting, and what holds each
/// active one.
pub struct Slots;

impl Route for Slots {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/slots";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let open = dispatch::requests(call.store(), &OPEN).await.map_err(refused)?;
    let backend = call.sessions();
    let mut items = vec![];

    for role in agent_roles::every(call.store()).await? {
      let of_role: Vec<_> = open.iter().filter(|request| request.role == role.name).collect();
      let count = |state: &str| of_role.iter().filter(|request| request.state == state).count();
      let runs: Vec<Value> = of_role
        .iter()
        .filter(|request| request.state == "active")
        .map(|request| {
          let session = bound_session(request);
          // A slot on a role session names the run its session's turn goes out as while one runs; a slot on
          // anything else, the child it is bound to; a run name a binding named stands over either.
          let going = match (&session, &backend) {
            (Some(session), Some(backend)) => backend.turn_run(&agent_session::SessionId(session.clone())),
            (Some(_), None) => None,
            (None, _) => request.child_id.clone(),
          };

          json!({
            "token": request.token,
            "role": request.role,
            "path": request.path,
            "provider": request.provider,
            "model": request.model,
            "source": request.source,
            "since": request.leased_at,
            "agent": request.seat,
            "runName": request.agent_name.clone().or(going),
            "session": session,
            "jobId": dispatch::job_of(request),
            "title": request.title,
          })
        })
        .collect();

      items.push(json!({
        "role": role.name,
        "maxConcurrent": role.max_concurrent,
        "active": count("active"),
        "reserved": count("reserved"),
        "waiting": count("waiting"),
        "runs": runs,
      }));
    }

    Ok(answer(StatusCode::OK, json!({ "items": items })))
  }
}

crate::moved!(Slots);

/// A branch's place in its order's pool of standing checkouts, taken.
pub struct PlaceTake;

impl Route for PlaceTake {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/places";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let asked = places::Asked {
      cwd: text(&body, "cwd")?,
      selection: text(&body, "selection")?,
      role: text(&body, "role")?,
      effort: optional(&body, "effort"),
    };
    let slot = places::take_slot(call.ground(), &asked, &held(&body), now_ms())
      .await
      .map_err(refused)?;
    let cwd = places::slot_cwd(call.ground(), asked.cwd, slot).await;
    let path = cwd.to_string_lossy().into_owned();

    Ok(answer(
      StatusCode::CREATED,
      json!({ "id": path, "path": path, "nodeName": Value::Null, "slot": slot, "role": body.get("role").cloned(), "state": "taken", "lastWrite": Value::Null }),
    ))
  }
}

crate::moved!(PlaceTake);

/// The places the agency holds, by node and checkout.
pub struct PlaceList;

impl Route for PlaceList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/places";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let node = call.query("nodeName").filter(|node| !node.trim().is_empty());
    let checkout = call.query("checkout").filter(|checkout| !checkout.trim().is_empty());

    // A checkout on a node is held here as the shadow the node's link keeps of it.
    if let Some(node) = node.as_deref() {
      let Some(checkout) = checkout.as_deref() else {
        return Err(bad("checkout is required with nodeName"));
      };
      let path = checkout_shadow(&call.ground().roots.data, node, checkout)
        .to_string_lossy()
        .into_owned();

      return Ok(answer(
        StatusCode::OK,
        json!({ "items": [place_at(&path, Some(node), None, None, Value::Null)], "more": false }),
      ));
    }

    let pool = match checkout.as_deref() {
      Some(checkout) => Some(crate::places::branch::pool_of(checkout).await),
      None => None,
    };
    let items = held_places(call.ground(), pool.as_deref()).await?;

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": false })))
  }
}

crate::moved!(PlaceList);

/// One place by its key.
pub struct ReadPlace;

impl Route for ReadPlace {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/places/{placeId}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let place = place_named(&call);

    if let Some(one) = held_place(call.ground(), &place).await? {
      return Ok(answer(StatusCode::OK, one));
    }

    let row = store::one(
      call.store(),
      r#"SELECT "place", "provider", "model", "effort", "upstream", "written_at"
         FROM "place_writes" WHERE "place" = $1"#,
      vec![place.clone().into()],
    )
    .await?;

    let Some(row) = row else {
      return Err(Refused::new(StatusCode::NOT_FOUND, format!("no place is named {place}")));
    };

    Ok(answer(
      StatusCode::OK,
      json!({
        "id": place.clone(),
        "path": place,
        "nodeName": Value::Null,
        "slot": Value::Null,
        "role": Value::Null,
        "state": "remembered",
        "lastWrite": {
          "provider": row.try_get::<String>("", "provider")?,
          "model": row.try_get::<String>("", "model")?,
          "effort": row.try_get::<Option<String>>("", "effort")?,
          "upstream": row.try_get::<Option<String>>("", "upstream")?,
          "writtenAt": row.try_get::<i64>("", "written_at")?,
        },
      }),
    ))
  }
}

crate::moved!(ReadPlace);

/// One place as the boundary answers it.
fn place_at(
  path: &str,
  node: Option<&str>,
  slot: Option<i64>,
  role: Option<&str>,
  last_write: Value,
) -> Value {
  json!({
    "id": path,
    "path": path,
    "nodeName": node,
    "slot": slot,
    "role": role,
    "state": if last_write.is_null() { "prepared" } else { "remembered" },
    "lastWrite": last_write,
  })
}

// A standing place as its row and its write leave it: taken where a pool request holds it, remembered
// where a branch last answered in it, prepared where only its folder stands.
fn standing_place(
  path: &str,
  slot: i64,
  role: Option<&str>,
  row: Option<&(String, String, String, i64)>,
  taken: bool,
  write: Option<&Value>,
) -> Value {
  let last_write = write.cloned().or_else(|| {
    row.map(|(mode, model, upstream, at)| {
      json!({
        "provider": mode,
        "model": model,
        "effort": Value::Null,
        "upstream": if upstream.is_empty() { Value::Null } else { json!(upstream) },
        "writtenAt": at * 1000,
      })
    })
  });
  let mut place = place_at(path, None, Some(slot), role, last_write.unwrap_or(Value::Null));

  place["state"] = json!(if taken {
    "taken"
  } else if row.is_some() {
    "remembered"
  } else {
    "prepared"
  });

  place
}

// Every place the agency holds under the data root, by pool and slot, with the row that last answered
// in it and the write it last took.
async fn held_places(ground: &crate::places::Ground, only: Option<&str>) -> Result<Vec<Value>, Refused> {
  let data = &ground.roots.data;
  let rows = store::all(
    &ground.db,
    r#"SELECT "pool", "slot", "mode", "model", "role", "upstream", "last_reply_at"
       FROM "branch_agents" ORDER BY "last_reply_at" DESC"#,
    vec![],
  )
  .await?;
  let mut written: std::collections::HashMap<String, Value> = std::collections::HashMap::new();

  for row in store::all(
    &ground.db,
    r#"SELECT "place", "provider", "model", "effort", "upstream", "written_at"
       FROM "place_writes""#,
    vec![],
  )
  .await?
  {
    written.insert(
      row.try_get("", "place")?,
      json!({
        "provider": row.try_get::<String>("", "provider")?,
        "model": row.try_get::<String>("", "model")?,
        "effort": row.try_get::<Option<String>>("", "effort")?,
        "upstream": row.try_get::<Option<String>>("", "upstream")?,
        "writtenAt": row.try_get::<i64>("", "written_at")?,
      }),
    );
  }

  let mut taken = std::collections::HashSet::new();

  for row in store::all(
    &ground.db,
    r#"SELECT DISTINCT "place_key" FROM "role_slot_requests"
       WHERE "state" = 'active' AND "place_key" IS NOT NULL"#,
    vec![],
  )
  .await?
  {
    taken.insert(row.try_get::<String>("", "place_key")?);
  }

  let mut newest: std::collections::HashMap<(String, i64), (String, String, String, i64)> =
    std::collections::HashMap::new();
  let mut roles: std::collections::HashMap<(String, i64), String> = std::collections::HashMap::new();

  for row in &rows {
    let pool: String = row.try_get("", "pool")?;
    let slot: i64 = row.try_get("", "slot")?;
    let entry = (
      row.try_get::<String>("", "mode")?,
      row.try_get::<String>("", "model")?,
      row.try_get::<String>("", "upstream")?,
      row.try_get::<i64>("", "last_reply_at")?,
    );

    roles
      .entry((pool.clone(), slot))
      .or_insert_with(|| row.try_get("", "role").unwrap_or_default());
    newest.entry((pool, slot)).or_insert(entry);
  }

  let mut pools: Vec<String> = std::fs::read_dir(crate::places::branch::slot_root(data))
    .into_iter()
    .flatten()
    .flatten()
    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
    .filter_map(|entry| entry.file_name().into_string().ok())
    .collect();

  for (pool, _) in roles.keys() {
    if !pools.contains(pool) {
      pools.push(pool.clone());
    }
  }

  pools.sort();
  pools.retain(|pool| only.is_none_or(|only| only == pool));

  let mut items = vec![];

  for pool in pools {
    let mut slots = crate::places::branch::pool_slots(data, &pool);

    for (_, slot) in roles.keys().filter(|(held, _)| *held == pool) {
      if !slots.contains(slot) {
        slots.push(*slot);
      }
    }

    slots.sort_unstable();

    for slot in slots {
      let path = crate::places::branch::slot_dir(data, &pool, slot).to_string_lossy().into_owned();
      let row = newest.get(&(pool.clone(), slot));
      let role = roles.get(&(pool.clone(), slot)).map(String::as_str);

      items.push(standing_place(&path, slot, role, row, taken.contains(&path), written.get(&path)));
    }
  }

  Ok(items)
}

// One place by its path, where the path is one under the data root.
async fn held_place(ground: &crate::places::Ground, place: &str) -> Result<Option<Value>, Refused> {
  if let Some((pool, slot)) = crate::places::branch::place_of(&ground.roots.data, place) {
    let found = held_places(ground, Some(&pool))
      .await?
      .into_iter()
      .find(|one| one["slot"].as_i64() == Some(slot));

    return Ok(found);
  }

  // A checkout on a node is held as the shadow under the node's own directory.
  let shadow = std::path::Path::new(&ground.roots.data).join("nodes");
  let within = std::path::Path::new(place).strip_prefix(&shadow).ok();
  let mut parts = within.into_iter().flat_map(|rest| rest.components());
  let node = parts.next().and_then(|node| node.as_os_str().to_str()).map(str::to_owned);
  let kind = parts.next().and_then(|kind| kind.as_os_str().to_str()).map(str::to_owned);

  Ok(match (node, kind.as_deref()) {
    (Some(node), Some("shadow")) => Some(place_at(place, Some(&node), None, None, Value::Null)),
    _ => None,
  })
}

/// A place laid, remembered or retired, or its last write recorded.
pub struct ChangePlace;

impl Route for ChangePlace {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/places/{placeId}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let place = place_named(&call);
    let body = call.body().await?;

    match body.get("state").and_then(Value::as_str) {
      Some("prepared") => {
        places::prepare_slot(text(&body, "source")?, Path::new(text(&body, "cwd")?))
          .await
          .map_err(refused)?;
      }
      Some("remembered") => {
        let slot = body
          .get("slot")
          .and_then(whole)
          .ok_or_else(|| bad("slot (a whole number) is required"))?;
        places::remember_slot(
          call.ground(),
          text(&body, "cwd")?,
          slot,
          text(&body, "role")?,
          optional(&body, "mode"),
          optional(&body, "model"),
          optional(&body, "effort"),
          optional(&body, "upstream"),
        )
        .await
        .map_err(refused)?;
      }
      Some("retired") => {
        let used: Vec<i64> = body
          .get("used")
          .and_then(Value::as_array)
          .map(|used| used.iter().filter_map(whole).collect())
          .unwrap_or_default();
        places::retire_places(call.ground(), text(&body, "cwd")?, &used, &held(&body)).await;
      }
      _ => {
        super::pool::place_written(
          call.store(),
          &place,
          text(&body, "provider")?,
          text(&body, "model")?,
          optional(&body, "effort"),
          optional(&body, "upstream"),
        )
        .await?;
      }
    }

    let state = body.get("state").and_then(Value::as_str).unwrap_or("taken");

    Ok(answer(
      StatusCode::OK,
      json!({ "id": place, "path": place, "nodeName": Value::Null, "slot": Value::Null, "role": Value::Null, "state": state, "lastWrite": body.get("lastWrite").cloned().unwrap_or(Value::Null) }),
    ))
  }
}

crate::moved!(ChangePlace);

/// A worker's workbench folder, with every item's scratch folder standing under it now, in item order.
pub struct Workbench;

impl Route for Workbench {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/repositories/{repositoryName}/workbench";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let worker = call
      .param("repositoryName")
      .filter(|worker| !worker.is_empty())
      .ok_or_else(|| bad("a worker is required"))?;
    let root = dev::workbench(call.ground(), worker);
    let mut scratches: Vec<(i64, String)> = std::fs::read_dir(dev::item_scratches(call.ground(), worker))
      .into_iter()
      .flatten()
      .flatten()
      .filter(|entry| entry.path().is_dir())
      .filter_map(|entry| {
        let item = entry.file_name().to_str()?.parse::<i64>().ok()?;

        Some((item, entry.path().to_string_lossy().into_owned()))
      })
      .collect();

    scratches.sort();

    let scratches: Vec<String> = scratches.into_iter().map(|(_, path)| path).collect();

    Ok(answer(
      StatusCode::OK,
      json!({ "root": root.to_string_lossy(), "scratches": scratches }),
    ))
  }
}

crate::moved!(Workbench);
