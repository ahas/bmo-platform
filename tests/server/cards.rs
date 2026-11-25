//! The cards the arbiter's master grants a run on a registration naming a card service, against a stand-in
//! master (docs/own-llm.md, 「An engine the arbiter stands」): a run waiting with no clock until its demand is
//! granted, a demand handed in stood on and left to its caller, work asked within a run's demand lending the
//! run's card and telling the run its engine stood down, the demands left behind given back at start and
//! with their lease, a seat's
//! admission reading its demand as it stands, a seat standing on its caller's own demand with none asked,
//! one card shared by the runs out at once on a registration, kept for the runs still out when one is
//! let go and given back, granted or still waiting, once none is; and a turn's card wait carried as the
//! turn's typed state and its job's hold until the grant clears it.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agency_server::cards::{
  Ask, Asking, CardWait, Gateway, SharedAsk, Sharing, WaitTold, give_back_lease, give_back_left,
  give_back_on, give_back_with_leases, hold_on, holding, only_supervisor_of_room, seat_card_on, share,
  supervisor_rank, taken_from, terms, until_taken, watch_turn,
};
use agency_server::control::registry::{change, registration, view};
use agency_server::herdr_backend::TurnChanged;
use agency_server::order_run::dispatch::Request;
use agency_server::session_output::{OutputWatch, Outputs};
use agent_session::{SessionId, WaitKind};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use harness_machine::card_master::{Master, MasterAddress};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::scratch::Scratch;

/// The stand-in master's memory: the demands it holds, how often each was read, the ids given back, and
/// how it grants.
#[derive(Default)]
struct StandIn {
  next: i64,
  demands: BTreeMap<i64, Value>,
  reads: BTreeMap<i64, u32>,
  /// How long each read asked the answer held open, in milliseconds, in the order the reads came.
  waits: Vec<u64>,
  given_back: Vec<i64>,
  asked: Vec<Value>,
  /// How many reads a waiting demand waits before its card is granted.
  grant_after: u32,
  /// Whether a demand asked within another lends that other's card.
  lends: bool,
  /// The address the granted card's engine answers at, and its key file.
  engine: String,
  key_file: String,
  /// The origin the stand-in answers at, its gateway's among them.
  base: String,
}

type Shared = Arc<Mutex<StandIn>>;

fn card(stand_in: &StandIn) -> Value {
  json!([{ "nodeName": "n1", "address": stand_in.engine, "keyFile": stand_in.key_file }])
}

async fn ask(State(shared): State<Shared>, Json(body): Json<Value>) -> impl IntoResponse {
  let mut stand_in = shared.lock().unwrap();
  stand_in.next += 1;
  let id = stand_in.next;
  let demand = json!({
    "id": id, "serviceName": body["serviceName"], "count": 1, "cards": [], "state": "waiting",
    "reason": { "code": "in-use", "text": "Every card is in use." }, "waitedMs": 0,
    "within": body.get("within"), "workKey": body.get("workKey"),
  });

  stand_in.asked.push(body);
  stand_in.demands.insert(id, demand.clone());
  (StatusCode::ACCEPTED, Json(demand))
}

async fn read(
  State(shared): State<Shared>,
  Path(id): Path<i64>,
  Query(query): Query<BTreeMap<String, String>>,
) -> axum::response::Response {
  let mut stand_in = shared.lock().unwrap();
  let wait = query.get("waitMs").and_then(|wait| wait.parse::<u64>().ok()).unwrap_or_default();

  stand_in.waits.push(wait);

  let reads = {
    let reads = stand_in.reads.entry(id).or_default();
    *reads += 1;
    *reads
  };
  let grant_after = stand_in.grant_after;
  let cards = card(&stand_in);
  let lends = stand_in.lends;
  let Some(demand) = stand_in.demands.get_mut(&id) else {
    return StatusCode::NOT_FOUND.into_response();
  };

  if demand["state"] == "waiting" && reads >= grant_after {
    demand["state"] = json!("granted");
    demand["cards"] = cards;
    demand["reason"] = Value::Null;
  }

  let answered = demand.clone();

  if lends
    && answered["state"] == "granted"
    && let Some(outer) = answered["within"].as_i64()
    && let Some(outer) = stand_in.demands.get_mut(&outer)
  {
    outer["state"] = json!("lent");
  }

  Json(answered).into_response()
}

async fn give_back(State(shared): State<Shared>, Path(id): Path<i64>) -> StatusCode {
  let mut stand_in = shared.lock().unwrap();
  let Some(demand) = stand_in.demands.remove(&id) else {
    return StatusCode::NOT_FOUND;
  };

  stand_in.given_back.push(id);

  // The lent card goes back to the demand it was asked within: its engine started again, granted anew.
  if let Some(outer) = demand["within"].as_i64()
    && let Some(held) = stand_in.demands.get_mut(&outer)
    && held["state"] == "lent"
  {
    held["state"] = json!("waiting");
    held["cards"] = json!([]);
    held["reason"] = json!({ "code": "starting", "text": "The engine is starting." });
    stand_in.reads.insert(outer, 0);
  }

  StatusCode::NO_CONTENT
}

