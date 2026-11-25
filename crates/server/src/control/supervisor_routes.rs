//! The supervisors' routes (docs/api.md, 「supervisors」; docs/supervisors.md): every supervisor read, one
//! registered on its one road and given its brain, changed — its label, call names, road, warming, threshold,
//! repository, gates, sampling and voice — its road judged for a turn now, and its registration ended with
//! everything of its own but its brain where the caller keeps it.

use std::collections::BTreeMap;

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use bmo_agency_client::BrainBirth;
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use policy::roles::ROLES;
use policy::supervisors::SUPERVISORS;
use regex::Regex;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};
use std::sync::LazyLock;
use store::{one, run};

use super::agent_sampling;
use super::claude_accounts::Judging;
use super::sessions::asked_by;
use super::supervisor_removal::{Left, remove};
use super::supervisor_reset::{reset_whole, rooms_named, to_its_end};
use super::{Call, Refused, Route, answer, set_setting, setting};
use crate::access::Acting;
use crate::conversational::Settings as Conversation;
use crate::now_ms;
use crate::order::{self, Entry};
use crate::peers::brain::{Brain, Made};
use crate::places::Ground;
use crate::places::locks::{agent_lock_path, held};
use crate::queue::settings;
use crate::road::{self, Ask, Row};
use crate::supervisors::{self, Supervisor};

static NAME: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(&SUPERVISORS.name_pattern).expect("the supervisor name pattern reads"));

// The line every supervisor's turns go out on.
const CHAT_LINE: &str = "chat";

fn bad(field: &str, error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error).with_field(field)
}

fn not_found(name: &str) -> Refused {
  Refused::new(StatusCode::NOT_FOUND, format!("There is no supervisor {name}."))
}

/// The gates a supervisor's turn passes, once each with the first part it stands in, with its own switch, on
/// unless it holds off.
async fn gates_view(db: &DatabaseConnection, name: &str) -> Result<Value, Refused> {
  let turn = crate::gate_kinds::turn(&crate::gate_kinds::declared(db).await?);
  let mut views = vec![];

  for (part, key) in turn.gates() {
    let on = setting(db, &format!("agent_gate:{name}:{key}")).await?.as_deref() != Some("off");

    views.push(json!({ "part": part, "key": key, "on": on }));
  }

  Ok(json!(views))
}

/// One supervisor as the routes answer it: every field of its row, its one road, its gates, sampling and voice.
pub async fn view(ground: &Ground, supervisor: &Supervisor) -> Result<Value, Refused> {
  let db = &ground.db;
  let sampling = agent_sampling::view(db, &supervisor.name).await.map(|view| view["values"].clone()).ok();
  let road = supervisor.road.as_ref();

  Ok(json!({
    "name": supervisor.name,
    "label": supervisor.label,
    "ownerAccount": supervisor.owner_account,
    "aliases": supervisor.aliases,
    "icon": ROLES.icon("supervisor"),
    "sessionScope": supervisor.session_scope,
    "turnShape": supervisor.turn_shape,
    "brainFolder": supervisor.brain_folder,
    "repository": supervisor.repository,
    "provider": road.map(|road| &road.provider),
    "model": road.map(|road| &road.model),
    "effort": road.and_then(|road| road.effort.as_ref()),
    "thinking": road.and_then(|road| road.thinking.as_ref()),
    "answerTokens": road.and_then(|road| road.answer_tokens),
    "advisor": road.and_then(|road| road.advisor.as_ref()),
    "warming": supervisor.warming,
    "foldRatio": supervisor.fold_ratio,
    "cardRank": supervisor.card_rank,
    "gates": gates_view(db, &supervisor.name).await?,
    "voice": supervisor.voice,
    "sampling": sampling,
    "seeded": supervisor.builtin,
    "resetPoint": crate::supervisor_turn::overtaken::point(db, &supervisor.name).await,
  }))
}

/// `GET /supervisors`: every supervisor.
pub struct SupervisorList;

impl Route for SupervisorList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/supervisors";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let mut items = vec![];

    for supervisor in supervisors::every(call.store()).await? {
      items.push(view(call.ground(), &supervisor).await?);
    }

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": false })))
  }
}

crate::moved!(SupervisorList);

/// `GET /supervisors/{supervisorName}`: one supervisor.
pub struct SupervisorRead;

