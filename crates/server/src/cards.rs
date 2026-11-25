//! The cards the arbiter's master grants the runs on a registration naming a card service
//! (docs/own-llm.md, 「An engine the arbiter stands」): a demand asked for every run and held for its turn,
//! waited on however long its grant takes; the run placed on the card granted once the gateway serves that
//! card's entry; a demand asked within the run's own for work the run asks, and the run told where its own
//! engine stood down for it; a card shared by the runs a caller has out at once on one registration, asked
//! once for all of them and given back once the last has gone, granted or still waiting; and every demand
//! agency asked kept in the store's ledger, `card_demands`, so a demand whose run ended without giving it
//! back is given back at the next start and with its lease.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use harness_machine::card_master::{
  CardPool, CardTerms, DEMAND_WAIT, Demand, DemandAsk, GrantedCard, Master, MasterFault,
};
use harness_machine::engine_health;
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use policy::gateway::GATEWAY;
use sea_orm::{ConnectionTrait, DatabaseConnection};
use store::{all, one, run};
use tokio::sync::watch;

use crate::Refusal;
use crate::control::registry::{self, Registration};
use crate::lines::fault;
use crate::now_ms;
use crate::places::Ground;
use crate::run_log::RunLog;
use crate::run_log::lines::Line;
use crate::session_output::OutputWatch;

// How long a look that found nothing new waits before the next: the master unreached, the gateway not yet
// serving a card's entry, or an engine not yet answering its health.
const AGAIN: Duration = Duration::from_secs(1);

/// What a run hands in of the demands it holds itself: one of its own, granted, which the run stands on
/// with no other asked for it; or the run's demand the run's own is asked within.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ask {
  pub own: Option<i64>,
  pub within: Option<i64>,
  /// What its caller says of a demand the run asks.
  pub terms: CardTerms,
}

/// The ending of a turn stopped because the demand it stood on lost its card (docs/api.md, 「sessions」).
pub const CARD_TAKEN: &str = "card-taken";

// Who took the card of each turn stopped for it, by the turn's run, until its end is told.
static TAKEN_FROM: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
  std::sync::LazyLock::new(Mutex::default);

/// Who took the card of the turn of this run, where it was stopped for that; told once.
pub fn taken_from(run: &str) -> Option<String> {
  TAKEN_FROM.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(run)
}

/// The terms a rank and a pool make, as a caller names them.
pub fn terms(rank: Option<u32>, pool: Option<(String, u32)>) -> CardTerms {
  CardTerms {
    rank,
    pool: pool.map(|(key, cards)| CardPool { key, cards }),
  }
}

/// The rank of the demands of the runs for a supervisor, where the name is one's and its row holds one.
pub async fn supervisor_rank<C: ConnectionTrait>(db: &C, name: &str) -> Option<u32> {
  crate::supervisors::named(db, name)
    .await
    .ok()
    .flatten()
    .and_then(|supervisor| supervisor.card_rank)
}

/// The supervisor the ledger names for a room: the one whose turns went into it, where exactly one's did.
pub async fn only_supervisor_of_room<C: ConnectionTrait>(
  db: &C,
  room: i64,
) -> Result<Option<String>, Refusal> {
  let rows = all(
    db,
    r#"SELECT DISTINCT "r"."agent" FROM "runs" AS "r" JOIN "supervisors" AS "s" ON "s"."name" = "r"."agent"
       WHERE "r"."chat_id" = $1"#,
    vec![room.into()],
  )
  .await?;

  match rows.as_slice() {
    [only] => Ok(Some(only.try_get("", "agent")?)),
    _ => Ok(None),
  }
}