/// A stand-in master on a port of its own, its engine's health answered at the same origin.
async fn stand_in(grant_after: u32, lends: bool, key_file: &str) -> (Master, Shared) {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  let shared: Shared = Arc::new(Mutex::new(StandIn {
    grant_after,
    lends,
    engine: format!("{base}/v1/chat/completions"),
    key_file: key_file.to_owned(),
    base: base.clone(),
    ..StandIn::default()
  }));
  let app = axum::Router::new()
    .route("/demands", axum::routing::post(ask))
    .route("/demands/{id}", get(read).delete(give_back))
    .route("/health", get(|| async { Json(json!({ "context": 65_536 })) }))
    // The gateway's table: the granted card's entry served.
    .route("/v1/models", get(|| async { Json(json!({ "data": [{ "id": "flash@n1", "owned_by": "gateway" }] })) }))
    .with_state(shared.clone());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  let address = MasterAddress::read(|name| (name == "AGENCY_ARBITER_URL").then(|| base.clone()))
    .unwrap()
    .unwrap();

  (Master::new(address), shared)
}

// A demand the stand-in already holds, granted.
fn granted(shared: &Shared, id: i64) {
  let mut stand_in = shared.lock().unwrap();
  let cards = card(&stand_in);

  stand_in.next = stand_in.next.max(id);
  stand_in
    .demands
    .insert(id, json!({ "id": id, "serviceName": "arena", "cards": cards, "state": "granted" }));
}

fn asking<'a>(holder: &'a str, ask: Ask, lease: Option<&'a str>) -> Asking<'a> {
  Asking {
    registration: "flash",
    service: "arena",
    holder,
    work_key: Some("session-1"),
    ask,
    lease,
    stop: None,
  }
}

async fn ledger(db: &sea_orm::DatabaseConnection) -> Vec<i64> {
  store::all(db, r#"SELECT "demand_id" FROM "card_demands" ORDER BY "demand_id""#, vec![])
    .await
    .unwrap()
    .iter()
    .map(|row| row.try_get::<i64>("", "demand_id").unwrap())
    .collect()
}

// A seat the pool let in on the registration `flash`, for run 7.
fn seat(token: &str) -> Request {
  Request {
    sequence: 1,
    token: token.into(),
    request_key: "research:7".into(),
    owner: "research:7".into(),
    source: "search".into(),
    role: "scout".into(),
    path: None,
    ended_at: None,
    outcome: None,
    reservation: None,
    provider: "registered".into(),
    model: "flash".into(),
    effort: None,
    capacity: 1,
    state: "active".into(),
    enqueued_at: 1,
    leased_at: Some(1),
    child_id: None,
    yielding: false,
    place_key: None,
    session: Some("session-7".into()),
    title: None,
    asked_by: None,
    account: None,
    agent_name: None,
    agent_wanted: None,
    seat: None,
    held_for: None,
    ended_by: None,
  }
}

fn key_file(dir: &tempfile::TempDir) -> String {
  let path = dir.path().join("engine.key");

  std::fs::write(&path, "engine-key\n").unwrap();
  path.to_string_lossy().into_owned()
}

#[tokio::test]
async fn a_run_waits_with_no_clock_until_its_demand_is_granted_and_gives_it_back_at_its_end() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let (master, shared) = stand_in(4, false, &key_file(&data)).await;
  let mut said = vec![];
  let held = hold_on(
    scratch.db(),
    &master,
    asking("worker-a", Ask::default(), None),
    &mut |wait: &CardWait| said.push(wait.sentence()),
  )
  .await
  .unwrap();

  assert_eq!(held.node, "n1");
  assert!(held.asked && !held.lent, "{held:?}");
  assert_eq!(said, vec!["Waiting for a card of arena: Every card is in use.".to_owned()]);
  assert_eq!(shared.lock().unwrap().reads[&held.demand], 4, "read until granted, however many reads");
  assert_eq!(shared.lock().unwrap().asked[0]["workKey"], "session-1");
  assert_eq!(shared.lock().unwrap().asked[0]["count"], 1);
  assert_eq!(ledger(scratch.db()).await, vec![held.demand]);
  assert_eq!(holding(scratch.db(), "worker-a").await.unwrap(), Some(held.demand));

  let told = give_back_on(scratch.db(), &master, data.path(), &held).await;

  assert_eq!(told, None, "a run that lent nothing is told nothing");
  assert_eq!(shared.lock().unwrap().given_back, vec![held.demand]);
  assert!(ledger(scratch.db()).await.is_empty());
}