impl Route for SupervisorRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/supervisors/{supervisorName}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("supervisorName").unwrap_or_default().to_owned();
    let supervisor = supervisors::named(call.store(), &name).await?.ok_or_else(|| not_found(&name))?;

    Ok(answer(StatusCode::OK, view(call.ground(), &supervisor).await?))
  }
}

crate::moved!(SupervisorRead);

/// `GET /supervisors/{supervisorName}/reset-point`: a supervisor's reset point alone, read from the store the
/// reset writes it to.
pub struct SupervisorResetPoint;

impl Route for SupervisorResetPoint {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/supervisors/{supervisorName}/reset-point";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("supervisorName").unwrap_or_default().to_owned();

    if supervisors::named(call.store(), &name).await?.is_none() {
      return Err(not_found(&name));
    }

    let point = crate::supervisor_turn::overtaken::point(call.store(), &name).await;

    Ok(answer(StatusCode::OK, json!({ "supervisor": name, "resetPoint": point })))
  }
}

crate::moved!(SupervisorResetPoint);

/// `GET /supervisors/{supervisorName}/road`: the supervisor's one road and whether a turn would go out on it now,
/// judged as a turn's start judges it, with the sentence a turn would fail with where it cannot
/// (docs/provider-order.md, 「Whether a road can answer」).
pub struct SupervisorRoad;

impl Route for SupervisorRoad {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/supervisors/{supervisorName}/road";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("supervisorName").unwrap_or_default().to_owned();
    let supervisor = supervisors::named(call.store(), &name).await?.ok_or_else(|| not_found(&name))?;
    let order = supervisor.order();
    let resolution = road::resolve(call.ground(), &Ask::of(Row::Supervisor(&name), CHAT_LINE, &order))
      .await
      .map_err(Refused::failed)?;
    let answers = resolution.road().is_some();
    let road = supervisor.road.as_ref();

    Ok(answer(
      StatusCode::OK,
      json!({
        "supervisor": name,
        "provider": road.map(|road| &road.provider),
        "model": road.map(|road| &road.model),
        "effort": road.and_then(|road| road.effort.as_ref()),
        "thinking": road.and_then(|road| road.thinking.as_ref()),
        "answers": answers,
        "reason": (!answers).then(|| resolution.sentence()),
      }),
    ))
  }
}

crate::moved!(SupervisorRoad);

/// `GET /supervisors/{supervisorName}/rooms`: every room the supervisor's turns went into, as the run ledger
/// records them, each with its latest run there, the latest first.
pub struct SupervisorRooms;

impl Route for SupervisorRooms {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/supervisors/{supervisorName}/rooms";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("supervisorName").unwrap_or_default().to_owned();

    if supervisors::named(call.store(), &name).await?.is_none() {
      return Err(not_found(&name));
    }

    let rows = store::all(
      call.store(),
      r#"SELECT "r"."chat_id", "counted"."runs", "r"."id", "r"."unit", "r"."started_at", "r"."finished_at"
         FROM (SELECT "chat_id", COUNT(*) AS "runs", MAX("id") AS "last" FROM "runs"
               WHERE "agent" = $1 AND "chat_id" IS NOT NULL GROUP BY "chat_id") AS "counted"
         JOIN "runs" AS "r" ON "r"."id" = "counted"."last"
         ORDER BY "r"."started_at" DESC, "r"."id" DESC"#,
      vec![name.into()],
    )
    .await?;
    let mut items = vec![];

    for row in &rows {
      items.push(json!({
        "roomId": row.try_get::<i64>("", "chat_id")?,
        "runs": row.try_get::<i64>("", "runs")?,
        "lastRunId": row.try_get::<i64>("", "id")?,
        "lastRunUnit": row.try_get::<String>("", "unit")?,
        "lastStartedAt": row.try_get::<i64>("", "started_at")?,
        "lastFinishedAt": row.try_get::<Option<i64>>("", "finished_at")?,
      }));
    }

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": false })))
  }
}

crate::moved!(SupervisorRooms);

// The fields a supervisor's body may no longer name: a supervisor holds no order and nothing is forced over its
// road (docs/api.md, 「supervisors」).
const RETIRED: [&str; 3] = ["order", "quotaShare", "active"];