// The rank of the supervisor that ordered a job (docs/own-llm.md, 「An engine the arbiter stands」): the job's
// item, its orderer's room, and the one supervisor whose turns went into that room. Anything short of that is no
// answer, said in the log and never guessed.
async fn job_rank(ground: &Ground, job: i64) -> Option<u32> {
  let row = one(&ground.db, r#"SELECT "item_id" FROM "jobs" WHERE "id" = $1"#, vec![job.into()])
    .await
    .ok()??;
  let item: i64 = row.try_get("", "item_id").ok()?;
  let service = crate::queue::orderer::JobService::of(ground.env())?;
  let orderer = match service.orderer_of_id(item).await {
    Ok((_, orderer)) => orderer,
    Err(why) => {
      eprintln!("cards: the orderer of job {job} was not read, so it asks at the default rank: {why}");
      return None;
    }
  };
  let room = orderer.room?;
  let supervisor = only_supervisor_of_room(&ground.db, room).await.ok().flatten();
  let Some(supervisor) = supervisor else {
    eprintln!("cards: room {room} names no one supervisor, so job {job} asks at the default rank");
    return None;
  };

  supervisor_rank(&ground.db, &supervisor).await
}

/// A turn's watch on the demand it stands on: the turn stopped the moment a demand of a better rank takes its
/// card. A claim or a lower tier stops the engine under it, which cuts the request in hand and leaves the rest of
/// the turn as it always has.
pub struct TurnWatch {
  run: String,
  taken: Arc<std::sync::atomic::AtomicBool>,
  watch: tokio::task::JoinHandle<()>,
}

impl TurnWatch {
  /// The watch ended with the turn; the stop it raised over the run's name taken back, so the turn asked again
  /// under that name is not stopped by it.
  pub async fn finish(self, ground: &Ground) {
    self.watch.abort();

    if self.taken.load(Ordering::Acquire) {
      let _ = crate::queue::cancels::take_back(&ground.db, &self.run).await;
    }
  }
}

/// The turn of this run watched on the demand it holds (docs/own-llm.md, 「An engine the arbiter stands」).
pub fn watch_turn(ground: &Ground, held: &Held, run: &str) -> TurnWatch {
  let taken = Arc::new(std::sync::atomic::AtomicBool::new(false));
  let master = master_of(ground);
  let demand = held.demand;
  let stops = crate::queue::stops::Stops::of(ground, None);
  let name = run.to_owned();
  let raised = Arc::clone(&taken);
  let watch = tokio::spawn(async move {
    let Some(master) = master else { return };
    let Some(by) = until_taken(&master, demand).await else { return };

    TAKEN_FROM
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .insert(name.clone(), by);
    raised.store(true, Ordering::Release);

    if let Err(why) = stops.cancel(&name).await {
      eprintln!("cards: the turn {name} was not stopped as its card was taken: {why:?}");
    }
  });

  TurnWatch {
    run: run.to_owned(),
    taken,
    watch,
  }
}

/// Which demand of a better rank took the card of the demand, read with its answer held open until one does; none
/// where the master no longer holds it. A demand lent, or taken by a claim or a lower tier, stands.
pub async fn until_taken(master: &Master, demand: i64) -> Option<String> {
  loop {
    match master.watch(demand, DEMAND_WAIT).await {
      Ok(Some(read)) => {
        if let Some(taken) = read.taken.as_ref().filter(|taken| read.outranked && !taken.is_null()) {
          return Some(taken.as_str().map_or_else(|| taken.to_string(), str::to_owned));
        }
      }
      Ok(None) => return None,
      Err(_) => tokio::time::sleep(AGAIN).await,
    }
  }
}

/// One reason a run waits for its card, handed on as it changes: the card service it waits on and why, as it
/// stands now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardWait {
  pub service: String,
  pub reason: String,
}

impl CardWait {
  fn of(service: &str, reason: impl Into<String>) -> Self {
    Self {
      service: service.to_owned(),
      reason: reason.into(),
    }
  }

  /// The wait as the run's log says it.
  pub fn sentence(&self) -> String {
    fill_marks(&CONTROL_WORDS.cards.waiting, &[("service", &self.service), ("why", &self.reason)])
  }
}

/// A card a run holds for its turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
  pub demand: i64,
  /// The node of the card granted, whose gateway entry the run names.
  pub node: String,
  pub service: String,
  /// Whether agency asked the demand, and so gives it back; a demand the caller handed in is the caller's.
  pub asked: bool,
  /// The run's demand this one was asked within, where it was.
  pub within: Option<i64>,
  /// Whether the run's own card was lent to this one, its engine standing down meanwhile.
  pub lent: bool,
}

/// The master the configuration names.
pub fn master_of(ground: &Ground) -> Option<Master> {
  ground.config.peers.arbiter.clone().map(Master::new)
}

/// A card held for a run on a registration naming a card service, or none on one that names none: a
/// demand handed in, read until it is granted; or one asked for one card with the session's key as the
/// work it continues and kept in the ledger under its holder. Either is waited on however long its grant
/// takes, each reason it waits on handed to `waiting` as it changes, and then until the gateway serves the
/// card's entry; a stop standing over the run `stop` names ends the wait, the demand asked given back.
pub async fn hold(
  ground: &Ground,
  registration: &Registration,
  holder: &str,
  work_key: Option<&str>,
  ask: Ask,
  stop: Option<&str>,
  waiting: &mut (dyn FnMut(&CardWait) + Send),
) -> Result<Option<Held>, Refusal> {
  let Some(service) = registration.card_service.clone() else {
    return Ok(None);
  };
  let Some(master) = master_of(ground) else {
    return Err(no_master(&registration.name, &service));
  };
  let held = hold_on(
    &ground.db,
    &master,
    Asking {
      registration: &registration.name,
      service: &service,
      holder,
      work_key,
      ask,
      lease: None,
      stop,
    },
    waiting,
  )
  .await?;

  served(ground, &registration.name, &service, &held.node, waiting).await?;

  Ok(Some(held))
}

// A run on a registration naming a card service where no master is named.
fn no_master(registration: &str, service: &str) -> Refusal {
  fault(fill_marks(
    &CONTROL_WORDS.cards.no_master,
    &[("name", registration), ("service", service)],
  ))
}

/// The cards a caller's runs share, one for each registration it has runs out on at once.
#[derive(Default)]
pub struct Sharing {
  out: Mutex<HashMap<String, Out>>,
  asks: AtomicU64,
}

// What the runs out on a registration are answered: none while their card is asked, then the card or the
// refusal.
type Answer = Option<Result<Held, Refusal>>;