#[tokio::test]
async fn a_run_stopped_while_it_waits_for_its_grant_gives_its_demand_back_and_holds_nothing() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  // Never granted while the test lasts.
  let (master, shared) = stand_in(u32::MAX, false, &key_file(&data)).await;
  let run = "chat:5:1";
  let asked = Asking {
    stop: Some(run),
    ..asking("supervisor-a", Ask::default(), None)
  };
  let db = scratch.db().clone();
  let waiting = tokio::spawn(async move { hold_on(&db, &master, asked, &mut |_: &CardWait| {}).await });

  until(&shared, |stand_in| stand_in.reads.values().any(|reads| *reads > 2)).await;
  assert!(!waiting.is_finished(), "a run not stopped waits for its grant");

  agency_server::queue::settings::set(
    scratch.db(),
    &format!("{}{run}", agency_server::queue::running::EXECUTION_CANCEL_PREFIX),
    "1",
  )
  .await
  .unwrap();

  let held = tokio::time::timeout(Duration::from_secs(20), waiting)
    .await
    .expect("the stop ends the wait")
    .unwrap();
  let refused = held.expect_err("a stopped run holds no card");

  assert_eq!(refused.code, "stopped");
  assert!(refused.detail.contains(run), "{}", refused.detail);
  assert_eq!(shared.lock().unwrap().given_back.len(), 1, "the demand asked is given back");
  assert!(ledger(scratch.db()).await.is_empty());
}

#[tokio::test]
async fn a_demand_handed_in_is_stood_on_and_left_to_its_caller() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let (master, shared) = stand_in(1, false, &key_file(&data)).await;

  granted(&shared, 7);

  let own = Ask {
    own: Some(7),
    within: None,
    ..Ask::default()
  };
  let held = hold_on(scratch.db(), &master, asking("research", own, None), &mut |_: &CardWait| {})
    .await
    .unwrap();

  assert_eq!((held.demand, held.node.as_str(), held.asked), (7, "n1", false));
  assert!(shared.lock().unwrap().asked.is_empty(), "no other demand is asked for the run");

  give_back_on(scratch.db(), &master, data.path(), &held).await;

  assert!(shared.lock().unwrap().given_back.is_empty(), "the caller gives its own demand back");

  let gone = Ask {
    own: Some(99),
    within: None,
    ..Ask::default()
  };
  let refused =
    hold_on(scratch.db(), &master, asking("research", gone, None), &mut |_: &CardWait| {}).await;

  assert!(matches!(&refused, Err(why) if why.code == "demand-gone"), "{refused:?}");
}

#[tokio::test]
async fn work_asked_within_a_run_lends_its_card_and_the_run_is_told_once_its_engine_is_back() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let (master, shared) = stand_in(2, true, &key_file(&data)).await;

  granted(&shared, 50);

  let within = Ask {
    own: None,
    within: Some(50),
    ..Ask::default()
  };
  let held = hold_on(
    scratch.db(),
    &master,
    asking("worker-a", within, Some("seat-1")),
    &mut |_: &CardWait| {},
  )
  .await
  .unwrap();

  assert!(held.lent, "the run's own card was lent: {held:?}");
  assert_eq!(held.within, Some(50));
  assert_eq!(shared.lock().unwrap().asked[0]["within"], 50);
  assert_eq!(shared.lock().unwrap().demands[&50]["state"], "lent");

  let told = give_back_on(scratch.db(), &master, data.path(), &held).await;

  assert_eq!(
    told.as_deref(),
    Some("Your own model stood down while this work ran on its card, and it has come back.")
  );
  assert_eq!(
    shared.lock().unwrap().demands[&50]["state"],
    "granted",
    "the run's demand was granted again"
  );
  assert!(shared.lock().unwrap().reads[&50] >= 2, "the run waited for its own grant");
}

#[tokio::test]
async fn demands_left_behind_are_given_back_at_start_and_with_an_ended_lease() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let (master, shared) = stand_in(1, false, &key_file(&data)).await;

  for id in [11, 12] {
    granted(&shared, id);
  }

  for (id, lease) in [(11, None), (12, Some("ended-seat"))] {
    scratch
      .run(
        r#"INSERT INTO "card_demands" ("demand_id", "service", "registration", "holder", "lease", "asked_at")
           VALUES ($1, 'arena', 'flash', 'worker-a', $2, 1)"#,
        vec![i64::from(id).into(), lease.map(str::to_owned).into()],
      )
      .await;
  }

  assert_eq!(give_back_with_leases(scratch.db(), &master).await.unwrap(), 1);
  assert_eq!(
    shared.lock().unwrap().given_back,
    vec![12],
    "a lease no seat holds active gives its demand back"
  );
  assert_eq!(ledger(scratch.db()).await, vec![11]);

  assert_eq!(give_back_left(scratch.db(), &master).await.unwrap(), 1);
  assert_eq!(shared.lock().unwrap().given_back, vec![12, 11]);
  assert!(ledger(scratch.db()).await.is_empty());

  // A demand the master no longer holds counts as given back.
  scratch
    .run(
      r#"INSERT INTO "card_demands" ("demand_id", "service", "registration", "holder", "asked_at")
         VALUES (404, 'arena', 'flash', 'worker-a', 1)"#,
      vec![],
    )
    .await;

  assert_eq!(give_back_left(scratch.db(), &master).await.unwrap(), 1);
  assert!(ledger(scratch.db()).await.is_empty());
}

