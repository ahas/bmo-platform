//! The gate kinds the gate declares (docs/api.md, 「Gate kinds」): the order it publishes, kept in the
//! store so agency's agent switches and an agent's own reasoning read the gate's declaration rather
//! than a copy of the gate's policy. `PUT /gate-kinds` is where the gate registers it, and a start
//! reads `GET /gates` so a deployment whose gate stands needs no push of its own.

use std::collections::HashSet;

use axum::http::{Method, StatusCode};
use axum::response::Response;
use bmo_gate_client::Gate as Declared;
use sea_orm::{ConnectionTrait, TransactionTrait};
use serde_json::{Value, json};
use store::{all, run};

use crate::Refusal;
use crate::control::{Call, Refused, Route, answer};
use crate::places::Ground;

/// One gate kind as the store keeps it: the key, where it stands, and the step it stands on there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kind {
  pub key: String,
  /// `pre`, `post` or `apart`.
  pub part: String,
  pub step: i64,
  pub condition: Option<String>,
}

/// Every gate kind the gate declared, in the order it stands.
pub async fn declared(db: &impl ConnectionTrait) -> Result<Vec<Kind>, Refusal> {
  let rows = all(
    db,
    r#"SELECT "key", "part", "step", "condition" FROM "gate_kinds"
       ORDER BY CASE "part" WHEN 'pre' THEN 0 WHEN 'post' THEN 1 ELSE 2 END, "step", "position""#,
    vec![],
  )
  .await?;

  Ok(
    rows
      .iter()
      .map(|row| {
        Ok(Kind {
          key: row.try_get("", "key")?,
          part: row.try_get("", "part")?,
          step: row.try_get("", "step")?,
          condition: row.try_get("", "condition")?,
        })
      })
      .collect::<Result<Vec<_>, store::Fault>>()?,
  )
}

/// Whether a name is one of the gate kinds the gate declared.
pub async fn holds(db: &impl ConnectionTrait, name: &str) -> Result<bool, Refusal> {
  Ok(
    store::one(db, r#"SELECT "key" FROM "gate_kinds" WHERE "key" = $1"#, vec![name.into()])
      .await?
      .is_some(),
  )
}

/// The chat turn's two parts as the gate declared them: its pre-gates and post-gates, each step holding
/// the gates that start together. The off-turn gates are no part of a turn.
pub fn turn(kinds: &[Kind]) -> policy::gates::TurnGates {
  let mut pre: Vec<(i64, Vec<policy::gates::Place>)> = Vec::new();
  let mut post: Vec<(i64, Vec<policy::gates::Place>)> = Vec::new();

  for kind in kinds {
    let steps = match kind.part.as_str() {
      "pre" => &mut pre,
      "post" => &mut post,
      _ => continue,
    };
    let place = policy::gates::Place {
      key: kind.key.clone(),
      condition: kind.condition.clone(),
      reference: false,
    };

    match steps.last_mut() {
      Some(step) if step.0 == kind.step => step.1.push(place),
      _ => steps.push((kind.step, vec![place])),
    }
  }

  let steps = |steps: Vec<(i64, Vec<policy::gates::Place>)>| {
    steps.into_iter().map(|(_, places)| policy::gates::Step { places }).collect()
  };

  policy::gates::TurnGates {
    pre: steps(pre),
    post: steps(post),
  }
}

/// The gate's declaration written over whatever stood: every kind the gate names, and none it does not.
pub async fn store(db: &sea_orm::DatabaseConnection, gates: &[Declared]) -> Result<(), Refusal> {
  let tx = db.begin().await.map_err(store::Fault::from)?;
  // The connection's own read of the clock, so a row's age is the store's and not this process's.
  let now = crate::now_ms();

  run(&tx, r#"DELETE FROM "gate_kinds""#, vec![]).await?;

  let mut seen = HashSet::new();

  for (position, gate) in gates.iter().enumerate() {
    if !seen.insert((gate.gate_name.as_str(), gate.part.as_str(), gate.step)) {
      continue;
    }

    run(
      &tx,
      r#"INSERT INTO "gate_kinds"
           ("key", "part", "step", "position", "condition", "switchable",
            "answer_tokens", "declared_at")
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
      vec![
        gate.gate_name.clone().into(),
        gate.part.clone().into(),
        gate.step.into(),
        i64::try_from(position).unwrap_or(i64::MAX).into(),
        gate.condition.clone().into(),
        if gate.switchable { 1 } else { 0 }.into(),
        gate.answer_tokens.into(),
        now.into(),
      ],
    )
    .await?;
  }

  tx.commit().await.map_err(store::Fault::from)?;

  Ok(())
}

/// The gate's declaration read afresh, kept where the agent switches and the reasoning read it. An empty
/// order the gate answers is kept as one; a gate that is undeployed or does not answer leaves what the
/// store holds as it stands.
pub async fn refresh(ground: &Ground) {
  match ground.peers.gate.kinds().await {
    Ok(Some(gates)) => {
      if let Err(why) = store(&ground.db, &gates).await {
        eprintln!("gate kinds: the declaration did not read: {}", why.detail);
      }
    }
    Ok(None) => {}
    Err(why) => eprintln!("gate kinds: the gate did not answer: {why}"),
  }
}

fn row(kind: &Kind) -> Value {
  json!({ "key": kind.key, "part": kind.part, "step": kind.step, "condition": kind.condition })
}

fn page(items: Vec<Value>) -> Response {
  answer(StatusCode::OK, json!({ "items": items, "more": false }))
}

/// Every gate kind the gate declared, as the console reads them.
pub struct GateKinds;

impl Route for GateKinds {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/gate-kinds";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let kinds = declared(call.store()).await?;

    Ok(page(kinds.iter().map(row).collect()))
  }
}

/// The gate's declaration of its kinds, written over the one standing. A declaration of no kinds is
/// refused: a gate that declares none is a gate that did not answer.
pub struct RegisterGateKinds;

impl Route for RegisterGateKinds {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/gate-kinds";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let gates: Vec<Declared> = serde_json::from_value(body.get("gates").cloned().unwrap_or(Value::Null))
      .map_err(|why| {
        Refused::new(StatusCode::BAD_REQUEST, format!("gates: {why}")).with_code("bad-request")
      })?;

    if gates.is_empty() {
      return Err(
        Refused::new(StatusCode::BAD_REQUEST, "a declaration names its gates").with_code("bad-request"),
      );
    }

    let wrong = gates.iter().find(|gate| !["pre", "post", "apart"].contains(&gate.part.as_str()));

    if let Some(gate) = wrong {
      return Err(
        Refused::new(
          StatusCode::BAD_REQUEST,
          format!("{} stands in no turn part: pre, post or apart", gate.part),
        )
        .with_code("bad-request"),
      );
    }

    store(call.store(), &gates).await?;

    let kinds = declared(call.store()).await?;

    Ok(page(kinds.iter().map(row).collect()))
  }
}

crate::moved!(GateKinds);
crate::moved!(RegisterGateKinds);
