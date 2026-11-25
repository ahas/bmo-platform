//! The gateway's front: the one address every run and every gate asks, which checks each request's key
//! and streams the request through to the LiteLLM child it serves from and each answer back as they
//! stand, reading of them only the model a request names and whether an answer is streamed. A
//! streamed answer that sends no byte for the stall span is cut as broken (docs/own-llm.md, 「A streamed
//! answer whose bytes stop is cut」). A new table is a new child: once it answers, new requests go to it,
//! and the old child is stopped only when the requests in hand through it have ended, however long they
//! take, so a saved change never cuts a request. A request on a registration
//! naming a card service whose engine stands nowhere asks the door for its card first, and is sent once
//! the granted card's entry is served. A streamed answer whose request names its program's session has its
//! open block counted as it passes (`streams`).
//!
//! The front takes the gateway's own key, the platform's, or a call key the store holds unrevoked, and
//! names the request's payer to the child; a call key reaches the request routes and the model lists alone,
//! a request of its whose model cannot be read is refused, and its request goes on under the gateway's own
//! key. A personal member answers its own person alone, a member or registration a person owns answers its
//! owner alone and the gateway's own key only where that owner is the administrator, whose own call key
//! is asked for no reservation, and every listing or description of models leaves out what a
//! call key does not reach while the gateway's own key lists the whole table; a person reaches the
//! platform's supply only on an open reservation (`supply`). What each request cost comes back from the
//! child's costs hook and is written down (`costs`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use harness_machine::card_master::{CardTerms, POOL_HEADER, RANK_HEADER};
use http_body_util::{BodyDataStream, BodyExt};
use hyper_util::rt::TokioIo;
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::costs::{Cost, Records};
use crate::door::{Door, Seat};
use crate::streams::{SESSION_HEADER, Streams};
use crate::supply::{Payer, Supply};
use crate::table::Owner;

/// The header the front names a request's payer to the child in, in place of any the caller sent.
pub const PAYER_HEADER: &str = "x-bmo-payer";

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One LiteLLM process serving one table, ready to be asked.
pub trait Child: Send + Sync {
  fn port(&self) -> u16;
  fn stop(&self) -> BoxFuture<'_, ()>;
}

/// What the console is told of the gateway: which table it serves, whether a new one is starting, and
/// how many old children are still finishing their requests.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Status {
  pub serving: Option<String>,
  pub starting: Option<String>,
  pub draining: usize,
}

struct Serving {
  table: String,
  /// Every model name the table holds an entry under.
  names: BTreeSet<String>,
  child: Box<dyn Child>,
  in_hand: AtomicUsize,
  retired: AtomicBool,
}

#[derive(Default)]
struct Held {
  current: Option<Arc<Serving>>,
  retired: Vec<Arc<Serving>>,
  starting: Option<String>,
}

pub struct Front {
  /// The gateway's own key, which every request carries as its Bearer.
  key: String,
  held: Mutex<Held>,
  /// Told each time a new child takes over, for the requests waiting on an entry to be served.
  taken: watch::Sender<u64>,
  /// Where cards are asked, where the arbiter's master is named.
  door: Option<Arc<Door>>,
  /// Each name of a person's own member or registration, with whom it answers, as the last beat read them.
  owners: Mutex<BTreeMap<String, Owner>>,
  /// Each registration naming a card service, with the service, as the registry last read says.
  card_services: Mutex<HashMap<String, String>>,
  /// How long a streamed answer may send no byte before it is cut.
  stall: Duration,
  /// The block each program session's streamed answer is writing.
  streams: Streams,
  /// The call keys, the personal supply and the reservations, as the store last read.
  supply: Supply,
  /// Where each request's cost is written down.
  records: Records,
  on_status: Box<dyn Fn(&Status) + Send + Sync>,
}

// The paths a model request comes on, whose body names its model: chat completions, the embeddings route and
// the Messages route.
const MODEL_PATHS: [&str; 5] = [
  "/v1/chat/completions",
  "/chat/completions",
  "/v1/embeddings",
  "/embeddings",
  "/v1/messages",
];

// The path Claude Code counts a request's tokens on, which names its model as a request does and asks no card.
const COUNT_PATH: &str = "/v1/messages/count_tokens";