#[tokio::test]
async fn naming_a_card_service_takes_the_addresses_away_and_refuses_new_ones() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let data = tempfile::tempdir().unwrap();
  let save = |more: Value| {
    let mut body = json!({
      "action": "save", "name": "flash", "model": "m", "contextTokens": 32_768, "answerTokens": 4_096,
    });

    body.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
    body
  };
  let address = json!({
    "action": "set-address", "name": "flash", "node": "alpha",
    "url": "http://127.0.0.1:18021/v1/chat/completions", "key": null,
  });

  change(db, data.path(), &save(json!({}))).await.unwrap().unwrap();
  change(db, data.path(), &address).await.unwrap().unwrap();

  let held = registration(db, "flash").await.unwrap().unwrap();

  assert!(held.card_service.is_none() && held.addresses.len() == 1 && held.offered());

  change(
    db,
    data.path(),
    &json!({ "action": "save", "name": "flash", "cardService": "arena", "roleFiles": true }),
  )
  .await
  .unwrap()
  .unwrap();

  let held = registration(db, "flash").await.unwrap().unwrap();

  assert_eq!(held.card_service.as_deref(), Some("arena"));
  assert!(held.role_files && held.addresses.is_empty() && held.offered());
  assert!(held.holds_node("n4"), "a session goes wherever its demand is granted");

  let refused = change(db, data.path(), &address).await.unwrap();

  assert!(matches!(&refused, Err(why) if why.contains("arena")), "{refused:?}");

  let shown = view(db, data.path()).await.unwrap();

  assert_eq!(shown["registrations"][0]["cardService"], "arena");
  assert_eq!(shown["registrations"][0]["roleFiles"], true);

  let bad = change(db, data.path(), &save(json!({ "cardService": "Not A Service" }))).await.unwrap();

  assert!(bad.is_err(), "{bad:?}");

  change(db, data.path(), &save(json!({ "cardService": "none" }))).await.unwrap().unwrap();

  let held = registration(db, "flash").await.unwrap().unwrap();

  assert!(
    held.card_service.is_none() && !held.offered(),
    "no address and no card service is offered nowhere"
  );
}

// The registration `flash` naming the card service `arena`, and what a card its runs share is asked
// with: the stand-in as the master and as the gateway, its key file readable as the gateway's is.
async fn shared_ask(
  db: &sea_orm::DatabaseConnection,
  data: &tempfile::TempDir,
  master: &Master,
  shared: &Shared,
) -> SharedAsk {
  change(
    db,
    data.path(),
    &json!({
      "action": "save", "name": "flash", "model": "m", "contextTokens": 32_768, "answerTokens": 4_096,
      "cardService": "arena", "roleFiles": true,
    }),
  )
  .await
  .unwrap()
  .unwrap();

  let key_file = data.path().join("gateway.key");

  std::fs::write(&key_file, "gateway-key\n").unwrap();
  std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o600)).unwrap();

  SharedAsk {
    db: db.clone(),
    master: master.clone(),
    registration: "flash".into(),
    service: "arena".into(),
    holder: "translator".into(),
    gateway: Gateway {
      url: shared.lock().unwrap().base.clone(),
      key_file,
    },
  }
}

// The looks the reads below take, ten milliseconds apart, before they fail: half a minute, well past what
// the work under test takes on a loaded machine.
const LOOKS: usize = 3000;

// Read until `done` holds of the stand-in, the work under test going on its own meanwhile.
async fn until(shared: &Shared, done: impl Fn(&StandIn) -> bool) {
  for _ in 0..LOOKS {
    if done(&shared.lock().unwrap()) {
      return;
    }
    tokio::time::sleep(Duration::from_millis(10)).await;
  }

  panic!("the stand-in never came to the state looked for");
}

// Read until the ledger holds no demand. A part let go gives its demand back on a task of its own, which
// forgets the demand only once the master has answered the give-back, so the stand-in hears it first.
async fn until_forgotten(db: &sea_orm::DatabaseConnection) {
  for _ in 0..LOOKS {
    if ledger(db).await.is_empty() {
      return;
    }
    tokio::time::sleep(Duration::from_millis(10)).await;
  }

  panic!("the ledger still holds {:?}", ledger(db).await);
}

