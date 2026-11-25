//! Agency's control routes (docs/api.md): every route answered here, a path none answers a 404. A route
//! registers itself where it is written, so no list of them is kept anywhere; every route answers on
//! agency's ground, with the store it keeps.

mod account_routes;
mod agent_roles;
mod agent_sampling;
mod attachment_routes;
mod background_work;
mod base_prompts;
pub mod borrowings;
mod catalogue_routes;
mod catalogues;
mod chat_documents;
pub mod claude_account_changes;
pub mod claude_accounts;
pub mod claude_logins;
mod documents;
pub mod events_stream;
mod executions;
pub mod fallback;
pub mod fault;
mod gateway_members;
pub mod github_deliveries;
pub mod harness;
mod health;
mod hook_routes;
pub mod item_filings;
pub mod job_stops;
mod keepalive;
pub mod loans;
mod machine;
pub mod models;
mod node_reads;
pub mod organisation_supply;
mod own_agent_routes;
pub mod own_llm;
mod owner_removal;
mod person_node_routes;
mod personal_app_routes;
pub mod personal_models;
mod personal_skill_routes;
pub mod personal_supply;
pub mod pool;
mod pool_routes;
pub mod preamble;
pub mod quota;
pub mod registry;
pub mod relay_routes;
mod repository_routes;
pub mod road_verbs;
pub(crate) mod role_agents;
mod rooms;
pub mod run_clocks;
mod run_ledger;
mod run_log;
pub mod session_ops;
pub mod sessions;
pub mod signup_composition;
mod supervisor_removal;
pub mod supervisor_reset;
mod supervisor_routes;
pub mod supervisor_settings;
pub mod supervisor_sleep;
pub mod supply_test;
pub(crate) mod support_growth;
pub mod translation_runs;
pub mod typed_line;
pub(crate) mod walk_reads;
pub mod words;
pub mod work;
mod worker_runtime;
mod workers_routes;

use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Query, Request, State};
use axum::http::{Method, StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use harness_providers::Machine;
use policy::control_words::CONTROL_WORDS;
use sea_orm::{ConnectionTrait, DatabaseConnection};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::events::Bus;
use crate::places::{Ground, Places};
use crate::queue::settings;
use crate::supervisor_turn::going::GoingRuns;
pub use inventory;

/// One route the front answers, written beside its answer: `crate::moved!(BasePrompts);`
pub trait Route: Send + Sync + 'static {
  const METHOD: Method;
  const PATH: &'static str;
  /// Whether a run's program may call the route for itself with its session token, beside the control token's
  /// holder (docs/api.md, 「The control port」). A route that does answers for the record's holder alone.
  const TAKES_RUN_TOKEN: bool = false;

  fn answer(&self, call: Call) -> impl Future<Output = Result<Response, Refused>> + Send;
}

type Answering<'a> = Pin<Box<dyn Future<Output = Result<Response, Refused>> + Send + 'a>>;

/// A route as the front holds it, whatever its type.
pub trait Answers: Send + Sync {
  fn method(&self) -> Method;
  fn path(&self) -> &'static str;
  fn takes_run_token(&self) -> bool;
  fn answer(&self, call: Call) -> Answering<'_>;
}

impl<R: Route> Answers for R {
  fn method(&self) -> Method {
    R::METHOD
  }

  fn path(&self) -> &'static str {
    R::PATH
  }

  fn takes_run_token(&self) -> bool {
    R::TAKES_RUN_TOKEN
  }

  fn answer(&self, call: Call) -> Answering<'_> {
    Box::pin(Route::answer(self, call))
  }
}

/// One registered route.
pub struct Moved(pub &'static dyn Answers);

inventory::collect!(Moved);

/// Registers a route with the front.
#[macro_export]
macro_rules! moved {
  ($route:expr) => {
    $crate::control::inventory::submit! { $crate::control::Moved(&$route) }
  };
}

/// Every route linked into this binary.
pub fn moved_routes() -> impl Iterator<Item = &'static dyn Answers> {
  inventory::iter::<Moved>.into_iter().map(|entry| entry.0)
}

/// A route's name as a caller writes it: its method and its path.
pub fn route_name(method: &Method, path: &str) -> String {
  format!("{method} {path}")
}