// The paths that list or describe models, whose answer a key reads without the models it does not reach: the
// model list and LiteLLM's model information routes.
const LIST_PATHS: [&str; 6] = [
  "/v1/models",
  "/models",
  "/model/info",
  "/v1/model/info",
  "/v2/model/info",
  "/model_group/info",
];

// The paths that describe one model, the name after them, which a key it does not reach is refused.
const ONE_MODEL_PATHS: [&str; 2] = ["/v1/models/", "/models/"];

// The fields a listed model is named by, as the list and the information routes write it.
const LISTED_NAMES: [&str; 3] = ["id", "model_name", "model_group"];

// Whether a call key may ask the route: the request routes and the model lists alone; every other route of
// LiteLLM's, its writes and its spend and health reads, is the gateway's own key's.
fn call_key_route(method: &Method, path: &str) -> bool {
  match *method {
    Method::POST => MODEL_PATHS.contains(&path) || path == COUNT_PATH,
    Method::GET => {
      LIST_PATHS.contains(&path) || ONE_MODEL_PATHS.iter().any(|prefix| path.starts_with(prefix))
    }
    _ => false,
  }
}

// What the front reads of a model request: the model it names.
#[derive(Deserialize)]
struct Named {
  model: Option<String>,
}

// The hop-by-hop headers, which belong to one connection and are not handed on, and the ones a hop
// settles for itself.
const UNPASSED: [&str; 6] =
  ["host", "connection", "keep-alive", "transfer-encoding", "upgrade", "content-length"];

// A task taken down when its holder goes: the connection to a child, closed with the request it carries.
struct Owned(JoinHandle<()>);

impl Drop for Owned {
  fn drop(&mut self) {
    self.0.abort();
  }
}

// What a streamed answer is told by: its content type.
const EVENT_STREAM: &str = "text/event-stream";

// Whether an answer's headers say it is streamed.
fn streamed(headers: &header::HeaderMap) -> bool {
  headers
    .get(header::CONTENT_TYPE)
    .and_then(|kind| kind.to_str().ok())
    .and_then(|kind| kind.split(';').next())
    .is_some_and(|essence| essence.trim().eq_ignore_ascii_case(EVENT_STREAM))
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

// A request in hand through one child, let go when the answer ends however it ends.
struct InHand {
  front: Arc<Front>,
  serving: Arc<Serving>,
}

impl Drop for InHand {
  fn drop(&mut self) {
    self.serving.in_hand.fetch_sub(1, Ordering::SeqCst);
    let (front, serving) = (Arc::clone(&self.front), Arc::clone(&self.serving));
    // Outside a runtime — the process ending — nothing is left to stop it for.
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
      runtime.spawn(async move { front.stop_if_idle(&serving).await });
    }
  }
}

fn text(status: StatusCode, said: String) -> Response {
  (status, [(header::CONTENT_TYPE, "text/plain;charset=utf-8")], said).into_response()
}

// Two secrets compared in a time that depends on neither: each is hashed to the same length first, and
// every byte of the two hashes is looked at.
fn same_secret(given: &[u8], key: &[u8]) -> bool {
  let (given, key) = (Sha256::digest(given), Sha256::digest(key));
  let differ = given.iter().zip(key.iter()).fold(0u8, |differ, (a, b)| differ | (a ^ b));
  std::hint::black_box(differ) == 0
}

/// Whether a request carries the key as its Bearer: the scheme in any case, the key as it stands.
pub fn carries_key(headers: &header::HeaderMap, key: &str) -> bool {
  bearer(headers).is_some_and(|given| same_secret(given, key.as_bytes()))
}

// The key a request carries as its Bearer, the scheme in any case, trimmed.
fn bearer(headers: &header::HeaderMap) -> Option<&[u8]> {
  let said = headers.get(header::AUTHORIZATION).map(HeaderValue::as_bytes)?;
  let (scheme, given) = said.split_at_checked(7)?;
  scheme.eq_ignore_ascii_case(b"bearer ").then(|| given.trim_ascii())
}

// An error in the OpenAI shape, as LiteLLM's callers read one.
fn openai_error(status: StatusCode, message: &str, kind: &str, code: &str) -> Response {
  let body = serde_json::json!({
    "error": { "message": message, "type": kind, "code": code }
  });
  (status, [(header::CONTENT_TYPE, "application/json")], body.to_string()).into_response()
}