#[tokio::test]
async fn the_runs_out_at_once_on_a_registration_share_one_demand_given_back_by_the_last() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let (master, shared) = stand_in(3, false, &key_file(&data)).await;
  let sharing = Arc::new(Sharing::default());
  let db = scratch.db();
  let ask = shared_ask(db, &data, &master, &shared).await;
  let parts = (0..4).map(|_| share(&sharing, ask.clone()));
  let mut sharers: Vec<_> =
    futures_util::future::join_all(parts).await.into_iter().map(Result::unwrap).collect();
  let demand = sharers[0].held.demand;

  assert_eq!(shared.lock().unwrap().asked.len(), 1, "four parts out at once ask one demand");
  assert!(sharers.iter().all(|sharer| sharer.held.demand == demand && sharer.held.node == "n1"));

  let last = sharers.pop().unwrap();
  for sharer in sharers {
    sharer.leave().await;
  }

  assert!(shared.lock().unwrap().given_back.is_empty(), "kept while a part is still out");
  assert_eq!(ledger(db).await, vec![demand]);

  last.leave().await;

  assert_eq!(shared.lock().unwrap().given_back, vec![demand], "given back once, by the last");
  assert!(ledger(db).await.is_empty());

  let again = share(&sharing, ask.clone()).await.unwrap();

  assert_eq!(shared.lock().unwrap().asked.len(), 2, "a part out after the last left asks anew");
  again.leave().await;
  assert_eq!(shared.lock().unwrap().given_back.len(), 2);
}

#[tokio::test]
async fn a_part_let_go_while_the_shared_demand_waits_leaves_it_to_the_rest_and_the_last_gives_it_back() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let (master, shared) = stand_in(u32::MAX, false, &key_file(&data)).await;
  let sharing = Arc::new(Sharing::default());
  let db = scratch.db();
  let ask = shared_ask(db, &data, &master, &shared).await;
  let out = |sharing: &Arc<Sharing>| {
    let (sharing, ask) = (sharing.clone(), ask.clone());

    tokio::spawn(async move { share(&sharing, ask).await.map(|sharer| sharer.held.demand) })
  };
  let first = out(&sharing);

  until(&shared, |stand_in| stand_in.asked.len() == 1).await;

  let second = out(&sharing);
  let third = out(&sharing);
  let demand = *shared.lock().unwrap().demands.keys().next().unwrap();

  // The part that asked the demand is let go by its caller: the others keep waiting on the same demand.
  let read = shared.lock().unwrap().reads.get(&demand).copied().unwrap_or_default();

  first.abort();
  until(&shared, |stand_in| stand_in.reads.get(&demand).is_some_and(|reads| *reads > read + 20)).await;

  assert_eq!(shared.lock().unwrap().asked.len(), 1, "no part asks a demand of its own");
  assert!(shared.lock().unwrap().given_back.is_empty(), "kept while a part still waits on it");
  assert_eq!(ledger(db).await, vec![demand]);

  second.abort();
  third.abort();
  until(&shared, |stand_in| !stand_in.given_back.is_empty()).await;

  assert_eq!(
    shared.lock().unwrap().given_back,
    vec![demand],
    "given back by the last let go, still waiting"
  );
  assert_eq!(shared.lock().unwrap().asked.len(), 1);
  until_forgotten(db).await;

  // A part out after that asks anew, and is granted as the master grants.
  shared.lock().unwrap().grant_after = 1;

  let again = share(&sharing, ask.clone()).await.unwrap();

  assert_eq!(shared.lock().unwrap().asked.len(), 2);

  // The last let go while the card stands gives it back as it goes.
  drop(again);
  until(&shared, |stand_in| stand_in.given_back.len() == 2).await;
  until_forgotten(db).await;
}

#[tokio::test]
async fn a_seat_s_admission_reads_its_demand_as_it_stands_and_answers_at_once() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let (master, shared) = stand_in(2, false, &key_file(&data)).await;

  change(
    scratch.db(),
    data.path(),
    &json!({
      "action": "save", "name": "flash", "model": "m", "contextTokens": 32_768, "answerTokens": 4_096,
      "cardService": "arena", "roleFiles": true,
    }),
  )
  .await
  .unwrap()
  .unwrap();

  let seat = seat("seat-polled");
  let within = Ask {
    own: None,
    within: Some(30),
    ..Ask::default()
  };
  let admission = || seat_card_on(scratch.db(), Some(&master), &seat, within.clone());

  // The first admission asks the demand, within the run's; the stand-in grants it on its second read.
  let asked = admission().await.unwrap().unwrap();

  assert!(!asked.granted() && asked.state == "waiting", "{asked:?}");
  assert_eq!(shared.lock().unwrap().asked[0]["within"], 30);
  assert_eq!(ledger(scratch.db()).await, vec![asked.id], "the demand is kept under the seat's lease");

  let waiting = admission().await.unwrap().unwrap();

  assert_eq!(waiting.id, asked.id);
  assert!(!waiting.granted() && waiting.reason.is_some(), "{waiting:?}");

  let granted = admission().await.unwrap().unwrap();

  assert_eq!(granted.id, asked.id);
  assert_eq!(granted.node_name.as_deref(), Some("n1"));
  assert!(granted.granted() && granted.reason.is_none(), "{granted:?}");
  assert_eq!(
    shared.lock().unwrap().waits,
    vec![0, 0],
    "an admission reads the demand as it stands, holding nothing open at the master"
  );
}

