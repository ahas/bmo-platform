//! The events agency publishes as things happen (docs/api.md, 「Events」). Each kind of event is a type
//! of its own that names itself; a consumer subscribes to the kinds it reads and takes each from its own
//! stream at its own pace. Publishing never waits on a subscriber and reads nothing back, and an event
//! with no subscriber reaches nobody. A consumer across the boundary reads the same events by name, each as
//! its JSON, from a stream of the kinds it names (docs/api.md, 「Events」). Once the bus is made durable on
//! the store, every event the stream carries is kept there under a number that keeps rising across the
//! service's restarts, so a reader resumes after the last number it read, and is told where what it asks
//! is no longer kept (docs/api.md, 「events」).

use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use sea_orm::DatabaseConnection;
use store::{Fault, all, one, run};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;

use crate::queue::settings;

/// One kind of event, with the name a consumer knows it by.
pub trait Event: Clone + Send + serde::Serialize + 'static {
  const NAME: &'static str;
}

type Senders = Vec<Box<dyn Any + Send>>;

// How many events the bus keeps in memory for a reader that asks for a page of them.
const KEPT: usize = 1000;

/// How many events the store keeps: the newest, an older one dropped as newer ones come.
pub const STORED: i64 = 50_000;

// How many numbers a process writes down ahead of those it gives, and how many events go in between two
// drops of the oldest.
const RESERVE: i64 = 100_000;
const DROP_EVERY: u64 = 1_000;

// How many events one write to the store takes at most.
const WRITTEN_AT_ONCE: usize = 1_000;

// The settings rows the numbering is kept in (docs/store.md, 「The settings table」).
const RESERVED_KEY: &str = "event_numbers_reserved";
const DROPPED_KEY: &str = "events_dropped_through";

/// The kind of the stream's own notice to a reader resuming after a number whose events are no longer kept.
pub const MISSED: &str = "events.missed";

// The kinds agency keeps to its own processes, which the boundary does not publish.
const INTERNAL: &[&str] = &[
  "supervisor-run-going",
  "herdr-leftovers",
  "machine-pressure",
  "nodes-moved",
  "open-items",
  "pane-claims",
  "road-verbs",
  "room-title",
  "run-recovered",
  "system-prompt-changed",
  "typed-line",
  "worker-words",
];

/// One event as a consumer across the boundary reads it: its number, its kind's name, when it happened and
/// its JSON.
#[derive(Clone, Debug, PartialEq)]
pub struct Named {
  pub id: i64,
  pub at: i64,
  pub name: Cow<'static, str>,
  pub body: serde_json::Value,
}

impl Named {
  /// The event as the stream and the page carry it: the envelope around the member.
  pub fn envelope(&self) -> serde_json::Value {
    serde_json::json!({ "id": self.id, "kind": self.name, "at": self.at, "data": self.body })
  }
}

// One stream of named events, and the kinds it reads; none reads every kind.
struct Wire {
  kinds: Option<Vec<String>>,
  sender: UnboundedSender<Named>,
}

impl Wire {
  fn reads(&self, name: &str) -> bool {
    self.kinds.as_ref().is_none_or(|kinds| kinds.iter().any(|kind| kind == name))
  }
}

/// Every subscription, by the kind of event it reads. Cloned freely: every clone is the one bus.
#[derive(Clone, Default)]
pub struct Bus {
  subscribers: Arc<Mutex<HashMap<TypeId, Senders>>>,
  wires: Arc<Mutex<Vec<Wire>>>,
  // The number the next event takes, and the events kept for a page.
  next: Arc<AtomicI64>,
  history: Arc<Mutex<VecDeque<Named>>>,
  // The highest number no longer kept: dropped from the store, or from memory on a bus with no store.
  dropped: Arc<AtomicI64>,
  // Where every event goes to be kept, once the bus is durable.
  kept: Arc<OnceLock<Kept>>,
}

// The store a durable bus keeps its events in, and the writer that keeps them in order.
struct Kept {
  db: DatabaseConnection,
  writer: UnboundedSender<Write>,
}

enum Write {
  Event(Named),
  Flushed(oneshot::Sender<()>),
}

/// What a reader resuming after a number is sent: the kept events after it, of the kinds it reads, whether
/// there are more after those, and the notice where events after it are no longer kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Replay {
  pub events: Vec<Named>,
  pub more: bool,
  pub missed: Option<Missed>,
}

/// The notice that what a reader asked for is no longer kept: the number it named, and the oldest kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Missed {
  pub after: i64,
  pub kept_from: Option<i64>,
}