// A request naming a person's own member or registration under a key it does not answer.
fn not_yours() -> Response {
  openai_error(
    StatusCode::FORBIDDEN,
    "This model is an account's own and answers only that account's call key.",
    "permission_error",
    "not-your-member",
  )
}

// A call key's request to a route that is the gateway's own key's.
fn gateway_key_only() -> Response {
  openai_error(
    StatusCode::FORBIDDEN,
    "This route answers the gateway's own key alone; a call key reaches the request routes and the model lists.",
    "permission_error",
    "gateway-key-only",
  )
}

// A call key's request on a route that names a model whose model cannot be read, so neither its owner nor its
// supply can be judged.
fn model_unread() -> Response {
  openai_error(
    StatusCode::BAD_REQUEST,
    "The request's model could not be read, so it was not sent.",
    "invalid_request_error",
    "model-unread",
  )
}

// A request of a person's on the platform's supply with no reservation open.
fn no_supply() -> Response {
  openai_error(
    StatusCode::PAYMENT_REQUIRED,
    "This account holds no open reservation of the platform's supply, so the request was not sent.",
    "insufficient_quota",
    "no-supply",
  )
}

// A request without the key, refused in the OpenAI error shape before anything of it is handed on.
fn refused() -> Response {
  let body = serde_json::json!({
    "error": {
      "message": "The gateway's key is missing or wrong: send it as `Authorization: Bearer <key>`.",
      "type": "authentication_error",
      "code": "401",
    }
  });
  (
    StatusCode::UNAUTHORIZED,
    [(header::CONTENT_TYPE, "application/json"), (header::WWW_AUTHENTICATE, "Bearer")],
    body.to_string(),
  )
    .into_response()
}

// What a request names of its demand, for a card asked at the door.
fn terms_of(headers: &HeaderMap) -> CardTerms {
  let named = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
  CardTerms::of_headers(named(RANK_HEADER), named(POOL_HEADER))
}

impl Front {
  /// A front checking every request against the gateway's own key, which is never empty, and the call
  /// keys `supply` holds, asking its cards at the door where one is given, cutting a streamed answer silent
  /// for `stall`, and writing each request's cost into `records`.
  pub fn new(
    key: String,
    door: Option<Arc<Door>>,
    stall: Duration,
    supply: Supply,
    records: Records,
    on_status: impl Fn(&Status) + Send + Sync + 'static,
  ) -> Arc<Front> {
    assert!(!key.is_empty(), "the gateway's key is never empty");
    Arc::new(Front {
      key,
      held: Mutex::new(Held::default()),
      taken: watch::channel(0).0,
      door,
      owners: Mutex::new(BTreeMap::new()),
      card_services: Mutex::new(HashMap::new()),
      stall,
      streams: Streams::default(),
      supply,
      records,
      on_status: Box::new(on_status),
    })
  }

  /// The block each program session's streamed answer is writing.
  pub fn streams(&self) -> &Streams {
    &self.streams
  }

  /// The call keys, the personal supply and the reservations, as the store last read.
  pub fn supply(&self) -> &Supply {
    &self.supply
  }

  /// The registrations naming a card service, each with its service, as the registry now reads.
  pub fn know(&self, card_services: HashMap<String, String>) {
    *self.card_services.lock().unwrap() = card_services;
  }

  fn status_of(held: &Held) -> Status {
    Status {
      serving: held.current.as_ref().map(|serving| serving.table.clone()),
      starting: held.starting.clone(),
      draining: held.retired.len(),
    }
  }

  pub fn status(&self) -> Status {
    Front::status_of(&self.held.lock().unwrap())
  }

  fn report(&self) {
    let status = self.status();
    (self.on_status)(&status);
  }

  /// The names of the people's own members and registrations with whom each answers, which hold from now
  /// whatever table is served.
  pub fn own(&self, owners: BTreeMap<String, Owner>) {
    *self.owners.lock().unwrap() = owners;
  }