fn retired(body: &Value) -> Result<(), Refused> {
  match RETIRED.iter().find(|field| body.get(**field).is_some()) {
    Some(field) => Err(bad(field, "A supervisor stands on one road: name its provider and model.")),
    None => Ok(()),
  }
}

// The road a body names over the one `held`, judged against the chat line: its provider and model, each kept
// where the body names it not, and its effort, thinking, answer ceiling and advisor, kept only while the provider
// and model stay; none where the body names no field of a road. An advisor refused is refused under its own field.
async fn road_of(ground: &Ground, body: &Value, held: Option<&Entry>) -> Result<Option<Entry>, Refused> {
  const FIELDS: [&str; 6] = ["provider", "model", "effort", "thinking", "answerTokens", "advisor"];

  if !FIELDS.iter().any(|field| body.get(*field).is_some()) {
    return Ok(None);
  }

  let text = |field: &str| body.get(field).and_then(Value::as_str).map(str::trim).map(str::to_owned);
  let provider = text("provider").or_else(|| held.map(|road| road.provider.clone())).unwrap_or_default();
  let model = text("model").or_else(|| held.map(|road| road.model.clone())).unwrap_or_default();

  if provider.is_empty() || model.is_empty() {
    return Err(bad("provider", "A supervisor's road names its provider and its model."));
  }

  let kept = held.filter(|road| road.provider == provider && road.model == model);
  let named = |field: &str| body.get(field).cloned();
  let entry = json!([{
    "provider": provider,
    "model": model,
    "effort": named("effort").unwrap_or_else(|| json!(kept.and_then(|road| road.effort.clone()))),
    "thinking": named("thinking").unwrap_or_else(|| json!(kept.and_then(|road| road.thinking.clone()))),
    "answerTokens": named("answerTokens").unwrap_or_else(|| json!(kept.and_then(|road| road.answer_tokens))),
    "advisor": advisor_of(body, kept),
  }]);
  let judged =
    order::judge(Judging::of(ground), &ground.machine(), CHAT_LINE, &entry, order::Empty::Refused)
      .await
      .map_err(|refusal| {
        let field = if refusal.field == order::ADVISOR { order::ADVISOR } else { "provider" };

        bad(field, refusal.error)
      })?;

  Ok(judged.entries.into_iter().next())
}

// The advisor a body names over the one a kept road holds: a word trimmed, none for null or nothing.
fn advisor_of(body: &Value, kept: Option<&Entry>) -> Value {
  match body.get("advisor") {
    None => json!(kept.and_then(|road| road.advisor.clone())),
    Some(named) => json!(named.as_str().map(str::trim).filter(|advisor| !advisor.is_empty())),
  }
}

// A compaction threshold a body names: a fraction above 0 and at most 1.
fn fold_ratio_of(body: &Value) -> Result<Option<f64>, Refused> {
  match body.get("foldRatio") {
    None => Ok(None),
    Some(ratio) => ratio
      .as_f64()
      .filter(|ratio| *ratio > 0.0 && *ratio <= 1.0)
      .map(Some)
      .ok_or_else(|| bad("foldRatio", "A compaction threshold is a fraction above 0 and at most 1.")),
  }
}

// A repository a body names, which the store must hold; null clears it.
async fn repository_of(db: &DatabaseConnection, body: &Value) -> Result<Option<Option<String>>, Refused> {
  match body.get("repository") {
    None => Ok(None),
    Some(Value::Null) => Ok(Some(None)),
    Some(Value::String(name)) if crate::repositories::named(db, name).await?.is_some() => {
      Ok(Some(Some(name.clone())))
    }
    Some(_) => Err(bad("repository", "A supervisor's repository is one the store holds.")),
  }
}

// The call names a body names, as the row keeps them: none where it names none, and the line or none for an
// empty list or null where it names them; refused whole where a name is out of the policy's bounds
// (docs/supervisors.md, 「Its call names」).
fn call_names_of(body: &Value) -> Result<Option<Option<String>>, Refused> {
  let refused = || {
    let words = &CONTROL_WORDS.supervisor_roster.call_names;
    let most = SUPERVISORS.call_names_most.to_string();
    let longest = SUPERVISORS.call_name_longest.to_string();

    bad("aliases", fill_marks(words, &[("most", &most), ("longest", &longest)]))
  };
  let names = match body.get("aliases") {
    None => return Ok(None),
    Some(Value::Null) => vec![],
    Some(Value::Array(names)) if names.len() <= SUPERVISORS.call_names_most => names
      .iter()
      .map(|name| {
        let name = name.as_str().map(str::trim).ok_or_else(refused)?;
        let length = name.chars().count();

        if (1..=SUPERVISORS.call_name_longest).contains(&length) && !name.contains(',') {
          Ok(name.to_owned())
        } else {
          Err(refused())
        }
      })
      .collect::<Result<Vec<_>, _>>()?,
    Some(_) => return Err(refused()),
  };

  Ok(Some(supervisors::call_names_line(&names)))
}