/// What is wrong with the routes linked in: two answers for one route would leave which one answers to
/// the order they were linked. The front refuses to stand on any.
pub fn route_faults() -> Vec<String> {
  let mut seen = BTreeSet::new();

  moved_routes()
    .map(|route| route_name(&route.method(), route.path()))
    .filter(|name| !seen.insert(name.clone()))
    .map(|name| format!("{name}: registered twice"))
    .collect()
}

/// A refusal: a status, and a body naming why under `error`, with the status's own kebab-case code.
#[derive(Debug)]
pub struct Refused {
  pub status: StatusCode,
  pub error: String,
  /// The code the route answers with where it is not the status's own: a rule of its own, as
  /// `no-worker-free`.
  pub code: Option<String>,
  /// The field of the body it refuses, where it refuses one.
  pub field: Option<String>,
}

impl Refused {
  /// A refusal under the status the route gives it.
  pub fn new(status: StatusCode, error: impl Into<String>) -> Self {
    Self {
      status,
      error: error.into(),
      code: None,
      field: None,
    }
  }

  /// The same refusal naming the field of the body it refuses.
  pub fn with_field(mut self, field: impl Into<String>) -> Self {
    self.field = Some(field.into());
    self
  }

  /// The same refusal under a code of the route's own.
  pub fn with_code(mut self, code: impl Into<String>) -> Self {
    self.code = Some(code.into());
    self
  }

  /// What a route answers for a failure it did not expect: 400, with the failure's own words.
  pub fn failed(error: impl std::fmt::Display) -> Self {
    Self {
      status: StatusCode::BAD_REQUEST,
      error: error.to_string(),
      code: None,
      field: None,
    }
  }

  /// The code it is answered with: the route's own where it named one, the status's kebab-case reason
  /// otherwise.
  pub fn code(&self) -> String {
    self.code.clone().unwrap_or_else(|| {
      self
        .status
        .canonical_reason()
        .unwrap_or("failed")
        .to_lowercase()
        .replace([' ', '\''], "-")
    })
  }
}

impl From<sea_orm::DbErr> for Refused {
  fn from(error: sea_orm::DbErr) -> Self {
    Self::failed(error)
  }
}

// A refusal of agency's own helpers, said as any unexpected failure is.
impl From<crate::Refusal> for Refused {
  fn from(refusal: crate::Refusal) -> Self {
    Self::failed(refusal.detail)
  }
}

// A fault of the store's statements, said the same way.
impl From<store::Fault> for Refused {
  fn from(fault: store::Fault) -> Self {
    Self::failed(fault.detail)
  }
}

impl IntoResponse for Refused {
  fn into_response(self) -> Response {
    let mut body = json!({ "code": self.code(), "error": self.error });

    if let Some(field) = self.field {
      body["field"] = json!(field);
    }

    (self.status, Json(body)).into_response()
  }
}

/// One setting of agency's own `settings` table, as a route reads it.
pub(crate) async fn setting<C: ConnectionTrait>(db: &C, key: &str) -> Result<Option<String>, Refused> {
  Ok(settings::get(db, key).await?)
}

/// One setting written, over whatever it held.
pub(crate) async fn set_setting<C: ConnectionTrait>(db: &C, key: &str, value: &str) -> Result<(), Refused> {
  Ok(settings::set(db, key, value).await?)
}

/// The agent a name stands for: a room's own conversational agent, named `chat:<room>`, is the supervisor of
/// one session across its rooms whose turn went into that room last, as the run ledger records it.
pub(crate) async fn canonical_agent<C: ConnectionTrait>(db: &C, name: &str) -> Result<String, Refused> {
  let room = name.strip_prefix("chat:").map(|room| room.strip_prefix('-').unwrap_or(room));
  let names_room =
    room.is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()));

  if !names_room {
    return Ok(name.to_owned());
  }

  let chat_id: i64 = name.trim_start_matches("chat:").parse().unwrap_or_default();
  let latest = store::one(
    db,
    r#"SELECT "r"."agent" FROM "runs" AS "r" JOIN "supervisors" AS "s" ON "s"."name" = "r"."agent"
       WHERE "r"."chat_id" = $1 AND "s"."session_scope" = 'agent' ORDER BY "r"."id" DESC LIMIT 1"#,
    vec![chat_id.into()],
  )
  .await?;

  Ok(match latest {
    Some(row) => row.try_get("", "agent")?,
    None => name.to_owned(),
  })
}