  /// The table the front is to serve, with the names it holds entries under. A table already served, or a
  /// start already under way, starts nothing; a new one starts its child, waits for it, and takes over from
  /// the old one, which drains.
  pub async fn take(
    &self,
    table: &str,
    names: BTreeSet<String>,
    start: impl Future<Output = Result<Box<dyn Child>, String>>,
  ) -> Result<(), String> {
    {
      let mut held = self.held.lock().unwrap();
      if held.current.as_ref().is_some_and(|serving| serving.table == table) || held.starting.is_some() {
        return Ok(());
      }
      held.starting = Some(table.to_owned());
    }
    self.report();

    let started = start.await;
    self.held.lock().unwrap().starting = None;
    self.report();
    let child = started?;

    let old = {
      let mut held = self.held.lock().unwrap();
      let old = held.current.replace(Arc::new(Serving {
        table: table.to_owned(),
        names,
        child,
        in_hand: AtomicUsize::new(0),
        retired: AtomicBool::new(false),
      }));
      if let Some(old) = &old {
        old.retired.store(true, Ordering::SeqCst);
        held.retired.push(Arc::clone(old));
      }
      old
    };
    self.taken.send_modify(|taken| *taken += 1);
    if let Some(old) = old {
      self.stop_if_idle(&old).await;
    }
    self.report();
    Ok(())
  }

  // Who pays for a request: the platform on the gateway's own key, the account a call key is, or none, which
  // is refused.
  async fn payer_of(&self, headers: &HeaderMap) -> Option<Payer> {
    if carries_key(headers, &self.key) {
      return Some(Payer::Platform);
    }
    let given = std::str::from_utf8(bearer(headers)?).ok().filter(|given| !given.is_empty())?;
    self.supply.account_of(given).await.map(Payer::Account)
  }

  // Why a request naming `model` is not sent, where it is not: a person's own member or registration under
  // a key it does not answer, or a person on the platform's supply with no reservation open while a supply
  // hook stands.
  async fn withheld(&self, payer: Payer, model: &str) -> Option<Response> {
    let owner = self.owner_of(model);
    if let Some(Owner::Organisation(unit)) = owner {
      return match payer {
        Payer::Account(account) if self.supply.supplied_by_unit(account, unit).await => None,
        _ => Some(not_yours()),
      };
    }
    if owner.is_some_and(|owner| !owner.admits(payer, self.supply.administrator())) {
      return Some(not_yours());
    }
    match self.judged(payer) {
      Payer::Account(account) if owner.is_none() && !self.supply.supplied(account).await => {
        Some(no_supply())
      }
      _ => None,
    }
  }

  // Whether any table stands.
  fn stands(&self) -> bool {
    self.held.lock().unwrap().current.is_some()
  }

  // Whether the child serving now holds an entry under `name`; none where no table stands.
  fn serves(&self, name: &str) -> Option<bool> {
    let held = self.held.lock().unwrap();
    held.current.as_ref().map(|serving| serving.names.contains(name))
  }

  // Until the child serving holds an entry under `name`: the launcher lays a standing instance into a new
  // table on its beat, and a new child takes over once it answers.
  async fn until_served(&self, name: &str) {
    let mut taken = self.taken.subscribe();
    while self.serves(name) != Some(true) {
      if taken.changed().await.is_err() {
        return;
      }
    }
  }

  // The registration naming a card service a model name stands for, with the node it pins, if any.
  fn card_registration(&self, model: &str) -> Option<(String, String, Option<String>)> {
    let card_services = self.card_services.lock().unwrap();
    if let Some(service) = card_services.get(model) {
      return Some((model.to_owned(), service.clone(), None));
    }
    let (name, node) = model.rsplit_once('@')?;
    let service = card_services.get(name)?;
    Some((name.to_owned(), service.clone(), Some(node.to_owned())))
  }

  // A model request whose model names a registration naming a card service under a name the table serving
  // now holds no entry for, its card asked at the door and waited for until the granted card's entry is
  // served. Answered with the body to send, the model renamed where a pinned node is not the one granted,
  // and the seat the request holds the card by; or with the status and line to answer where the master
  // refuses the card.
  async fn carded(
    &self,
    door: &Arc<Door>,
    bytes: Bytes,
    model: &str,
    terms: &CardTerms,
  ) -> Result<(Bytes, Option<Seat>), (StatusCode, String)> {
    if self.serves(model) != Some(false) {
      return Ok((bytes, None));
    }
    let Some((registration, service, pinned)) = self.card_registration(model) else {
      return Ok((bytes, None));
    };

    let (seat, node) = door
      .card(&service, terms)
      .await
      .map_err(|refused| (StatusCode::SERVICE_UNAVAILABLE, refused))?;
    self.until_served(&format!("{registration}@{node}")).await;

    if pinned.is_none_or(|pinned| pinned == node) {
      return Ok((bytes, Some(seat)));
    }
    let mut named: Value = serde_json::from_slice(&bytes).expect("a body that named its model reads");
    named["model"] = Value::String(registration);
    Ok((Bytes::from(serde_json::to_vec(&named).expect("a body read is written")), Some(seat)))
  }