// The runs out on one registration, the ask that holds their card, and the card once it is granted and
// served.
struct Out {
  // Which ask this is: a run that joined an earlier one, since ended, touches nothing of this one.
  ask: u64,
  runs: usize,
  card: watch::Receiver<Answer>,
  held: Option<Held>,
  // Told once the last run has gone while the card is still asked, so the ask gives back what it asked.
  gone: watch::Sender<bool>,
}

impl Sharing {
  fn out(&self) -> std::sync::MutexGuard<'_, HashMap<String, Out>> {
    self.out.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
  }

  // The run joins the ask out on the registration, or starts one where none is, which then runs on its
  // own: no run's going away cuts it.
  fn join(self: &Arc<Self>, asking: SharedAsk) -> (u64, watch::Receiver<Answer>) {
    let mut out = self.out();

    if let Some(one) = out.get_mut(&asking.registration) {
      one.runs += 1;
      return (one.ask, one.card.clone());
    }

    let ask = self.asks.fetch_add(1, Ordering::Relaxed);
    let (answer, card) = watch::channel(None);
    let (gone, left) = watch::channel(false);

    out.insert(
      asking.registration.clone(),
      Out {
        ask,
        runs: 1,
        card: card.clone(),
        held: None,
        gone,
      },
    );
    tokio::spawn(ask_for_all(self.clone(), ask, asking, answer, left));

    (ask, card)
  }

  // A run of `ask` gone: where it was the last out, the card to give back where one stands, or else the
  // ask told to give back what it asked.
  fn leave(&self, registration: &str, ask: u64) -> Option<Held> {
    let mut out = self.out();
    let one = out.get_mut(registration).filter(|one| one.ask == ask)?;

    one.runs -= 1;
    if one.runs > 0 {
      return None;
    }

    let one = out.remove(registration)?;

    if one.held.is_none() {
      let _ = one.gone.send(true);
    }

    one.held
  }

  // The ask's answer laid down where its runs are still out: a card kept for them, or a refusal ending the
  // ask so the next run asks anew. False where the last run has gone, whatever the ask holds being its own
  // to give back.
  fn answered(&self, registration: &str, ask: u64, answer: &Result<Held, Refusal>) -> bool {
    let mut out = self.out();
    let Some(one) = out.get_mut(registration).filter(|one| one.ask == ask) else {
      return false;
    };

    match answer {
      Ok(held) => one.held = Some(held.clone()),
      Err(_) => {
        out.remove(registration);
      }
    }

    true
  }
}

/// The gateway a run sends to, and the file its key is read from.
#[derive(Clone, Debug)]
pub struct Gateway {
  pub url: String,
  pub key_file: PathBuf,
}

impl Gateway {
  /// The gateway the machine names.
  pub fn of(ground: &Ground) -> Self {
    let machine = ground.machine();

    Self {
      url: harness_machine::gateway::url(machine.var("BMO_LLM_GATEWAY_URL")),
      key_file: harness_providers::registered::Registered::key_file(&machine),
    }
  }
}

/// What a card the runs share is asked with, all of it owned: the ask runs on its own while runs join and
/// leave, for as long as any is out.
#[derive(Clone)]
pub struct SharedAsk {
  pub db: DatabaseConnection,
  pub master: Master,
  pub registration: String,
  pub service: String,
  pub holder: String,
  /// The gateway that must serve the granted card's entry before a run is sent on it.
  pub gateway: Gateway,
}

impl SharedAsk {
  // What its demand is asked for: no work key, no lease, nothing within.
  fn asking(&self) -> Asking<'_> {
    Asking {
      registration: &self.registration,
      service: &self.service,
      holder: &self.holder,
      work_key: None,
      ask: Ask::default(),
      lease: None,
      stop: None,
    }
  }
}

/// One run standing on a shared card: `leave` once its work is done, and a run dropped before it leaves —
/// its caller gone — leaves as it is dropped.
pub struct Sharer {
  pub held: Held,
  leaving: Leaving,
}

// What a run leaving needs to give the card back where it is the last out.
struct Leaving {
  sharing: Arc<Sharing>,
  registration: String,
  ask: u64,
  db: DatabaseConnection,
  master: Master,
  left: bool,
}

impl Sharer {
  /// The run's work done: the card given back where no other run is out on it, the giving back going on
  /// to its end whether or not the run's caller still waits for it.
  pub async fn leave(mut self) {
    self.leaving.left = true;
    if let Some(held) = self.leaving.sharing.leave(&self.leaving.registration, self.leaving.ask) {
      let (db, master) = (self.leaving.db.clone(), self.leaving.master.clone());
      let _ = tokio::spawn(async move { returned(&db, &master, held.demand).await }).await;
    }
  }
}

impl Drop for Leaving {
  fn drop(&mut self) {
    if self.left {
      return;
    }
    let Some(held) = self.sharing.leave(&self.registration, self.ask) else { return };
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
    let (db, master) = (self.db.clone(), self.master.clone());

    runtime.spawn(async move { returned(&db, &master, held.demand).await });
  }
}