#[tokio::test]
async fn a_seat_standing_on_its_caller_s_own_demand_asks_none_and_leaves_it_to_its_caller() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let (master, shared) = stand_in(1, true, &key_file(&data)).await;

  change(
    scratch.db(),
    data.path(),
    &json!({
      "action": "save", "name": "flash", "model": "m", "contextTokens": 32_768, "answerTokens": 4_096,
      "cardService": "arena", "roleFiles": true,
    }),
  )
  .await
  .unwrap()
  .unwrap();
  granted(&shared, 7);

  let seat = seat("seat-standing");
  let own = Ask {
    own: Some(7),
    within: None,
    ..Ask::default()
  };
  let card = seat_card_on(scratch.db(), Some(&master), &seat, own).await.unwrap().unwrap();

  assert_eq!((card.id, card.node_name.as_deref()), (7, Some("n1")));
  assert!(card.granted(), "{card:?}");
  assert!(shared.lock().unwrap().asked.is_empty(), "no demand of the seat's own is asked");
  assert!(ledger(scratch.db()).await.is_empty(), "nothing is kept under the seat's lease");
  assert_eq!(shared.lock().unwrap().waits, vec![0], "the demand is read as it stands");

  let told = give_back_lease(scratch.db(), &master, data.path(), &seat.token).await.unwrap();

  assert_eq!(told, None, "the caller's engine never stood down");
  assert!(shared.lock().unwrap().given_back.is_empty(), "the caller gives its own demand back");
  assert_eq!(shared.lock().unwrap().demands[&7]["state"], "granted");

  let gone = Ask {
    own: Some(99),
    within: None,
    ..Ask::default()
  };
  let refused = seat_card_on(scratch.db(), Some(&master), &seat, gone).await;

  assert!(matches!(&refused, Err(why) if why.code == "demand-gone"), "{refused:?}");
}

// A ground whose environment names the stand-in job service and receiver, so a wait is told to them.
fn told_ground(
  scratch: &Scratch,
  home: &std::path::Path,
  vars: BTreeMap<String, String>,
) -> agency_server::places::Ground {
  let service = agency_server::Service::new(
    scratch.db().clone(),
    agency_server::config::Config::read(|_| None).unwrap(),
  );
  let mut env = vec![
    ("HOME".to_owned(), home.to_string_lossy().into_owned()),
    ("PATH".to_owned(), "/nowhere".to_owned()),
  ];

  env.extend(vars);

  agency_server::places::Ground::new(
    &service,
    &harness_providers::Machine::with_env(env, home.join("data")),
    None,
  )
}

// A job's row as a launch writes it, its turn going out.
async fn running_job(scratch: &Scratch, id: i64) {
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
                             "state", "started_at", "prompt")
         VALUES ($1, 40, 1, 'harness', 1, 'helper', 'running', $2, 'work')"#,
      vec![id.into(), agency_server::now_ms().into()],
    )
    .await;
}