  /// One request, refused without the key, else carried to the child serving now and its answer
  /// carried back as it comes.
  pub async fn handle(self: &Arc<Self>, request: Request) -> Response {
    let Some(payer) = self.payer_of(request.headers()).await else {
      return refused();
    };
    let (parts, body) = request.into_parts();
    let session = parts
      .headers
      .get(SESSION_HEADER)
      .and_then(|named| named.to_str().ok())
      .map(str::to_owned);
    if payer != Payer::Platform && !call_key_route(&parts.method, parts.uri.path()) {
      return gateway_key_only();
    }
    let carding = parts.method == Method::POST && MODEL_PATHS.contains(&parts.uri.path());
    let naming = carding || parts.method == Method::POST && parts.uri.path() == COUNT_PATH;
    let listing = parts.method == Method::GET && LIST_PATHS.contains(&parts.uri.path());
    let described = (parts.method == Method::GET)
      .then(|| ONE_MODEL_PATHS.iter().find_map(|path| parts.uri.path().strip_prefix(path)))
      .flatten()
      .map(|named| percent_decode_str(named).decode_utf8_lossy().into_owned());
    if payer != Payer::Platform && described.is_some_and(|model| !self.reaches(payer, &model)) {
      return not_yours();
    }
    // A model request's body is read whole for the model it names, which a card is asked by and a cut
    // stream is told by.
    let (body, model, seat) = if naming {
      let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(fault) => {
          return text(StatusCode::BAD_REQUEST, format!("the request's body could not be read: {fault}"));
        }
      };
      let model = serde_json::from_slice::<Named>(&bytes).ok().and_then(|named| named.model);
      let withheld = match model.as_deref() {
        Some(named) => self.withheld(payer, named).await,
        None if payer != Payer::Platform => Some(model_unread()),
        None => None,
      };
      if let Some(refusal) = withheld {
        return refusal;
      }
      let door = self.door.as_ref().filter(|_| carding && self.stands());
      match (door, model.as_deref()) {
        (Some(door), Some(named)) => match self.carded(door, bytes, named, &terms_of(&parts.headers)).await
        {
          Ok((bytes, seat)) => (Body::from(bytes), model, seat),
          Err((status, said)) => return text(status, said),
        },
        _ => (Body::from(bytes), model, None),
      }
    } else {
      (body, None, None)
    };
    let request = Request::from_parts(parts, body);
    // Counted in hand under the lock the child is taken under, so a take that retires that child in
    // between never finds it idle and stops it under the request.
    let serving = {
      let held = self.held.lock().unwrap();
      let Some(serving) = held.current.clone() else {
        return text(StatusCode::SERVICE_UNAVAILABLE, "the gateway has no table to serve yet".to_owned());
      };
      serving.in_hand.fetch_add(1, Ordering::SeqCst);
      serving
    };
    let in_hand = InHand {
      front: Arc::clone(self),
      serving: Arc::clone(&serving),
    };
    let port = serving.child.port();
    let table = serving.table.clone();