/// A card shared with every run `sharing` has out on the registration at once: the run joins the card the
/// runs out already stand on, or waits on the one being asked for them, and where none is it starts the
/// ask, which the runs joining meanwhile wait on with it. The ask is the runs', not the first run's: it
/// goes on while any run waits on it, whichever runs go away meanwhile, and once the last has gone its
/// demand is given back, granted or still waiting. An ask refused answers every run waiting on it, and the
/// next run out asks again.
pub async fn share(sharing: &Arc<Sharing>, asking: SharedAsk) -> Result<Sharer, Refusal> {
  let (registration, db, master) = (asking.registration.clone(), asking.db.clone(), asking.master.clone());
  let (ask, mut card) = sharing.join(asking);
  let leaving = Leaving {
    sharing: sharing.clone(),
    registration,
    ask,
    db,
    master,
    left: false,
  };
  let answer = card.wait_for(Option::is_some).await.ok().and_then(|answer| answer.clone());
  let held = match answer {
    Some(answer) => answer?,
    None => return Err(fault("the card the runs share was asked by none".to_owned())),
  };

  Ok(Sharer { held, leaving })
}

// The ask a shared card is held by, on its own: its demand asked and written into the ledger, waited on
// until it is granted and the gateway serves its entry, and the answer laid down for the runs out. Where
// the last run goes before then, or the ask is refused after its demand was asked, the demand asked last
// is given back here.
async fn ask_for_all(
  sharing: Arc<Sharing>,
  ask: u64,
  asking: SharedAsk,
  answer: watch::Sender<Answer>,
  mut left: watch::Receiver<bool>,
) {
  let asked = AtomicI64::new(0);
  let held = match ask_kept(&asking.db, &asking.master, &asking.asking(), &mut |_| {}).await {
    Ok(demand) => {
      asked.store(demand.id, Ordering::Release);
      tokio::select! {
        biased;
        Ok(_) = left.wait_for(|gone| *gone) => None,
        held = held_for_all(&asking, demand, &asked) => Some(held),
      }
    }
    Err(refused) => Some(Err(refused)),
  };
  let kept = held.as_ref().is_some_and(|held| sharing.answered(&asking.registration, ask, held));

  if !(kept && matches!(held, Some(Ok(_)))) {
    let demand = asked.load(Ordering::Acquire);

    if demand > 0 {
      returned(&asking.db, &asking.master, demand).await;
    }
  }

  if kept {
    let _ = answer.send(held);
  }
}

// A shared card's demand waited on until it is granted, and then until the gateway serves its entry.
async fn held_for_all(asking: &SharedAsk, demand: Demand, asked: &AtomicI64) -> Result<Held, Refusal> {
  let mut quiet = |_: &CardWait| {};
  let (demand, card) =
    granted(&asking.db, &asking.master, demand, Some(()), &asking.asking(), asked, &mut quiet).await?;

  served_at(
    &asking.db,
    &asking.gateway,
    &asking.registration,
    &asking.service,
    &card.node_name,
    &mut quiet,
  )
  .await?;

  Ok(Held {
    demand: demand.id,
    node: card.node_name,
    service: asking.service.clone(),
    asked: true,
    within: None,
    lent: false,
  })
}

/// A card held for a run on a registration naming a card service, shared with every run `sharing` has out
/// on it at once (`share`) and asked as `hold` asks one, with no work key; none on one that names none.
pub async fn hold_shared(
  ground: &Ground,
  sharing: &Arc<Sharing>,
  registration: &Registration,
  holder: &str,
) -> Result<Option<Sharer>, Refusal> {
  let Some(service) = registration.card_service.clone() else {
    return Ok(None);
  };
  let Some(master) = master_of(ground) else {
    return Err(no_master(&registration.name, &service));
  };
  let asking = SharedAsk {
    db: ground.db.clone(),
    master,
    registration: registration.name.clone(),
    service,
    holder: holder.to_owned(),
    gateway: Gateway::of(ground),
  };

  share(sharing, asking).await.map(Some)
}

/// A card held for a run whose log says each reason it waits on as it changes, and whose turn carries the wait
/// as its typed state (docs/api.md, 「sessions」, a running turn's wait), so its room or its watcher reads why
/// the run has not begun; a job's run holds its job for the card meanwhile. The wait clears as the hold
/// returns, granted or not, and the run's stop ends it; the same as `hold` otherwise.
pub async fn hold_logged(
  ground: &Ground,
  registration: &Registration,
  holder: &str,
  work_key: Option<&str>,
  ask: Ask,
  log: &mut RunLog,
  shown: Option<OutputWatch<'_>>,
) -> Result<Option<Held>, Refusal> {
  let mut ask = ask;

  // Named by no caller, its rank is the supervisor's the run is for.
  if ask.terms.rank.is_none() && ask.own.is_none() {
    ask.terms.rank = match supervisor_rank(&ground.db, holder).await {
      Some(rank) => Some(rank),
      None => match log.job() {
        Some(job) if registration.card_service.is_some() => job_rank(ground, job).await,
        _ => None,
      },
    };
  }

  let (said, mut heard) = tokio::sync::mpsc::unbounded_channel::<CardWait>();
  let mut waiting = move |wait: &CardWait| {
    let _ = said.send(wait.clone());
  };
  let run = log.run().to_owned();
  let mut told = WaitTold {
    ground,
    shown,
    run: &run,
    job: log.job(),
    since: None,
  };
  let holding = hold(ground, registration, holder, work_key, ask, Some(&run), &mut waiting);

  tokio::pin!(holding);

  let held = loop {
    tokio::select! {
      held = &mut holding => break held,
      Some(wait) = heard.recv() => {
        log.write(&ground.db, Line::note(wait.sentence())).await;
        told.waits(&wait).await;
      }
    }
  };

  told.cleared().await;

  held
}

