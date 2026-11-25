//! A loopback tap dsh's OpenRouter route points at for the length of one run (docs/cache-fallback.md,
//! 「OpenRouter warms only when the actual upstream it rode has a cache」). dsh reads the gateway's stream
//! for the answer and prints only the answer, so which upstream took the turn and what the turn cost are
//! lost on its side; the tap hands every byte on unchanged, both ways, and reads the answer as it goes by.
//! A pin writes one upstream into the request as the gateway's own provider preference. The key travels in
//! the request's own header and is never read, kept or logged here.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use futures_util::{Stream, StreamExt};
use serde_json::{Value, json};
use sse_stream::SseStream;
use tokio::sync::{mpsc, oneshot};

// The path dsh's route asks under, which the gateway answers under too.
const BASE_PATH: &str = "/api/v1";

// A non-streamed answer is read whole to find its usage; one longer than this is passed on unread.
const MAX_READ_CHARS: usize = 4_000_000;

/// One request's figures, in agency's four disjoint kinds and the dollars the gateway charged.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
  pub input_tokens: i64,
  pub output_tokens: i64,
  pub cache_read_tokens: i64,
  pub cache_write_tokens: i64,
  pub cost: f64,
}

impl Usage {
  fn added(self, other: Self) -> Self {
    Self {
      input_tokens: self.input_tokens + other.input_tokens,
      output_tokens: self.output_tokens + other.output_tokens,
      cache_read_tokens: self.cache_read_tokens + other.cache_read_tokens,
      cache_write_tokens: self.cache_write_tokens + other.cache_write_tokens,
      cost: self.cost + other.cost,
    }
  }
}

/// What the tap saw of a run: the upstream that answered its latest request, and every request added
/// together and the latest alone, none where the answer carried no usage at all.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Observation {
  pub upstream: Option<String>,
  pub total: Option<Usage>,
  pub last: Option<Usage>,
}

/// Where a request is to go: one upstream, with or without the gateway's fallback to the others.
#[derive(Clone, Debug)]
pub struct Pin {
  pub upstream: String,
  pub allow_fallbacks: bool,
}

// A count the gateway reported, where it is a positive number; nought otherwise.
fn count(value: Option<&Value>) -> i64 {
  value
    .and_then(Value::as_f64)
    .filter(|number| number.is_finite() && *number > 0.0)
    .map_or(0, |number| number.floor() as i64)
}

/// A request's usage as the gateway reports it, split into agency's kinds: the gateway counts cached and
/// cache-written tokens inside the prompt's, as OpenAI does, so the ordinary input is what remains.
pub fn usage_of(usage: &Value) -> Usage {
  let prompt = count(usage.get("prompt_tokens"));
  let details = usage.get("prompt_tokens_details");
  let cache_read_tokens = count(details.and_then(|details| details.get("cached_tokens")));
  let cache_write_tokens = count(details.and_then(|details| details.get("cache_write_tokens")));

  Usage {
    input_tokens: (prompt - cache_read_tokens - cache_write_tokens).max(0),
    output_tokens: count(usage.get("completion_tokens")),
    cache_read_tokens,
    cache_write_tokens,
    cost: usage
      .get("cost")
      .and_then(Value::as_f64)
      .filter(|cost| cost.is_finite() && *cost > 0.0)
      .unwrap_or(0.0),
  }
}

#[derive(Clone)]
struct Tapping {
  target: String,
  pin: Option<Pin>,
  seen: Arc<Mutex<Observation>>,
  client: reqwest::Client,
}

impl Tapping {
  // One frame the gateway wrote, noted where it names its upstream or carries a usage.
  fn note(&self, frame: &Value) {
    let Ok(mut seen) = self.seen.lock() else { return };

    if let Some(upstream) = frame.get("provider").and_then(Value::as_str).filter(|name| !name.is_empty()) {
      seen.upstream = Some(upstream.to_owned());
    }

    if let Some(usage) = frame.get("usage").filter(|usage| usage.is_object()) {
      let last = usage_of(usage);

      seen.total = Some(seen.total.map_or(last, |total| total.added(last)));
      seen.last = Some(last);
    }
  }

  fn note_text(&self, text: &str) {
    if let Ok(frame) = serde_json::from_str::<Value>(text) {
      self.note(&frame);
    }
  }
}

// A request's body with the pin written in as the gateway's provider preference; a body that is no JSON
// object goes as it came.
fn pinned(body: &Bytes, pin: &Pin) -> Bytes {
  let Ok(mut parsed) = serde_json::from_slice::<Value>(body) else {
    return body.clone();
  };
  let Some(object) = parsed.as_object_mut() else {
    return body.clone();
  };
  let mut provider = object.get("provider").and_then(Value::as_object).cloned().unwrap_or_default();

  provider.insert("order".to_owned(), json!([pin.upstream]));
  provider.insert("allow_fallbacks".to_owned(), json!(pin.allow_fallbacks));
  object.insert("provider".to_owned(), Value::Object(provider));

  Bytes::from(parsed.to_string())
}

fn headers_without(headers: &HeaderMap, dropped: &[&str]) -> HeaderMap {
  let mut kept = headers.clone();

  for name in dropped {
    kept.remove(*name);
  }

  kept
}

// A stream of what a channel is sent until every sender is gone.
fn received<T: Send + 'static>(rx: mpsc::UnboundedReceiver<T>) -> impl Stream<Item = T> + Send {
  futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) })
}