    let (mut parts, body) = request.into_parts();
    for name in UNPASSED {
      parts.headers.remove(name);
    }
    // The payer is the front's to name, in place of any the caller sent, and a request on a call key goes on
    // under the gateway's own key, which LiteLLM's own check holds.
    parts
      .headers
      .insert(PAYER_HEADER, HeaderValue::from_str(&payer.named()).expect("a payer is a plain word"));
    if payer != Payer::Platform {
      parts.headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", self.key))
          .expect("the gateway's key is a header's value"),
      );
    }
    // The answer goes back byte for byte, so it is asked for as it is written.
    parts.headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    parts
      .headers
      .insert(header::HOST, HeaderValue::from_str(&format!("127.0.0.1:{port}")).expect("an address"));
    let target = parts.uri.path_and_query().map_or("/", |at| at.as_str()).to_owned();
    parts.uri = target.parse().expect("a path and query read off a request");
    let upstream = hyper::Request::from_parts(parts, body);

    // No wait of the front's own: a slow card may write nothing for minutes, and the request's own wait
    // is the child's to keep.
    let asked = async {
      let stream = TcpStream::connect(("127.0.0.1", port)).await.map_err(|fault| fault.to_string())?;
      let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|fault| fault.to_string())?;
      let connection = Owned(tokio::spawn(async move {
        let _ = connection.await;
      }));
      let answer = sender.send_request(upstream).await.map_err(|fault| fault.to_string())?;
      Ok::<_, String>((answer, connection))
    };
    let (answer, connection) = match asked.await {
      Ok(asked) => asked,
      Err(fault) => {
        drop(in_hand);
        return text(StatusCode::BAD_GATEWAY, format!("the gateway could not reach its table: {fault}"));
      }
    };

    let (mut parts, body) = answer.into_parts();
    for name in UNPASSED.iter().chain(&["content-encoding"]) {
      parts.headers.remove(*name);
    }
    if listing && payer != Payer::Platform {
      let listed = self.listed(payer, body).await;
      drop((in_hand, connection, seat));
      return match listed {
        Ok(listed) => Response::from_parts(parts, listed),
        Err(fault) => text(StatusCode::BAD_GATEWAY, format!("the model list could not be read: {fault}")),
      };
    }
    // The request stays in hand, and its connection to the child open, until its answer has been
    // carried whole or its caller has gone, or, streamed, has gone silent for the stall span.
    let kept = (in_hand, connection, seat);
    if !streamed(&parts.headers) {
      let carried = BodyDataStream::new(body).map(move |chunk| {
        let _ = &kept;
        chunk
      });
      return Response::from_parts(parts, Body::from_stream(carried));
    }
    let watched = Watched {
      body: BodyDataStream::new(body),
      kept: Some(Box::new(kept)),
      counter: session
        .filter(|session| !session.is_empty())
        .map(|session| self.streams.counter(session)),
      stall: self.stall,
      model: model.unwrap_or_else(|| "(none named)".to_owned()),
      table,
      port,
    };
    Response::from_parts(parts, Body::from_stream(watched.relay()))
  }

  /// Every child stopped, the one serving and the ones draining.
  pub async fn close(&self) {
    let all: Vec<Arc<Serving>> = {
      let mut held = self.held.lock().unwrap();
      let mut all: Vec<Arc<Serving>> = std::mem::take(&mut held.retired);
      all.extend(held.current.take());
      all
    };
    futures_util::future::join_all(all.iter().map(|serving| serving.child.stop())).await;
    self.report();
  }

  async fn stop_if_idle(&self, serving: &Arc<Serving>) {
    {
      let mut held = self.held.lock().unwrap();
      if !serving.retired.load(Ordering::SeqCst) || serving.in_hand.load(Ordering::SeqCst) > 0 {
        return;
      }
      let Some(at) = held.retired.iter().position(|one| Arc::ptr_eq(one, serving)) else {
        return;
      };
      held.retired.remove(at);
    }
    serving.child.stop().await;
    self.report();
  }
}

impl Front {
  // The payer a request's supply is judged as: the administrator's call key as the gateway's own key, asked
  // for no reservation, since the administrator's runs are the deployment's own; every other as it is. Whose
  // a model is stays judged by the key itself, which reaches the administrator's own names.
  fn judged(&self, payer: Payer) -> Payer {
    match payer {
      Payer::Account(account) if self.supply.administrator() == Some(account) => Payer::Platform,
      _ => payer,
    }
  }

  // Whom a model name answers where it is a person's own: the name's own entry, else, for `<name>@<node>`
  // not served yet, its registration's.
  fn owner_of(&self, model: &str) -> Option<Owner> {
    let owners = self.owners.lock().unwrap();
    owners.get(model).or_else(|| owners.get(model.rsplit_once('@')?.0)).copied()
  }

  // Whether a request its payer sends reaches the model name: one no person owns always does.
  fn reaches(&self, payer: Payer, model: &str) -> bool {
    self
      .owner_of(model)
      .is_none_or(|owner| owner.admits(payer, self.supply.administrator()))
  }