/// A turn's card wait told as it begins, changes and clears: kept as the turn's `waiting` for its read,
/// published as `turn.changed`, and, for a job's turn, held as its job's `heldFor`, the item's orderer told
/// once as the wait begins.
pub struct WaitTold<'a> {
  pub ground: &'a Ground,
  /// The turn the wait is of, where the run goes out for one.
  pub shown: Option<OutputWatch<'a>>,
  pub run: &'a str,
  pub job: Option<i64>,
  /// When the wait began; none while nothing was told.
  pub since: Option<i64>,
}

impl WaitTold<'_> {
  /// The run waits for its card, for the reason given.
  pub async fn waits(&mut self, wait: &CardWait) {
    let first = self.since.is_none();
    let since = *self.since.get_or_insert_with(now_ms);

    self.told(Some(agent_session::Waiting {
      kind: agent_session::WaitKind::Card,
      service: wait.service.clone(),
      reason: wait.reason.clone(),
      since,
    }));

    if let Some(job) = self.job {
      crate::control::work::hold_job(self.ground, job, Some(&wait.sentence()), first).await;
    }
  }

  /// The run waits for its card no longer; nothing is told where it never waited.
  pub async fn cleared(&mut self) {
    if self.since.take().is_none() {
      return;
    }

    self.told(None);

    if let Some(job) = self.job {
      crate::control::work::hold_job(self.ground, job, None, false).await;
    }
  }

  fn told(&self, waiting: Option<agent_session::Waiting>) {
    let Some(shown) = self.shown else { return };

    shown.waiting(waiting.clone());
    self.ground.events.publish(crate::herdr_backend::TurnChanged {
      session: shown.session.0.clone(),
      turn_id: shown.turn,
      run: self.run.to_owned(),
      waiting,
    });
  }
}

/// A card held for a run on a provider's model, where the provider is the registry's and the registration
/// names a card service, its wait told as `hold_logged` tells it; none for any other.
pub async fn hold_for_model(
  ground: &Ground,
  provider: &str,
  model: &str,
  holder: &str,
  work_key: Option<&str>,
  log: &mut RunLog,
  shown: Option<OutputWatch<'_>>,
) -> Result<Option<Held>, Refusal> {
  if provider != registry::PROVIDER {
    return Ok(None);
  }

  match registry::registration(&ground.db, model).await.map_err(|why| fault(why.error))? {
    Some(held) => hold_logged(ground, &held, holder, work_key, Ask::default(), log, shown).await,
    None => Ok(None),
  }
}

/// What a demand is asked for.
pub struct Asking<'a> {
  pub registration: &'a str,
  pub service: &'a str,
  pub holder: &'a str,
  pub work_key: Option<&'a str>,
  pub ask: Ask,
  pub lease: Option<&'a str>,
  /// The run whose stop ends the wait: a cancellation standing over its name gives the demand asked back,
  /// and nothing is held.
  pub stop: Option<&'a str>,
}

/// A card held on the master: the demand handed in, or one asked and kept in the ledger, waited on until
/// granted.
pub async fn hold_on(
  db: &DatabaseConnection,
  master: &Master,
  asking: Asking<'_>,
  waiting: &mut (dyn FnMut(&CardWait) + Send),
) -> Result<Held, Refusal> {
  if let Some(own) = asking.ask.own {
    let demand = loop {
      match master.demand(own, Duration::ZERO).await {
        Ok(Some(demand)) => break demand,
        Ok(None) => {
          return Err(Refusal::bad(
            "demand-gone",
            fill_marks(&CONTROL_WORDS.cards.gone, &[("demand", &own.to_string())]),
          ));
        }
        Err(_) => unreached(asking.service, waiting).await,
      }
    };
    let (demand, card) = granted(db, master, demand, None, &asking, &AtomicI64::new(own), waiting).await?;

    return Ok(Held {
      demand: demand.id,
      node: card.node_name,
      service: asking.service.to_owned(),
      asked: false,
      within: None,
      lent: false,
    });
  }

  let asked = ask_kept(db, master, &asking, waiting).await?;
  let last = AtomicI64::new(asked.id);
  let (demand, card) = granted(db, master, asked, Some(()), &asking, &last, waiting).await?;
  let lent = match asking.ask.within {
    Some(outer) => master
      .demand(outer, Duration::ZERO)
      .await
      .ok()
      .flatten()
      .is_some_and(|outer| outer.lent()),
    None => false,
  };

  Ok(Held {
    demand: demand.id,
    node: card.node_name,
    service: asking.service.to_owned(),
    asked: true,
    within: asking.ask.within,
    lent,
  })
}