// The compaction threshold a supervisor added on a road is given where it names none: its provider's declared
// default, 0.85 where it declares none (docs/supervisors.md, 「What a provider declares for a conversation」).
fn default_fold_ratio(road: &Entry) -> f64 {
  harness_providers::provider(&road.provider)
    .and_then(|provider| provider.declared().fold_ratio)
    .unwrap_or(order::DEFAULT_FOLD_RATIO)
}

/// `POST /supervisors`: a supervisor registered as one added on the console is, on its one road, and given its
/// brain.
pub struct SupervisorCreate;

impl Route for SupervisorCreate {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/supervisors";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let roster = &CONTROL_WORDS.supervisor_roster;
    let text =
      |field: &str| body.get(field).and_then(Value::as_str).map(str::trim).unwrap_or_default().to_owned();
    let name = text("name");
    let label = text("label");

    if !NAME.is_match(&name) {
      return Err(bad("name", roster.name.as_str()));
    }

    if supervisors::named(call.store(), &name).await?.is_some() {
      let taken = fill_marks(&roster.name_taken, &[("name", &name)]);

      return Err(Refused::new(StatusCode::CONFLICT, taken).with_code("name-taken"));
    }

    if label.is_empty() {
      return Err(bad("label", roster.label.as_str()));
    }

    let named = match body.get("ownerAccount") {
      None | Some(Value::Null) => None,
      Some(owner) => {
        Some(owner.as_i64().ok_or_else(|| bad("ownerAccount", CONTROL_WORDS.agents.owner.as_str()))?)
      }
    };
    // A member's supervisor is filed under their own account (docs/api.md, 「Who a caller acts for」).
    let owner = Acting::of(&call)?.files_under(named, &format!("the supervisor {name}"))?;

    retired(&body)?;

    let road = road_of(call.ground(), &body, None)
      .await?
      .ok_or_else(|| bad("provider", "A supervisor's road names its provider and its model."))?;
    let repository = repository_of(call.store(), &body).await?.flatten();
    let fold_ratio = fold_ratio_of(&body)?.unwrap_or_else(|| default_fold_ratio(&road));
    let conversation = Conversation::console_added();
    let warming: BTreeMap<&str, bool> = BTreeMap::from([(road.provider.as_str(), true)]);
    let now = now_ms();

    run(
      call.store(),
      r#"INSERT INTO "supervisors" ("name", "label", "owner_account", "icon", "brain_folder", "session_scope",
           "turn_shape", "repository", "provider", "model", "effort", "thinking", "answer_tokens", "warming",
           "fold_ratio", "created_at", "updated_at", "advisor")
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16, $17)"#,
      vec![
        name.clone().into(),
        label.clone().into(),
        owner.into(),
        SUPERVISORS.added_icon.clone().into(),
        format!("agents/{name}").into(),
        conversation.scope.word().into(),
        conversation.shape.word().into(),
        repository.into(),
        road.provider.clone().into(),
        road.model.clone().into(),
        road.effort.clone().into(),
        road.thinking.clone().into(),
        road.answer_tokens.into(),
        serde_json::to_string(&warming).map_err(Refused::failed)?.into(),
        fold_ratio.into(),
        now.into(),
        road.advisor.clone().into(),
      ],
    )
    .await?;

    let brain = born(call.ground().peers.brain.as_ref(), &name, &label, owner).await;
    let supervisor = supervisors::named(call.store(), &name).await?.ok_or_else(|| not_found(&name))?;

    eprintln!("agency: the supervisor {name} was registered");

    Ok(answer(
      StatusCode::CREATED,
      json!({ "supervisor": view(call.ground(), &supervisor).await?, "brain": brain }),
    ))
  }
}

crate::moved!(SupervisorCreate);