/// One request a route answers, on agency's ground.
pub struct Call {
  pub request: Request,
  ground: Arc<Ground>,
  machine: Arc<Machine>,
  // The picture of this machine's herdr the server keeps, where one stands, which any route reading what
  // runs joins with the machine's units.
  herdr: Option<herdr::picture::Watch>,
  // The workers' places in herdr the server stands, where it stands them, which any route that changes
  // what they stand after hands its change to.
  places: Option<Places>,
  going: Arc<GoingRuns>,
  // The session contract over the places the server stands, where it stands them.
  sessions: Option<crate::herdr_backend::Shared>,
  // What the request's path held at each `{name}` of the route's path.
  params: Vec<(String, String)>,
  bearer: Bearer,
}

/// Whom a request comes from, as its bearer token says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bearer {
  /// The control token's holder, who reaches every route.
  Control,
  /// A run's program by the session token the tool host issued it: the holder its record names, the agent the
  /// run works for.
  Run(String),
}

impl Call {
  /// Whom the request comes from.
  pub fn bearer(&self) -> &Bearer {
    &self.bearer
  }

  /// The ground the server stands on: its store, configuration, peers, events, hooks and trees.
  pub fn ground(&self) -> &Ground {
    &self.ground
  }

  /// The ground, shared, for work a route sets off past its answer.
  pub fn shared_ground(&self) -> Arc<Ground> {
    self.ground.clone()
  }

  /// Agency's store.
  pub fn store(&self) -> &DatabaseConnection {
    &self.ground.db
  }

  /// The machine the server stands on, as a harness provider reads it (docs/provider-presence.md).
  pub fn machine(&self) -> &Machine {
    &self.machine
  }

  /// The data root the server stands on.
  pub fn data_root(&self) -> PathBuf {
    self.ground.roots.data.clone()
  }

  /// The tree agency's assets — its prompts and its policy — are read out of: the code tree it runs.
  pub fn asset_root(&self) -> PathBuf {
    self.ground.code_root()
  }

  /// The event bus agency publishes on.
  pub fn events(&self) -> &Bus {
    &self.ground.events
  }

  /// What is running on the machine, read off the store's open rows, the machine's units and the
  /// picture of herdr the server keeps where one stands.
  pub fn running(&self) -> crate::queue::running::Running {
    crate::queue::running::Running::new(self.ground.db.clone(), &self.machine, self.herdr.clone())
  }

  /// The server's stops.
  pub fn stops(&self) -> crate::queue::stops::Stops {
    crate::queue::stops::Stops::of(&self.ground, self.herdr.clone())
  }

  /// The workers' places in herdr, where the server stands them.
  pub fn places(&self) -> Option<&Places> {
    self.places.as_ref()
  }

  /// The session contract, where the server stands the places it is carried over.
  pub fn sessions(&self) -> Option<crate::herdr_backend::Shared> {
    self.sessions.clone()
  }

  /// The supervisors' runs going now, as `supervisor-run-going` names them.
  pub fn going(&self) -> Arc<GoingRuns> {
    self.going.clone()
  }

  /// What the request's path holds at the route path's `{name}`.
  pub fn param(&self, name: &str) -> Option<&str> {
    self.params.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
  }

  /// The names the request's query gives, each once, in the order first given.
  pub fn query_names(&self) -> Vec<String> {
    let Ok(Query(pairs)) = Query::<Vec<(String, String)>>::try_from_uri(self.request.uri()) else {
      return vec![];
    };
    let mut names: Vec<String> = vec![];

    for (name, _) in pairs {
      if !names.contains(&name) {
        names.push(name);
      }
    }

    names
  }

  /// One query parameter, the first where it is given more than once.
  pub fn query(&self, name: &str) -> Option<String> {
    let Query(pairs) = Query::<Vec<(String, String)>>::try_from_uri(self.request.uri()).ok()?;

    pairs.into_iter().find(|(key, _)| key == name).map(|(_, value)| value)
  }

  /// One header of the request, where it is given and reads as text.
  pub fn header(&self, name: &str) -> Option<String> {
    self
      .request
      .headers()
      .get(name)
      .and_then(|value| value.to_str().ok())
      .map(str::to_owned)
  }