impl Missed {
  /// The notice's data, as the stream and the page carry it.
  pub fn data(&self) -> serde_json::Value {
    serde_json::json!({ "after": self.after, "keptFrom": self.kept_from })
  }
}

/// One consumer's stream of one kind of event, from the moment it subscribed. Dropping it ends the
/// subscription.
pub struct Subscription<E> {
  events: UnboundedReceiver<E>,
}

impl<E: Event> Subscription<E> {
  /// The next event, waiting for one; none once the bus is gone.
  pub async fn next(&mut self) -> Option<E> {
    self.events.recv().await
  }

  /// The next event where one is waiting, without waiting for one.
  pub fn ready(&mut self) -> Option<E> {
    self.events.try_recv().ok()
  }
}

/// A stream of named events, of the kinds asked, from the moment it subscribed. Dropping it ends it.
pub struct WireSubscription {
  events: UnboundedReceiver<Named>,
}

impl WireSubscription {
  /// The next event, waiting for one; none once the bus is gone.
  pub async fn next(&mut self) -> Option<Named> {
    self.events.recv().await
  }

  /// The next event where one is waiting, without waiting for one.
  pub fn ready(&mut self) -> Option<Named> {
    self.events.try_recv().ok()
  }

  /// The stream itself, for a reader that takes it as a stream.
  pub fn into_receiver(self) -> UnboundedReceiver<Named> {
    self.events
  }
}

impl Bus {
  fn held(&self) -> MutexGuard<'_, HashMap<TypeId, Senders>> {
    self.subscribers.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  /// A subscription to every event of one kind published from now on.
  pub fn subscribe<E: Event>(&self) -> Subscription<E> {
    let (sender, events) = unbounded_channel::<E>();

    self.held().entry(TypeId::of::<E>()).or_default().push(Box::new(sender));

    Subscription { events }
  }

  /// A stream of the named events of the kinds given, every kind where none is given.
  pub fn subscribe_wire(&self, kinds: Option<Vec<String>>) -> WireSubscription {
    let (sender, events) = unbounded_channel();

    self
      .wires
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .push(Wire { kinds, sender });

    WireSubscription { events }
  }

  /// The event handed to every subscriber of its kind, and to nobody where there is none. A subscription
  /// dropped since is let go here.
  pub fn publish<E: Event>(&self, event: E) {
    self.publish_wire(&event);

    let mut subscribers = self.held();
    let Some(senders) = subscribers.get_mut(&TypeId::of::<E>()) else {
      return;
    };

    senders.retain(|sender| {
      sender
        .downcast_ref::<UnboundedSender<E>>()
        .is_some_and(|sender| sender.send(event.clone()).is_ok())
    });
  }

  // The event handed, as its JSON, to every stream reading its kind; serialised only where one does.
  fn publish_wire<E: Event>(&self, event: &E) {
    if INTERNAL.contains(&E::NAME) {
      return;
    }

    if let Ok(body) = serde_json::to_value(event) {
      self.publish_named(Named {
        id: 0,
        at: 0,
        name: Cow::Borrowed(E::NAME),
        body,
      });
    }
  }

  /// An event known by its name and its JSON handed to every stream reading its kind, as an event another
  /// process of agency's published reaches this one's streams (docs/api.md, 「Events」). Only the streams
  /// read it: no subscriber of a kind's own type does.
  pub fn publish_named(&self, event: Named) {
    let event = Named {
      id: self.next.fetch_add(1, Ordering::SeqCst) + 1,
      at: crate::now_ms(),
      ..event
    };

    {
      let mut history = self.history.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
      history.push_back(event.clone());
      while history.len() > KEPT {
        // On a bus with no store, what memory lets go of is no longer kept anywhere.
        if let Some(gone) = history.pop_front()
          && self.kept.get().is_none()
        {
          self.dropped.fetch_max(gone.id, Ordering::SeqCst);
        }
      }
    }

    if let Some(kept) = self.kept.get() {
      let _ = kept.writer.send(Write::Event(event.clone()));
    }

    let wires = self.wires.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    for wire in wires.iter().filter(|wire| wire.reads(&event.name)) {
      let _ = wire.sender.send(event.clone());
    }
  }