async fn held_for(scratch: &Scratch, id: i64) -> Option<String> {
  store::one(scratch.db(), r#"SELECT "held_for" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
    .await
    .unwrap()
    .and_then(|row| row.try_get::<Option<String>>("", "held_for").ok().flatten())
}

// A turn waiting for its card carries the wait as its typed state — what it waits for, the card service, the
// reason as it changes and the moment it began — on its read and in `turn.changed`, its job held with the
// wait's sentence meanwhile, and the grant clears all three (docs/api.md, 「sessions」).
#[tokio::test]
async fn a_turn_s_card_wait_is_its_state_and_its_job_s_hold_until_the_grant_clears_it() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (peers, vars) = crate::support::peers::stand().await;
  let ground = told_ground(&scratch, home.path(), vars);
  let outputs = Outputs::default();

  peers
    .lock()
    .unwrap()
    .items
    .insert(40, crate::support::peers::item(40, "leased", json!({ "scope": "-400" })));
  let session = SessionId("helper@run:1".into());
  let mut changed = ground.events.subscribe::<TurnChanged>();

  running_job(&scratch, 7).await;
  outputs.begun(&session, 3, "job-7");

  let mut told = WaitTold {
    ground: &ground,
    shown: Some(OutputWatch {
      outputs: &outputs,
      session: &session,
      turn: 3,
    }),
    run: "job-7",
    job: Some(7),
    since: None,
  };

  told.cleared().await;
  assert!(changed.ready().is_none(), "a run that never waited tells nothing");

  told
    .waits(&CardWait {
      service: "arena".into(),
      reason: "Every card is in use.".into(),
    })
    .await;

  let waiting = outputs.read(&session, Some(3)).unwrap().waiting.expect("the turn waits for its card");

  assert_eq!(
    (waiting.kind, waiting.service.as_str(), waiting.reason.as_str()),
    (WaitKind::Card, "arena", "Every card is in use.")
  );
  assert_eq!(
    held_for(&scratch, 7).await.as_deref(),
    Some("Waiting for a card of arena: Every card is in use.")
  );

  told
    .waits(&CardWait {
      service: "arena".into(),
      reason: "The engine is starting.".into(),
    })
    .await;

  let moved = outputs.read(&session, Some(3)).unwrap().waiting.unwrap();

  assert_eq!(moved.reason, "The engine is starting.");
  assert_eq!(moved.since, waiting.since, "the wait began once, whatever its reason became");

  told.cleared().await;

  assert_eq!(outputs.read(&session, Some(3)).unwrap().waiting, None, "the grant clears the wait");
  assert_eq!(held_for(&scratch, 7).await, None, "and the job's hold");

  let mut events = vec![];

  while let Some(event) = changed.ready() {
    events.push(event);
  }

  assert_eq!(events.len(), 3, "{events:?}");
  assert_eq!(
    (events[0].session.as_str(), events[0].turn_id, events[0].run.as_str()),
    ("helper@run:1", 3, "job-7")
  );
  assert_eq!(
    events[0].waiting.as_ref().map(|wait| wait.reason.as_str()),
    Some("Every card is in use.")
  );
  assert_eq!(
    events[1].waiting.as_ref().map(|wait| wait.reason.as_str()),
    Some("The engine is starting.")
  );
  assert_eq!(events[2].waiting, None, "the clearing is told as well");

  // The item's orderer was told of the wait once, as it began, however often its reason changed.
  let told_orderer = peers.lock().unwrap().instructed();

  assert_eq!(told_orderer.len(), 1, "{told_orderer:?}");
  assert_eq!(told_orderer[0]["kind"], "job-waiting:40");
  assert!(
    told_orderer[0]["said"]["order"].as_str().unwrap().contains("Every card is in use."),
    "{told_orderer:?}"
  );

  // A turn that ends waits for nothing.
  told
    .waits(&CardWait {
      service: "arena".into(),
      reason: "Every card is in use.".into(),
    })
    .await;
  outputs.ended(&session, 3);
  assert_eq!(outputs.read(&session, Some(3)).unwrap().waiting, None);
}

#[tokio::test]
async fn a_demand_is_asked_at_the_rank_and_in_the_pool_its_caller_names() {
  let scratch = Scratch::new().await;
  let data = tempfile::tempdir().unwrap();
  let (master, shared) = stand_in(1, false, &key_file(&data)).await;
  let mut waiting = |_: &CardWait| {};
  let named = Ask {
    terms: terms(Some(0), Some(("mine".into(), 1))),
    ..Ask::default()
  };

  hold_on(scratch.db(), &master, asking("a-holder", named, None), &mut waiting)
    .await
    .unwrap();
  hold_on(scratch.db(), &master, asking("a-holder", Ask::default(), None), &mut waiting)
    .await
    .unwrap();

  let asked = shared.lock().unwrap().asked.clone();

  assert_eq!(
    (&asked[0]["rank"], &asked[0]["pool"]),
    (&json!(0), &json!({ "key": "mine", "cards": 1 }))
  );
  assert!(asked[1].get("rank").is_none() && asked[1].get("pool").is_none(), "{:?}", asked[1]);
}