  /// The body as the bytes it came in, for a route that checks them before it reads them. A route reads it
  /// once.
  pub async fn raw_body(&mut self) -> Result<axum::body::Bytes, Refused> {
    let body = std::mem::take(self.request.body_mut());

    axum::body::to_bytes(body, BODY_LIMIT).await.map_err(Refused::failed)
  }

  /// The body read as JSON, whatever its shape; a body that is not JSON is refused. A route reads it once.
  pub async fn body(&mut self) -> Result<Value, Refused> {
    let body = std::mem::take(self.request.body_mut());
    let bytes = axum::body::to_bytes(body, BODY_LIMIT).await.map_err(Refused::failed)?;

    serde_json::from_slice(&bytes).map_err(|_| Refused::failed(CONTROL_WORDS.request.not_json.as_str()))
  }

  /// The request's JSON body read ahead of its route, and put back for the route to read; none where it is no JSON
  /// or cannot be read, the route then answering it as it does.
  pub async fn peek_body(&mut self) -> Option<Value> {
    let body = std::mem::take(self.request.body_mut());
    let bytes = axum::body::to_bytes(body, BODY_LIMIT).await.ok()?;
    let read = serde_json::from_slice(&bytes).ok();

    *self.request.body_mut() = axum::body::Body::from(bytes);
    read
  }
}

// The most a body read whole may carry: 128 MiB.
const BODY_LIMIT: usize = 128 * 1024 * 1024;

/// A number that is whole, however it was written, as JavaScript's `Number.isInteger` reads one.
pub fn whole(value: &Value) -> Option<i64> {
  value
    .as_i64()
    .or_else(|| value.as_f64().filter(|number| number.fract() == 0.0).map(|number| number as i64))
}

/// An answer: a JSON body under a status.
pub fn answer(status: StatusCode, body: Value) -> Response {
  (status, Json(body)).into_response()
}

/// What the front stands on: the token a caller is let in by, agency's ground, the places the server
/// stands, the picture of herdr it keeps, the supervisors' runs going, and the routes linked in.
pub struct Front {
  token: String,
  ground: Arc<Ground>,
  machine: Arc<Machine>,
  // The routes by method, each matched by its path; a path's static segment stands before a `{name}`.
  routes: HashMap<Method, matchit::Router<&'static dyn Answers>>,
  places: Option<Places>,
  herdr: Option<herdr::picture::Watch>,
  going: Arc<GoingRuns>,
  sessions: Option<crate::herdr_backend::Shared>,
}

impl Front {
  /// A front over the routes linked in, on agency's ground. Refused where they are not a set
  /// (`route_faults`).
  pub fn new(token: String, ground: Arc<Ground>) -> Result<Self, String> {
    let faults = route_faults();

    if !faults.is_empty() {
      return Err(faults.join("; "));
    }

    let mut routes: HashMap<Method, matchit::Router<&'static dyn Answers>> = HashMap::new();

    for route in moved_routes() {
      routes
        .entry(route.method())
        .or_default()
        .insert(route.path(), route)
        .map_err(|error| format!("{}: {error}", route_name(&route.method(), route.path())))?;
    }

    let machine = Arc::new(ground.machine());
    let going = Arc::new(GoingRuns::on(&ground.events));

    Ok(Self {
      token,
      ground,
      machine,
      routes,
      places: None,
      herdr: None,
      going,
      sessions: None,
    })
  }

  /// The session contract the front carries over the places, where it stands them.
  pub fn sessions(&self) -> Option<crate::herdr_backend::Shared> {
    self.sessions.clone()
  }

  /// The front with the workers' places the server stands, the session contract carried over them, and the
  /// picture of herdr it keeps.
  pub fn with_places(mut self, places: Places, herdr: Option<herdr::picture::Watch>) -> Self {
    let sessions = Arc::new(crate::herdr_backend::HerdrSessions::new(places.clone())).shared_as();
    self.sessions = Some(sessions);
    self.places = Some(places);
    self.herdr = herdr;
    self
  }

  /// Whom a request comes from: the control token's holder, or, on a route that takes a run's session token,
  /// the holder of the record the tool host keeps for the token it carries; none for any other request.
  async fn bearer(&self, authorization: Option<String>, route: Option<&dyn Answers>) -> Option<Bearer> {
    let given = authorization?.strip_prefix("Bearer ")?.to_owned();

    if given == self.token {
      return Some(Bearer::Control);
    }

    if given.is_empty() || !route.is_some_and(|route| route.takes_run_token()) {
      return None;
    }

    crate::lines::token::holder_of(&self.ground, &given).await.map(Bearer::Run)
  }
}