  // A listing or description of models as the child answered it, read whole, without the models the payer
  // does not reach; an answer that holds no list goes back as it came.
  async fn listed(&self, payer: Payer, body: hyper::body::Incoming) -> Result<Body, hyper::Error> {
    let bytes = body.collect().await?.to_bytes();
    let Ok(mut listed) = serde_json::from_slice::<Value>(&bytes) else {
      return Ok(Body::from(bytes));
    };
    let Some(models) = listed.get_mut("data").and_then(Value::as_array_mut) else {
      return Ok(Body::from(bytes));
    };
    models.retain(|model| {
      LISTED_NAMES
        .iter()
        .find_map(|field| model.get(*field).and_then(Value::as_str))
        .is_none_or(|name| self.reaches(payer, name))
    });
    Ok(Body::from(serde_json::to_vec(&listed).expect("a list read is written")))
  }
}

// A streamed answer carried back while its bytes are watched: from its headers on, a wait of the stall
// span with no byte ends it with an error the caller reads as a broken connection, and lets go of what the
// request held, its connection to the child first.
struct Watched {
  body: BodyDataStream<hyper::body::Incoming>,
  kept: Option<Box<dyn Send>>,
  /// Where its bytes are handed to be counted for its session's open block, where its request named one.
  counter: Option<tokio::sync::mpsc::UnboundedSender<Bytes>>,
  stall: Duration,
  model: String,
  table: String,
  port: u16,
}

impl Watched {
  fn relay(self) -> impl futures_util::Stream<Item = Result<Bytes, BoxError>> + Send {
    futures_util::stream::unfold((self, Instant::now()), |(mut watched, last)| async move {
      watched.kept.as_ref()?;
      match tokio::time::timeout(watched.stall, watched.body.next()).await {
        Ok(Some(chunk)) => {
          if let (Ok(bytes), Some(counter)) = (&chunk, &watched.counter) {
            let _ = counter.send(bytes.clone());
          }

          Some((chunk.map_err(BoxError::from), (watched, Instant::now())))
        }
        Ok(None) => None,
        Err(_) => {
          let silent = last.elapsed();
          watched.kept = None;
          watched.counter = None;
          eprintln!(
            "llm-gateway: model {} from table {} (port {}) sent no byte for {:.1}s; its stream was cut",
            watched.model,
            watched.table,
            watched.port,
            silent.as_secs_f64()
          );
          let cut = std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("the answer sent no byte for {:.1}s", silent.as_secs_f64()),
          );
          Some((Err(BoxError::from(cut)), (watched, last)))
        }
      }
    })
  }
}

/// The block a program session's streamed answer is writing, behind the key; 404 where none is open.
async fn open_block(
  State(front): State<Arc<Front>>,
  axum::extract::Path(session): axum::extract::Path<String>,
  headers: HeaderMap,
) -> Response {
  if !carries_key(&headers, &front.key) {
    return refused();
  }

  match front.streams.open(&session) {
    Some(block) => axum::Json(block).into_response(),
    None => StatusCode::NOT_FOUND.into_response(),
  }
}

/// One request's cost, as the child's costs hook hands it, written down; only the gateway's own key is taken,
/// so no caller writes a record.
async fn cost(State(front): State<Arc<Front>>, headers: HeaderMap, body: Bytes) -> Response {
  if !carries_key(&headers, &front.key) {
    return refused();
  }
  let cost: Cost = match serde_json::from_slice(&body) {
    Ok(cost) => cost,
    Err(fault) => return text(StatusCode::BAD_REQUEST, format!("the cost does not read: {fault}")),
  };

  front.records.write(&cost).await;
  StatusCode::NO_CONTENT.into_response()
}

/// Every path and method comes through the front, and through its check of the key; the gateway's own
/// routes, the reading of an open block and the costs the child hands back, stand under `/gateway/`, which no
/// route of the child's takes, and take the gateway's own key alone.
pub fn router(front: Arc<Front>) -> Router {
  Router::new()
    .route("/gateway/streams/{session}", axum::routing::get(open_block))
    .route("/gateway/costs", axum::routing::post(cost))
    .fallback(
      |State(front): State<Arc<Front>>, request: Request| async move { front.handle(request).await },
    )
    .with_state(front)
}