/// The registered supervisor's own brain asked of the brain, under its name, with its label, filed under its
/// owner. The supervisor stands whatever the brain answers, so what it could not do is logged.
async fn born(brain: &dyn Brain, name: &str, label: &str, owner: Option<i64>) -> BrainBirth {
  match brain.make(name, label, owner).await {
    Ok(Made::Made) => BrainBirth::Made,
    Ok(Made::Kept) => BrainBirth::Kept,
    Ok(Made::Undeployed) => BrainBirth::None,
    Err(fault) => {
      eprintln!("agency: the supervisor {name} was registered without its brain: {fault}");

      BrainBirth::Failed
    }
  }
}

/// `PATCH /supervisors/{supervisorName}`: the fields the body names changed, each judged as the registration
/// judges it.
pub struct SupervisorChange;

impl Route for SupervisorChange {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/supervisors/{supervisorName}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let name = call.param("supervisorName").unwrap_or_default().to_owned();
    let body = call.body().await?;
    let held = supervisors::named(call.store(), &name).await?.ok_or_else(|| not_found(&name))?;

    retired(&body)?;

    let road = road_of(call.ground(), &body, held.road.as_ref()).await?.or(held.road.clone());
    // A road the body moves — another provider, model, effort, thinking or answer ceiling — is what makes the
    // supervisor's sessions go, as the settings route's moved thinking does (docs/supervisors.md, 「Where a
    // supervisor's turn runs」). An advisor alone moves no session: the program takes it as it is started again on
    // its own session (docs/provider-order.md, 「The advisor」).
    let moved = match (&held.road, &road) {
      (Some(held), Some(road)) => road.moved_from(held),
      (held, road) => held != road,
    };
    let fold_ratio = fold_ratio_of(&body)?.unwrap_or(held.fold_ratio);
    let repository = repository_of(call.store(), &body).await?.unwrap_or(held.repository.clone());
    let label = match body.get("label") {
      None => held.label.clone(),
      Some(Value::String(label)) if !label.trim().is_empty() => Some(label.trim().to_owned()),
      Some(_) => return Err(bad("label", CONTROL_WORDS.supervisor_roster.label.as_str())),
    };
    let aliases = call_names_of(&body)?.unwrap_or(held.aliases.clone());
    let warming = match body.get("warming") {
      None => held.warming.clone(),
      Some(Value::Object(switches)) => {
        let mut warming = held.warming.clone();

        for (provider, on) in switches {
          let on = on.as_bool().ok_or_else(|| bad("warming", "A provider's warming is on or off."))?;

          warming.insert(provider.clone(), on);
        }

        warming
      }
      Some(_) => return Err(bad("warming", "Warming is an object from a provider to on or off.")),
    };
    let voice = match body.get("voice") {
      None => held.voice.clone(),
      Some(Value::Null) => None,
      Some(Value::String(voice)) => Some(voice.clone()),
      Some(_) => return Err(bad("voice", "A voice is a name, or null for none.")),
    };

    let card_rank = match body.get("cardRank") {
      None => held.card_rank,
      Some(Value::Null) => None,
      Some(rank) => Some(rank.as_u64().and_then(|rank| u32::try_from(rank).ok()).ok_or_else(|| {
        bad("cardRank", "A card rank is a whole number of zero or more, or null for none.")
      })?),
    };

    if let Some(Value::Object(gates)) = body.get("gates") {
      for (gate, on) in gates {
        let on = on.as_bool().ok_or_else(|| bad("gates", CONTROL_WORDS.agents.gate_switch.as_str()))?;

        set_setting(call.store(), &format!("agent_gate:{name}:{gate}"), if on { "on" } else { "off" })
          .await?;
      }
    }

    if let Some(sampling) = body.get("sampling") {
      let key = agent_sampling::key(&name);
      let kept = agent_sampling::held_values(settings::get(call.store(), &key).await?);
      let next = agent_sampling::changed(sampling, kept).map_err(|fault| match fault {
        super::fault::Fault::Field(_, error) => Refused::failed(error),
        super::fault::Fault::Failed(failed) => failed,
      })?;

      settings::set(call.store(), &key, &Value::Object(next).to_string()).await?;
    }