// A demand asked and written into the ledger, asked again while the master cannot be reached.
async fn ask_kept(
  db: &DatabaseConnection,
  master: &Master,
  asking: &Asking<'_>,
  waiting: &mut (dyn FnMut(&CardWait) + Send),
) -> Result<Demand, Refusal> {
  let ask = DemandAsk {
    service_name: asking.service.to_owned(),
    count: 1,
    within: asking.ask.within,
    work_key: asking.work_key.map(str::to_owned),
    reference: Some(format!("agency {}", asking.holder)),
    rank: asking.ask.terms.rank,
    pool: asking.ask.terms.pool.clone(),
  };
  let demand = loop {
    match master.ask(&ask).await {
      Ok(demand) => break demand,
      Err(MasterFault::Failed { status, error }) if status != 502 && status != 503 => {
        return Err(fault(format!(
          "the arbiter's master refused a demand for {}: {error}",
          asking.service
        )));
      }
      Err(_) => unreached(asking.service, waiting).await,
    }
  };

  run(
    db,
    r#"INSERT INTO "card_demands" ("demand_id", "service", "registration", "holder", "lease", "within", "asked_at")
       VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT ("demand_id") DO NOTHING"#,
    vec![
      demand.id.into(),
      asking.service.into(),
      asking.registration.into(),
      asking.holder.into(),
      asking.lease.map(str::to_owned).into(),
      asking.ask.within.into(),
      now_ms().into(),
    ],
  )
  .await?;

  Ok(demand)
}

// A demand read until a card is granted it, each reason it waits on said once as it changes. A demand the
// master no longer holds — a master restarted — is asked again where agency asked it, its work key bringing
// it back to its card, and `asked` then names the one asked anew; one the caller handed in is refused.
async fn granted(
  db: &DatabaseConnection,
  master: &Master,
  mut demand: Demand,
  ours: Option<()>,
  asking: &Asking<'_>,
  asked: &AtomicI64,
  waiting: &mut (dyn FnMut(&CardWait) + Send),
) -> Result<(Demand, GrantedCard), Refusal> {
  let mut said: Option<String> = None;

  loop {
    if let Some(card) = demand.card().cloned() {
      return Ok((demand, card));
    }

    if let Some(run) = asking.stop
      && crate::door::execution::cancelled(db, &[run]).await.unwrap_or(false)
    {
      if ours.is_some() {
        returned(db, master, demand.id).await;
      }

      return Err(Refusal::bad(
        "stopped",
        fill_marks(&CONTROL_WORDS.cards.stopped, &[("run", run), ("service", asking.service)]),
      ));
    }

    if let Some(why) = demand.waiting_on().filter(|why| said.as_ref() != Some(why)) {
      waiting(&CardWait::of(asking.service, why.clone()));
      said = Some(why);
    }

    match master.demand(demand.id, DEMAND_WAIT).await {
      Ok(Some(now)) => demand = now,
      Ok(None) if ours.is_some() => {
        forget(db, demand.id).await?;
        demand = ask_kept(db, master, asking, waiting).await?;
        asked.store(demand.id, Ordering::Release);
      }
      Ok(None) => {
        return Err(Refusal::bad(
          "demand-gone",
          fill_marks(&CONTROL_WORDS.cards.gone, &[("demand", &demand.id.to_string())]),
        ));
      }
      Err(_) => unreached(asking.service, waiting).await,
    }
  }
}

async fn unreached(service: &str, waiting: &mut (dyn FnMut(&CardWait) + Send)) {
  waiting(&CardWait::of(service, CONTROL_WORDS.cards.unreachable.clone()));
  tokio::time::sleep(AGAIN).await;
}

// The gateway the run sends to read until it serves the entry of the card granted, `<registration>@<node>`:
// the launcher lays a standing instance into its table on its beat, so an engine the grant started is
// served a moment after it stands, and a run sent before then would name an entry the gateway does not
// hold. A gateway that takes no such entry within the policy's span is given up on, saying so.
async fn served(
  ground: &Ground,
  registration: &str,
  service: &str,
  node: &str,
  waiting: &mut (dyn FnMut(&CardWait) + Send),
) -> Result<(), Refusal> {
  served_at(&ground.db, &Gateway::of(ground), registration, service, node, waiting).await
}

// The same, on a gateway of the caller's.
async fn served_at(
  db: &DatabaseConnection,
  gateway: &Gateway,
  registration: &str,
  card_service: &str,
  node: &str,
  waiting: &mut (dyn FnMut(&CardWait) + Send),
) -> Result<(), Refusal> {
  let entry = format!("{registration}@{node}");
  let Some(mut service) = registry::service_of(db, registration).await.map_err(|why| fault(why.error))?
  else {
    return Err(fault(format!("{registration} is no registration the registry holds")));
  };

  service.base_url = gateway.url.clone();

  let mut said = false;
  let bound = std::time::Instant::now() + Duration::from_secs(GATEWAY.serve_seconds);

  loop {
    let listed = match crate::control::own_llm::credential(&gateway.key_file) {
      Ok(key) => crate::control::own_llm::models(&service, &key).await.unwrap_or_default(),
      Err(why) => return Err(fault(why.said(&service))),
    };

    if listed.contains(&entry) {
      return Ok(());
    }

    if !said {
      waiting(&CardWait::of(card_service, format!("the gateway is taking {entry} into its table.")));
      said = true;
    }

    if std::time::Instant::now() >= bound {
      let seconds = GATEWAY.serve_seconds.to_string();

      return Err(fault(fill_marks(
        &CONTROL_WORDS.cards.unserved,
        &[("entry", &entry), ("seconds", &seconds)],
      )));
    }

    tokio::time::sleep(AGAIN).await;
  }
}

