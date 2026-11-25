//! The routes one BMO uses to hand a line to another person's BMO and to open a chat into that person's
//! room (docs/relays.md): the sender's own turn is the line's source, the receiving BMO's session carries
//! it, and the record answers both sides who sent what.

use axum::http::{Method, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::relays::{self, HANDED, OPENED, Verdict};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ask {
  from: String,
  to: String,
  text: String,
  #[serde(default)]
  room: Option<i64>,
  #[serde(default)]
  person: Option<i64>,
}

// What a body names, once every field has been judged: the sender, the receiver, the room and the line.
struct Line {
  from: String,
  to: String,
  room: i64,
  text: String,
}

fn bad(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error)
}

fn named(value: &str, field: &str) -> Result<String, Refused> {
  let value = value.trim();

  if value.is_empty() {
    return Err(bad(format!("{field} (string) is required")).with_field(field));
  }

  Ok(value.to_owned())
}

// The body as a line to hand over, every part judged before anything is delivered.
async fn line_of(db: &sea_orm::DatabaseConnection, ask: Ask, kind: &str) -> Result<Line, Refused> {
  let from = named(&ask.from, "from")?;
  let to = named(&ask.to, "to")?;
  let text = ask.text.trim().to_owned();

  if text.is_empty() {
    return Err(bad("text (string) is required").with_field("text"));
  }

  if from == to {
    return Err(
      Refused::new(StatusCode::CONFLICT, "A BMO does not relay to itself").with_code("relay-to-self"),
    );
  }

  let sender = crate::supervisors::named(db, &from).await?;

  if sender.is_none() {
    return Err(
      Refused::new(StatusCode::UNPROCESSABLE_ENTITY, format!("No supervisor named {from} is held"))
        .with_code("unknown-sender"),
    );
  }

  // Two supervisors of one person meet in a room, never through a relay: the line is said there, by the
  // name that wakes the one addressed, and its answer is a reply under it.
  if let (Some(sender), Some(target)) = (&sender, crate::supervisors::named(db, &to).await?)
    && sender.owner_account == target.owner_account
  {
    return Err(
      Refused::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!(
          "{to} is another supervisor of the same person: say the line in a room it answers, by the name that wakes it"
        ),
      )
      .with_code("relay-to-supervisor"),
    );
  }

  relays::receiver_settings(db, &to).await?;

  if let Some(person) = ask.person
    && crate::supervisors::named(db, &to).await?.and_then(|row| row.owner_account) != Some(person)
  {
    return Err(
      Refused::new(StatusCode::UNPROCESSABLE_ENTITY, format!("{to} is not the BMO of the person named"))
        .with_code("person-mismatch"),
    );
  }

  let room = ask.room.ok_or_else(|| {
    bad(format!("room (a whole number) is required: it is the room the {kind} is said in"))
      .with_field("room")
  })?;

  Ok(Line {
    from,
    to,
    room,
    text,
  })
}

// One line handed over or a chat opened, recorded whichever way it came out.
async fn carry(call: Call, ask: Ask, kind: &str) -> Result<Response, Refused> {
  let line = line_of(call.store(), ask, kind).await?;
  let settings = match relays::receiver_settings(call.store(), &line.to).await {
    Ok(settings) => settings,
    Err(refused) => return Err(refused),
  };
  let sessions = call.sessions().ok_or_else(|| {
    Refused::new(StatusCode::SERVICE_UNAVAILABLE, "agency stands no places to carry sessions on")
      .with_code("no-places")
  })?;
  let said = relays::prompt(&line.from, &line.text);
  let at = crate::now_ms();

  match relays::deliver(sessions.as_ref(), &line.to, &settings, line.room, said).await {
    Ok(end) => {
      let verdict = relays::verdict(&end);
      let record = relays::record(
        call.store(),
        &line.from,
        &line.to,
        line.room,
        kind,
        &line.text,
        relays::state_of(&verdict),
        end.answer.as_deref(),
        match &verdict {
          Verdict::Failed(reason) => Some(reason.as_str()),
          _ => None,
        },
        at,
      )
      .await?;

      match verdict {
        Verdict::Delivered => {
          Ok(answer(StatusCode::CREATED, json!({ "relay": record, "answer": end.answer })))
        }
        Verdict::Refused => Err(
          Refused::new(
            StatusCode::CONFLICT,
            format!("{} refused the line: the receiving BMO would not take it", line.to),
          )
          .with_code("relay-refused"),
        ),
        Verdict::Failed(reason) => Err(
          Refused::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{} did not take the line: {reason}", line.to),
          )
          .with_code("relay-failed"),
        ),
      }
    }
    Err(fault) => {
      let busy = fault.kind == agent_session::FaultKind::Busy;
      let reason = fault.message.clone();
      let _ = relays::record(
        call.store(),
        &line.from,
        &line.to,
        line.room,
        kind,
        &line.text,
        "failed",
        None,
        Some(&reason),
        at,
      )
      .await;

      let (status, code) = if busy {
        (StatusCode::CONFLICT, "relay-busy")
      } else {
        (StatusCode::UNPROCESSABLE_ENTITY, "relay-failed")
      };

      Err(Refused::new(status, format!("{} did not take the line: {reason}", line.to)).with_code(code))
    }
  }
}

/// `POST /relays`: hand a line to another person's BMO (docs/relays.md).
pub struct Relays;

impl Route for Relays {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/relays";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let ask: Ask = serde_json::from_value(call.body().await?)
      .map_err(|why| bad(format!("the relay does not read: {why}")))?;

    carry(call, ask, HANDED).await
  }
}

crate::moved!(Relays);

/// `POST /chats`: open a chat into another person's room and hand it a line (docs/relays.md).
pub struct Chats;

impl Route for Chats {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/chats";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let ask: Ask = serde_json::from_value(call.body().await?)
      .map_err(|why| bad(format!("the chat does not read: {why}")))?;

    carry(call, ask, OPENED).await
  }
}

crate::moved!(Chats);

/// `GET /relays`: the relays one BMO sent or received, the latest first (docs/relays.md).
pub struct RelayList;

impl Route for RelayList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/relays";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let agent = call
      .query("agent")
      .map(|agent| agent.trim().to_owned())
      .filter(|agent| !agent.is_empty())
      .ok_or_else(|| bad("agent (a supervisor's name) is required").with_field("agent"))?;
    let limit = call
      .query("limit")
      .and_then(|limit| limit.trim().parse::<u64>().ok())
      .unwrap_or(50)
      .clamp(1, 200);
    let items: Vec<Value> = relays::list(call.store(), &agent, limit)
      .await?
      .into_iter()
      .map(|relay| serde_json::to_value(relay).unwrap_or(Value::Null))
      .collect();

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": false })))
  }
}

crate::moved!(RelayList);