    run(
      call.store(),
      r#"UPDATE "supervisors" SET "label" = $2, "provider" = $3, "model" = $4, "effort" = $5, "thinking" = $6,
           "answer_tokens" = $7, "warming" = $8, "fold_ratio" = $9, "repository" = $10, "voice" = $11,
           "updated_at" = $12, "card_rank" = $13, "aliases" = $14, "advisor" = $15
         WHERE "name" = $1"#,
      vec![
        name.clone().into(),
        label.into(),
        road.as_ref().map(|road| road.provider.clone()).into(),
        road.as_ref().map(|road| road.model.clone()).into(),
        road.as_ref().and_then(|road| road.effort.clone()).into(),
        road.as_ref().and_then(|road| road.thinking.clone()).into(),
        road.as_ref().and_then(|road| road.answer_tokens).into(),
        serde_json::to_string(&warming).map_err(Refused::failed)?.into(),
        fold_ratio.into(),
        repository.into(),
        voice.into(),
        now_ms().into(),
        card_rank.and_then(|rank| i32::try_from(rank).ok()).into(),
        aliases.into(),
        road.as_ref().and_then(|road| road.advisor.clone()).into(),
      ],
    )
    .await?;

    // A road moved under a standing program reaches the supervisor's next turn: what its sessions held is
    // handed over and let go, and the program ends with them, so the next turn stands the model the row
    // names now; a supervisor carrying a turn keeps its session and its end makes the replacement
    // (docs/api.md, 「supervisors」; docs/supervisors.md, 「Where a supervisor's turn runs」).
    if moved && let Some(sessions) = call.sessions() {
      // Made behind the answer: a handoff takes as long as a session's note does, and the save waits on none.
      let name = name.clone();

      tokio::spawn(async move { sessions.road_moved(&name).await });
    }

    let supervisor = supervisors::named(call.store(), &name).await?.ok_or_else(|| not_found(&name))?;

    Ok(answer(StatusCode::OK, view(call.ground(), &supervisor).await?))
  }
}

crate::moved!(SupervisorChange);

// Whether the query switches a flag on.
fn switched(call: &Call, name: &str) -> bool {
  call.query(name).is_some_and(|value| value == "true")
}

/// `DELETE /supervisors/{supervisorName}`: a registration ended, with everything of the supervisor's own but
/// its brain where the caller keeps it.
pub struct SupervisorDelete;

impl Route for SupervisorDelete {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/supervisors/{supervisorName}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let words = &CONTROL_WORDS.agents;
    let name = call.param("supervisorName").unwrap_or_default().to_owned();
    let force = switched(&call, "force");
    let forget_brain = switched(&call, "forgetBrain");
    let rooms = rooms_named(call.query("rooms").map(Value::String).as_ref());
    let supervisor = supervisors::named(call.store(), &name).await?.ok_or_else(|| not_found(&name))?;
    let label = supervisor.label.clone().unwrap_or_else(|| name.clone());

    if supervisor.builtin {
      let seeded = fill_marks(&words.seeded, &[("label", &label)]);

      return Err(Refused::new(StatusCode::CONFLICT, seeded).with_code("seeded"));
    }

    if !force && held(&agent_lock_path(&call.data_root(), &name)) {
      let running = fill_marks(&words.running, &[("label", &label)]);

      return Err(Refused::new(StatusCode::CONFLICT, running).with_code("running"));
    }

    // A supervisor's conversation is reset first, so the rooms it answered keep their record and only its own
    // rows go; the reset cuts off a turn still holding it.
    let reset = reset_whole(&call, &name, rooms, &asked_by(&Value::Null), forget_brain);
    let left = Left::of(&call, &name);

    to_its_end(async move {
      reset.await?;

      let written = sea_orm::TransactionTrait::begin(&left.db).await?;

      if let Err(fault) = remove(&written, &name).await {
        return fault.answer();
      }

      written.commit().await?;
      left.taken(&name, forget_brain).await;
      eprintln!("agency: the registration of the supervisor {name} was ended");

      Ok(StatusCode::NO_CONTENT.into_response())
    })
    .await
  }
}

crate::moved!(SupervisorDelete);

#[allow(dead_code)]
async fn held_row(db: &DatabaseConnection, name: &str) -> Result<bool, Refused> {
  Ok(
    one(db, r#"SELECT 1 AS "one" FROM "supervisors" WHERE "name" = $1"#, vec![name.into()])
      .await?
      .is_some(),
  )
}