/// A card given back once its run's turn has ended: a demand agency asked given back on the master and
/// taken out of the ledger; one the caller handed in left to the caller. Where the run's own card was lent
/// for it, the run's demand is read until it is granted again and its engine answers health, however long
/// that takes, and the sentence telling the run so is answered.
pub async fn give_back(ground: &Ground, held: &Held) -> Option<String> {
  let master = master_of(ground)?;

  give_back_on(&ground.db, &master, &ground.roots.data, held).await
}

/// The same, on a master and a data root of the caller's.
pub async fn give_back_on(
  db: &DatabaseConnection,
  master: &Master,
  data_root: &Path,
  held: &Held,
) -> Option<String> {
  if held.asked {
    returned(db, master, held.demand).await;
  }

  let outer = held.within.filter(|_| held.lent)?;

  back(master, data_root, outer).await;

  Some(CONTROL_WORDS.cards.stood_down.clone())
}

// The run's own demand read until a card is granted it again and the engine there answers health.
async fn back(master: &Master, data_root: &Path, outer: i64) {
  loop {
    let card = match master.demand(outer, DEMAND_WAIT).await {
      Ok(Some(demand)) => demand.card().cloned(),
      Ok(None) => return,
      Err(_) => {
        tokio::time::sleep(AGAIN).await;
        continue;
      }
    };
    let Some(card) = card else { continue };
    let Some(url) = card.address.clone() else { return };
    let address = engine_health::Address {
      url,
      key_file: card.key_file.clone(),
    };

    if engine_health::health(data_root, std::slice::from_ref(&address)).await.is_some() {
      return;
    }

    tokio::time::sleep(AGAIN).await;
  }
}

// A demand given back on the master and taken out of the ledger; one the master could not be asked to
// give back stays in the ledger for the next pass.
async fn returned(db: &DatabaseConnection, master: &Master, demand: i64) -> bool {
  match master.give_back(demand).await {
    Ok(()) => forget(db, demand).await.is_ok(),
    Err(why) => {
      eprintln!("cards: the demand {demand} was not given back: {why}");
      false
    }
  }
}

async fn forget<C: ConnectionTrait>(db: &C, demand: i64) -> Result<(), Refusal> {
  run(db, r#"DELETE FROM "card_demands" WHERE "demand_id" = $1"#, vec![demand.into()]).await?;

  Ok(())
}

/// The granted demand a holder's turn stands on now, the latest it asked with no lease of its own; none
/// where it holds none.
pub async fn holding<C: ConnectionTrait>(db: &C, holder: &str) -> Result<Option<i64>, Refusal> {
  Ok(
    one(
      db,
      r#"SELECT "demand_id" FROM "card_demands" WHERE "holder" = $1 AND "lease" IS NULL
         ORDER BY "asked_at" DESC LIMIT 1"#,
      vec![holder.into()],
    )
    .await?
    .map(|row| row.try_get::<i64>("", "demand_id"))
    .transpose()?,
  )
}

/// The card a lease's demand holds, as the ledger keeps it: the demand, and the run's demand it was asked
/// within where it was.
pub async fn of_lease<C: ConnectionTrait>(
  db: &C,
  lease: &str,
) -> Result<Option<(i64, Option<i64>)>, Refusal> {
  one(
    db,
    r#"SELECT "demand_id", "within" FROM "card_demands" WHERE "lease" = $1"#,
    vec![lease.into()],
  )
  .await?
  .map(|row| Ok((row.try_get::<i64>("", "demand_id")?, row.try_get::<Option<i64>>("", "within")?)))
  .transpose()
}

/// Every demand the ledger keeps given back: what a process left behind when it ended, given back as the
/// next one starts, since no run of this one holds any yet. Answers how many were given back.
pub async fn give_back_left(db: &DatabaseConnection, master: &Master) -> Result<usize, Refusal> {
  let mut given = 0;

  for row in all(db, r#"SELECT "demand_id" FROM "card_demands""#, vec![]).await? {
    if returned(db, master, row.try_get::<i64>("", "demand_id")?).await {
      given += 1;
    }
  }

  Ok(given)
}

/// Every demand a lease held given back once that lease is no longer active — released, cancelled or gone
/// — whatever path ended it. Answers how many were given back.
pub async fn give_back_with_leases(db: &DatabaseConnection, master: &Master) -> Result<usize, Refusal> {
  let mut given = 0;
  let rows = all(
    db,
    r#"SELECT "demand_id" FROM "card_demands" AS "held" WHERE "lease" IS NOT NULL AND NOT EXISTS (
         SELECT 1 FROM "role_slot_requests" AS "seat"
         WHERE "seat"."token" = "held"."lease" AND "seat"."state" IN ('waiting', 'reserved', 'active'))"#,
    vec![],
  )
  .await?;

  for row in rows {
    if returned(db, master, row.try_get::<i64>("", "demand_id")?).await {
      given += 1;
    }
  }

  Ok(given)
}