async fn forward(State(tapping): State<Tapping>, request: Request) -> Response {
  let path = request.uri().path().to_owned();
  let Some(rest) = path.strip_prefix(BASE_PATH) else {
    return Response::builder()
      .status(StatusCode::NOT_FOUND)
      .body(Body::empty())
      .unwrap_or_default();
  };
  let query = request.uri().query().map_or_else(String::new, |query| format!("?{query}"));
  let method = request.method().clone();
  let headers = headers_without(request.headers(), &["host", "content-length", "accept-encoding"]);
  let carries = method != axum::http::Method::GET && method != axum::http::Method::HEAD;
  let body = if carries {
    match axum::body::to_bytes(request.into_body(), usize::MAX).await {
      Ok(body) => Some(body),
      Err(_) => {
        return Response::builder()
          .status(StatusCode::BAD_REQUEST)
          .body(Body::empty())
          .unwrap_or_default();
      }
    }
  } else {
    None
  };
  let body = match (&body, &tapping.pin) {
    (Some(body), Some(pin)) if method == axum::http::Method::POST => Some(pinned(body, pin)),
    _ => body,
  };
  let mut asked = tapping
    .client
    .request(method, format!("{}{rest}{query}", tapping.target))
    .headers(headers);

  if let Some(body) = body {
    asked = asked.body(body);
  }

  let answer = match asked.send().await {
    Ok(answer) => answer,
    Err(why) => {
      return Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .body(Body::from(why.to_string()))
        .unwrap_or_default();
    }
  };
  let status = answer.status();
  let sent = headers_without(answer.headers(), &["content-encoding", "content-length"]);
  let streamed = answer
    .headers()
    .get("content-type")
    .and_then(|kind| kind.to_str().ok())
    .is_some_and(|kind| kind.contains("text/event-stream"));
  let (to_caller, for_caller) = mpsc::unbounded_channel::<Result<Bytes, std::io::Error>>();
  let reader = tapping.clone();

  tokio::spawn(async move {
    let mut chunks = answer.bytes_stream();
    let (to_events, for_events) = mpsc::unbounded_channel::<Result<Bytes, std::io::Error>>();
    // Server-sent events are read by the parser the workspace already carries, fed what passes by.
    let events = tokio::spawn({
      let reader = reader.clone();

      async move {
        let mut events = std::pin::pin!(SseStream::from_bytes_stream(received(for_events)));

        while let Some(Ok(event)) = events.next().await {
          if let Some(data) = event.data.as_deref().map(str::trim).filter(|data| *data != "[DONE]") {
            reader.note_text(data);
          }
        }
      }
    });
    let mut whole: Vec<u8> = vec![];

    while let Some(chunk) = chunks.next().await {
      let Ok(chunk) = chunk else {
        let _ = to_caller.send(Err(std::io::Error::other("the gateway's answer broke off")));
        break;
      };

      if streamed {
        let _ = to_events.send(Ok(chunk.clone()));
      } else if whole.len() < MAX_READ_CHARS {
        whole.extend_from_slice(&chunk);
      }

      let _ = to_caller.send(Ok(chunk));
    }

    // The answer is read to its end before the caller is handed that end, so what the run's caller asks
    // the tap once its answer is in is all there is.
    if streamed {
      // A stream that ends without the blank line after its last event still ends that event.
      let _ = to_events.send(Ok(Bytes::from_static(b"\n\n")));
      drop(to_events);
      let _ = events.await;
    } else {
      drop(to_events);
      events.abort();

      if whole.len() < MAX_READ_CHARS {
        reader.note_text(&String::from_utf8_lossy(&whole));
      }
    }

    drop(to_caller);
  });

  let mut response = Response::builder().status(status.as_u16());

  if let Some(held) = response.headers_mut() {
    for (name, value) in &sent {
      held.append(name, value.clone());
    }
  }

  response.body(Body::from_stream(received(for_caller))).unwrap_or_default()
}

/// The tap standing on a loopback port: where dsh's route is pointed, what it has seen so far, and its
/// end.
pub struct Tap {
  pub url: String,
  seen: Arc<Mutex<Observation>>,
  stop: Option<oneshot::Sender<()>>,
}

impl Tap {
  /// What the tap has seen of the run so far.
  pub fn observed(&self) -> Observation {
    self.seen.lock().map(|seen| seen.clone()).unwrap_or_default()
  }

  /// The tap taken down.
  pub fn close(mut self) {
    if let Some(stop) = self.stop.take() {
      let _ = stop.send(());
    }
  }
}

impl Drop for Tap {
  fn drop(&mut self) {
    if let Some(stop) = self.stop.take() {
      let _ = stop.send(());
    }
  }
}

/// A tap opened on a loopback port, pinned where a pin is given, handing what it is asked on to the
/// gateway's API at the address given.
pub async fn open(pin: Option<Pin>, target: &str) -> std::io::Result<Tap> {
  let target = target.trim_end_matches('/').to_owned();
  let seen = Arc::new(Mutex::new(Observation::default()));
  let client = reqwest::Client::builder().build().map_err(std::io::Error::other)?;
  let tapping = Tapping {
    target,
    pin,
    seen: seen.clone(),
    client,
  };
  let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
  let port = listener.local_addr()?.port();
  let (stop, stopped) = oneshot::channel::<()>();
  let router = axum::Router::new().fallback(forward).with_state(tapping);

  tokio::spawn(async move {
    let _ = axum::serve(listener, router)
      .with_graceful_shutdown(async {
        let _ = stopped.await;
      })
      .await;
  });

  Ok(Tap {
    url: format!("http://127.0.0.1:{port}{BASE_PATH}"),
    seen,
    stop: Some(stop),
  })
}