  /// The bus made durable on the store, before anything publishes: its numbers go on above every number a
  /// process before it gave or wrote down ahead, the next numbers are written down ahead of being given,
  /// and every event from now on is kept in the store, the newest `stored` of them. A bus already durable
  /// stays as it is.
  pub async fn durable(&self, db: &DatabaseConnection, stored: i64) -> Result<(), Fault> {
    if self.kept.get().is_some() {
      return Ok(());
    }

    let number = |value: Option<String>| value.and_then(|value| value.parse::<i64>().ok()).unwrap_or(0);
    let reserved = number(settings::get(db, RESERVED_KEY).await.map_err(fault)?);
    let dropped = number(settings::get(db, DROPPED_KEY).await.map_err(fault)?);
    let highest = one(db, r#"SELECT COALESCE(MAX("id"), 0) AS "id" FROM "stream_events""#, vec![])
      .await?
      .map(|row| row.try_get::<i64>("", "id"))
      .transpose()?
      .unwrap_or(0);
    let start = reserved.max(highest).max(self.next.load(Ordering::SeqCst));
    let ahead = start + RESERVE;

    settings::set(db, RESERVED_KEY, &ahead.to_string()).await.map_err(fault)?;
    self.next.fetch_max(start, Ordering::SeqCst);
    self.dropped.fetch_max(dropped, Ordering::SeqCst);

    let (writer, written) = unbounded_channel();

    if self
      .kept
      .set(Kept {
        db: db.clone(),
        writer,
      })
      .is_err()
    {
      return Ok(());
    }

    tokio::spawn(keep_events(db.clone(), written, ahead, stored, self.dropped.clone()));

    Ok(())
  }

  /// Every event published so far written to the store, where the bus is durable.
  pub async fn flushed(&self) {
    let Some(kept) = self.kept.get() else { return };
    let (done, wait) = oneshot::channel();

    if kept.writer.send(Write::Flushed(done)).is_ok() {
      let _ = wait.await;
    }
  }

  /// The highest number given so far, none before the first.
  pub fn newest(&self) -> i64 {
    self.next.load(Ordering::SeqCst)
  }

  /// What a reader resuming after `after` is sent, `limit` events at a time: the kept events after it, of
  /// the kinds asked, oldest first — the store's and those still on their way to it alike — and the notice
  /// where events after it are no longer kept, or it is a number never given. A reader naming none is
  /// answered everything kept, with no notice.
  pub async fn replay(&self, after: Option<i64>, kinds: Option<&[String]>, limit: usize) -> Replay {
    let reads = |name: &str| kinds.is_none_or(|kinds| kinds.iter().any(|kind| kind == name));
    let from = after.unwrap_or(0);
    let mut found: Vec<Named> = vec![];

    if let Some(kept) = self.kept.get() {
      let mut cursor = from;

      // Read in pages of the whole stream, the kinds sorted here, until enough are found or none is left.
      while found.len() <= limit {
        let rows = all(
          &kept.db,
          r#"SELECT "id", "kind", "at", "data" FROM "stream_events" WHERE "id" > $1
             ORDER BY "id" LIMIT 1000"#,
          vec![cursor.into()],
        )
        .await
        .unwrap_or_default();

        if rows.is_empty() {
          break;
        }

        for row in &rows {
          let (Ok(id), Ok(name), Ok(at), Ok(data)) = (
            row.try_get::<i64>("", "id"),
            row.try_get::<String>("", "kind"),
            row.try_get::<i64>("", "at"),
            row.try_get::<String>("", "data"),
          ) else {
            continue;
          };

          cursor = id;

          if reads(&name) {
            found.push(Named {
              id,
              at,
              name: Cow::Owned(name),
              body: serde_json::from_str(&data).unwrap_or(serde_json::Value::Null),
            });
          }
        }
      }
    }

    {
      let history = self.history.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

      found.extend(history.iter().filter(|event| event.id > from && reads(&event.name)).cloned());
    }

    found.sort_by_key(|event| event.id);
    found.dedup_by_key(|event| event.id);

    let more = found.len() > limit;

    found.truncate(limit);

    let missed =
      after.filter(|after| *after < self.dropped.load(Ordering::SeqCst) || *after > self.newest());
    let missed = match missed {
      Some(after) => Some(Missed {
        after,
        kept_from: self.oldest().await,
      }),
      None => None,
    };

    Replay {
      events: found,
      more,
      missed,
    }
  }

  // The oldest number kept: the store's, else memory's.
  async fn oldest(&self) -> Option<i64> {
    if let Some(kept) = self.kept.get() {
      let stored = one(&kept.db, r#"SELECT MIN("id") AS "id" FROM "stream_events""#, vec![])
        .await
        .ok()
        .flatten()
        .and_then(|row| row.try_get::<Option<i64>>("", "id").ok().flatten());

      if stored.is_some() {
        return stored;
      }
    }

    let history = self.history.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    history.front().map(|event| event.id)
  }

  /// How many subscriptions to one kind of event stand.
  pub fn subscribers<E: Event>(&self) -> usize {
    let mut subscribers = self.held();
    let Some(senders) = subscribers.get_mut(&TypeId::of::<E>()) else {
      return 0;
    };

    senders.retain(|sender| {
      sender.downcast_ref::<UnboundedSender<E>>().is_some_and(|sender| !sender.is_closed())
    });
    senders.len()
  }
}

fn fault(refusal: crate::Refusal) -> Fault {
  Fault::new(refusal.detail)
}

// The writer of a durable bus: the events waiting are kept together, up to `WRITTEN_AT_ONCE` of them in one
// statement, so the store takes a burst at the pace of its statements rather than of its events, and a
// flush waits on a few writes however many events are behind; the next numbers are written down ahead once
// half of those written down are given, and the oldest past what is kept are dropped every so many events,
// the highest number dropped written down. A store that would not take a write is said and the writer goes
// on.
async fn keep_events(
  db: DatabaseConnection,
  mut written: UnboundedReceiver<Write>,
  mut ahead: i64,
  stored: i64,
  dropped: Arc<AtomicI64>,
) {
  let mut since_drop = 0_u64;

  while let Some(first) = written.recv().await {
    let mut events: Vec<Named> = vec![];
    let mut flushes: Vec<oneshot::Sender<()>> = vec![];
    let mut next = Some(first);

    // A flush waiting among them is answered once the events given before it are written with this batch.
    while let Some(write) = next {
      match write {
        Write::Event(event) => events.push(event),
        Write::Flushed(done) => flushes.push(done),
      }

      next = (events.len() < WRITTEN_AT_ONCE).then(|| written.try_recv().ok()).flatten();
    }

    if let Some(newest) = events.last().map(|event| event.id)
      && newest >= ahead - RESERVE / 2
    {
      ahead = newest + RESERVE;

      if let Err(why) = settings::set(&db, RESERVED_KEY, &ahead.to_string()).await {
        eprintln!("events: the numbers written down ahead: {}", why.detail);
      }
    }

    if let Err(why) = keep(&db, &events).await {
      let (first, last) = (events.first().map(|event| event.id), events.last().map(|event| event.id));

      eprintln!("events: events {first:?} to {last:?} were not kept: {why:?}");
    }

    for done in flushes {
      let _ = done.send(());
    }

    since_drop += events.len() as u64;

    if since_drop >= DROP_EVERY {
      since_drop = 0;

      if let Err(why) = drop_oldest(&db, stored, &dropped).await {
        eprintln!("events: the oldest past what is kept: {why:?}");
      }
    }
  }
}

// The events written to the store in one statement, handed as one JSON array of rows so the statement is
// the same however many there are.
async fn keep(db: &DatabaseConnection, events: &[Named]) -> Result<(), Fault> {
  if events.is_empty() {
    return Ok(());
  }

  let rows: Vec<serde_json::Value> = events
    .iter()
    .map(|event| serde_json::json!({ "id": event.id, "kind": event.name, "at": event.at, "data": event.body.to_string() }))
    .collect();

  run(
    db,
    r#"INSERT INTO "stream_events" ("id", "kind", "at", "data")
       SELECT "id", "kind", "at", "data"
       FROM jsonb_to_recordset($1::jsonb) AS "row"("id" bigint, "kind" text, "at" bigint, "data" text)
       ON CONFLICT ("id") DO NOTHING"#,
    vec![serde_json::Value::Array(rows).to_string().into()],
  )
  .await?;

  Ok(())
}

/// The events past the newest `stored` taken out of the store, the highest number taken written down.
pub async fn drop_oldest(db: &DatabaseConnection, stored: i64, dropped: &AtomicI64) -> Result<(), Fault> {
  let Some(row) = one(
    db,
    r#"SELECT "id" FROM "stream_events" ORDER BY "id" DESC OFFSET $1 LIMIT 1"#,
    vec![stored.into()],
  )
  .await?
  else {
    return Ok(());
  };
  let through: i64 = row.try_get("", "id")?;

  settings::set(db, DROPPED_KEY, &through.max(dropped.load(Ordering::SeqCst)).to_string())
    .await
    .map_err(fault)?;
  run(db, r#"DELETE FROM "stream_events" WHERE "id" <= $1"#, vec![through.into()]).await?;
  dropped.fetch_max(through, Ordering::SeqCst);

  Ok(())
}