/// A seat's card as an admission answers it: its demand, its state, the node granted where one is, and
/// what it waits on where it waits.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatCard {
  pub id: i64,
  pub state: String,
  pub node_name: Option<String>,
  pub reason: Option<String>,
}

impl SeatCard {
  fn of(demand: &Demand) -> Self {
    Self {
      id: demand.id,
      state: demand.state.clone(),
      node_name: demand.card().map(|card| card.node_name.clone()),
      reason: demand.waiting_on(),
    }
  }

  /// Whether the seat's child may go out now, on the node granted.
  pub fn granted(&self) -> bool {
    self.node_name.is_some()
  }
}

/// The card of a seat the pool let in, on a registration naming a card service: the caller's own demand
/// the admission hands in, which the seat stands on with none asked and nothing kept under its lease; or
/// else the demand its lease holds, or one asked now — within the asking run's demand where the admission
/// names one — and kept in the ledger under the lease. Either is read as it stands. None for a registration
/// that names none. The seat waits for its grant outside its task's clock, so a caller asks again while it
/// waits (docs/coworker-pool.md, 「Concurrency limits」); the read holds nothing open at the master, so
/// every admission answers at once, within whatever clock the caller's own call runs on.
pub async fn seat_card(
  ground: &Ground,
  seat: &crate::order_run::dispatch::Request,
  ask: Ask,
) -> Result<Option<SeatCard>, Refusal> {
  seat_card_on(&ground.db, master_of(ground).as_ref(), seat, ask).await
}

/// The same, on a master of the caller's; refused where the registration names a card service and no
/// master is named.
pub async fn seat_card_on(
  db: &DatabaseConnection,
  master: Option<&Master>,
  seat: &crate::order_run::dispatch::Request,
  ask: Ask,
) -> Result<Option<SeatCard>, Refusal> {
  if seat.provider != registry::PROVIDER {
    return Ok(None);
  }

  let Some(registration) = registry::registration(db, &seat.model).await.map_err(|why| fault(why.error))?
  else {
    return Ok(None);
  };
  let Some(service) = registration.card_service.clone() else {
    return Ok(None);
  };
  let Some(master) = master else {
    return Err(fault(fill_marks(
      &CONTROL_WORDS.cards.no_master,
      &[("name", &registration.name), ("service", &service)],
    )));
  };
  if let Some(own) = ask.own {
    return match master.demand(own, Duration::ZERO).await {
      Ok(Some(demand)) => Ok(Some(SeatCard::of(&demand))),
      Ok(None) => Err(Refusal::bad(
        "demand-gone",
        fill_marks(&CONTROL_WORDS.cards.gone, &[("demand", &own.to_string())]),
      )),
      Err(why) => Err(fault(why.to_string())),
    };
  }
  let asking = Asking {
    registration: &registration.name,
    service: &service,
    holder: &seat.owner,
    work_key: seat.session.as_deref(),
    ask: Ask {
      own: None,
      within: ask.within,
      terms: CardTerms {
        rank: match ask.terms.rank {
          Some(rank) => Some(rank),
          None => match seat.asked_by.as_deref() {
            Some(supervisor) => supervisor_rank(db, supervisor).await,
            None => None,
          },
        },
        pool: ask.terms.pool.clone(),
      },
    },
    lease: Some(&seat.token),
    stop: None,
  };
  let mut quiet = |_: &CardWait| {};
  let read = match of_lease(db, &seat.token).await? {
    Some((demand, _)) => match master.demand(demand, Duration::ZERO).await {
      Ok(Some(demand)) => demand,
      Ok(None) => {
        forget(db, demand).await?;
        ask_kept(db, master, &asking, &mut quiet).await?
      }
      Err(why) => return Err(fault(why.to_string())),
    },
    None => ask_kept(db, master, &asking, &mut quiet).await?,
  };

  Ok(Some(SeatCard::of(&read)))
}

/// The card a seat's lease held given back as the seat is finished. Where the asking run's own card was lent
/// for it, the run's demand is read until it is granted again and its engine answers health, and the
/// sentence telling the run so is answered.
pub async fn give_back_seat(ground: &Ground, token: &str) -> Result<Option<String>, Refusal> {
  match master_of(ground) {
    Some(master) => give_back_lease(&ground.db, &master, &ground.roots.data, token).await,
    None => Ok(None),
  }
}

/// The same, on a master and a data root of the caller's.
pub async fn give_back_lease(
  db: &DatabaseConnection,
  master: &Master,
  data_root: &Path,
  token: &str,
) -> Result<Option<String>, Refusal> {
  let Some((demand, within)) = of_lease(db, token).await? else {
    return Ok(None);
  };
  let lent = match within {
    Some(outer) => master
      .demand(outer, Duration::ZERO)
      .await
      .ok()
      .flatten()
      .is_some_and(|outer| outer.lent()),
    None => false,
  };
  let held = Held {
    demand,
    node: String::new(),
    service: String::new(),
    asked: true,
    within,
    lent,
  };

  Ok(give_back_on(db, master, data_root, &held).await)
}