async fn dispatch(State(front): State<Arc<Front>>, request: Request) -> Response {
  let found = front
    .routes
    .get(request.method())
    .and_then(|routes| routes.at(request.uri().path()).ok())
    .map(|found| {
      let params: Vec<(String, String)> =
        found.params.iter().map(|(key, value)| (key.to_owned(), value.to_owned())).collect();

      (*found.value, params)
    });
  let authorization = request
    .headers()
    .get(header::AUTHORIZATION)
    .and_then(|value| value.to_str().ok())
    .map(str::to_owned);
  let Some(bearer) = front.bearer(authorization, found.as_ref().map(|(route, _)| *route)).await else {
    return Refused::new(StatusCode::UNAUTHORIZED, "unauthorized").into_response();
  };

  // A server that drains takes no new call; its caller asks again once the next server stands
  // (docs/api.md, 「health」, the drain).
  if front.ground.drain.draining() {
    return draining();
  }

  let Some((route, params)) = found else {
    return Refused::new(StatusCode::NOT_FOUND, "not found").into_response();
  };
  let mut call = Call {
    request,
    ground: front.ground.clone(),
    machine: front.machine.clone(),
    herdr: front.herdr.clone(),
    places: front.places.clone(),
    going: front.going.clone(),
    sessions: front.sessions.clone(),
    params,
    bearer,
  };

  let method = route.method();

  if let Some(ask) = crate::access::asked(&call, &method, route.path())
    && !crate::access::unit_repository(&mut call, &method, route.path()).await
    && let Err(refused) = crate::access::check(ask).await
  {
    return refused.into_response();
  }

  let owning = match crate::access::owning(&call, &method, route.path()) {
    Ok(owning) => owning,
    Err(refused) => return refused.into_response(),
  };

  if let Some(owning) = owning
    && let Err(refused) = crate::access::judge(owning).await
  {
    return refused.into_response();
  }

  match route.answer(call).await {
    Ok(response) => response,
    Err(refused) => refused.into_response(),
  }
}

// A call refused because the server drains, with when to ask again.
fn draining() -> Response {
  let mut refused =
    Refused::new(StatusCode::SERVICE_UNAVAILABLE, "agency is stopping; ask again once it stands")
      .with_code(crate::drain::STOPPING)
      .into_response();

  if let Ok(value) = axum::http::HeaderValue::from_str(&crate::drain::retry_after().to_string()) {
    refused.headers_mut().insert(axum::http::header::RETRY_AFTER, value);
  }

  refused
}

/// The front's router: every request goes through the one dispatch.
pub fn router(front: Front) -> Router {
  Router::new().fallback(dispatch).with_state(Arc::new(front))
}

/// The control port bound on the loopback, asked for again every half second while another process
/// holds it: the process before it keeps it until a deployment restarts it.
pub async fn bind(port: u16) -> io::Result<TcpListener> {
  let mut said = false;

  loop {
    match TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await {
      Ok(listener) => return Ok(listener),
      Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
        if !said {
          eprintln!("agency: the control port {port} is held; asking again until it is free");
          said = true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
      }
      Err(error) => return Err(error),
    }
  }
}

/// Serves the front on a listener until the drain the ground carries ends it (docs/sessions.md, 「A restart
/// hands the session over」): from the drain on no connection is taken, the event streams and the turns'
/// sends are answered at once, and the calls in hand are given the drain's span; whatever still runs then is
/// let go of with the process. Every event published by then is written to the store, given the span again,
/// so a reader resuming on the next server misses none.
pub async fn serve(listener: TcpListener, front: Front) -> io::Result<()> {
  let drain = front.ground.drain.clone();
  let events = front.ground.events.clone();
  let served = axum::serve(listener, router(front)).with_graceful_shutdown({
    let drain = drain.clone();

    async move { drain.begun().await }
  });
  let served = tokio::select! {
    served = served => served,
    () = drain.spent() => {
      eprintln!("agency: calls still in hand at the drain's end are let go of");
      Ok(())
    }
  };

  if tokio::time::timeout(crate::drain::short_calls(), events.flushed()).await.is_err() {
    eprintln!("agency: events the store had not taken by the drain's end are let go of");
  }

  served
}