#[tokio::test]
async fn a_watch_ends_only_when_the_card_is_taken_or_the_demand_is_gone() {
  let data = tempfile::tempdir().unwrap();
  let (master, shared) = stand_in(1, false, &key_file(&data)).await;
  let scratch = Scratch::new().await;
  let mut quiet = |_: &CardWait| {};
  let held = hold_on(scratch.db(), &master, asking("a-holder", Ask::default(), None), &mut quiet)
    .await
    .unwrap();

  let watched = tokio::spawn({
    let master = master.clone();
    let demand = held.demand;
    async move { until_taken(&master, demand).await }
  });

  // Granted, and then lent by its own caller: the watch stands through both.
  tokio::time::sleep(Duration::from_millis(100)).await;
  shared.lock().unwrap().demands.get_mut(&held.demand).unwrap()["state"] = json!("lent");
  tokio::time::sleep(Duration::from_millis(100)).await;
  assert!(!watched.is_finished(), "a card its caller lent is not taken");

  // Taken by the bench: its engine is stopped under the turn, which the watch leaves as it stands.
  shared.lock().unwrap().demands.get_mut(&held.demand).unwrap()["taken"] = json!("bench");
  tokio::time::sleep(Duration::from_millis(100)).await;
  assert!(!watched.is_finished(), "a lower tier's taking stops no turn");

  {
    let demands = &mut shared.lock().unwrap().demands;
    let demand = demands.get_mut(&held.demand).unwrap();

    demand["taken"] = json!("agency a-better-rank");
    demand["outranked"] = json!(true);
  }

  assert_eq!(watched.await.unwrap().as_deref(), Some("agency a-better-rank"));

  assert!(shared.lock().unwrap().waits.iter().all(|wait| *wait > 0), "each read is held open");
  shared.lock().unwrap().demands.remove(&held.demand);
  assert_eq!(
    until_taken(&master, held.demand).await,
    None,
    "a demand the master no longer holds was not taken"
  );
}

#[tokio::test]
async fn a_turns_watch_marks_its_run_once_its_card_is_taken_and_finishes_clean() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (master, shared) = stand_in(1, false, &key_file(&home)).await;
  let base = shared.lock().unwrap().base.clone();
  let mut config = agency_server::config::Config::read(|_| None).unwrap();

  config.peers.arbiter =
    MasterAddress::read(|name| (name == "AGENCY_ARBITER_URL").then(|| base.clone())).unwrap();

  let service = agency_server::Service::new(scratch.db().clone(), config);
  let ground = agency_server::places::Ground::new(
    &service,
    &harness_providers::Machine::with_env(
      vec![("HOME".to_owned(), home.path().to_string_lossy().into_owned())],
      home.path().join("data"),
    ),
    None,
  );
  let mut quiet = |_: &CardWait| {};
  let held = hold_on(scratch.db(), &master, asking("a-holder", Ask::default(), None), &mut quiet)
    .await
    .unwrap();
  let watching = watch_turn(&ground, &held, "a-run-of-a-watch");

  {
    let demands = &mut shared.lock().unwrap().demands;
    let demand = demands.get_mut(&held.demand).unwrap();

    demand["taken"] = json!("agency a-better-rank");
    demand["outranked"] = json!(true);
  }

  for _ in 0..200 {
    if agency_server::queue::cancels::standing_named(scratch.db(), "a-run-of-a-watch")
      .await
      .unwrap()
    {
      break;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  assert!(
    agency_server::queue::cancels::standing_named(scratch.db(), "a-run-of-a-watch")
      .await
      .unwrap(),
    "the turn is stopped"
  );
  watching.finish(&ground).await;
  assert_eq!(taken_from("a-run-of-a-watch").as_deref(), Some("agency a-better-rank"));
  assert_eq!(taken_from("a-run-of-a-watch"), None, "told once");
}

#[tokio::test]
async fn a_room_names_its_supervisor_only_where_exactly_one_spoke_there() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  crate::support::rows::supervisor(db, "first", &[]).await;
  crate::support::rows::supervisor(db, "second", &[]).await;

  let (first, second) = (&"first".to_owned(), &"second".to_owned());
  let run_in = async |agent: &str, room: i64, unit: &str| {
    store::run(
      db,
      r#"INSERT INTO "runs" ("agent", "chat_id", "unit", "depth", "started_at") VALUES ($1, $2, $3, 0, 1)"#,
      vec![agent.into(), room.into(), unit.into()],
    )
    .await
    .unwrap();
  };

  run_in(first, 11, "one").await;
  run_in("a-role-that-is-no-supervisor", 11, "two").await;
  assert_eq!(only_supervisor_of_room(db, 11).await.unwrap().as_ref(), Some(first));

  run_in(second, 12, "three").await;
  run_in(first, 12, "four").await;
  assert_eq!(
    only_supervisor_of_room(db, 12).await.unwrap(),
    None,
    "two supervisors of one room is no answer"
  );
  assert_eq!(only_supervisor_of_room(db, 13).await.unwrap(), None, "a room none spoke in names none");

  assert_eq!(supervisor_rank(db, first).await, None);
  store::run(
    db,
    r#"UPDATE "supervisors" SET "card_rank" = 0 WHERE "name" = $1"#,
    vec![first.as_str().into()],
  )
  .await
  .unwrap();
  assert_eq!(supervisor_rank(db, first).await, Some(0));
  assert_eq!(supervisor_rank(db, "a-name-no-supervisor-holds").await, None);
}
