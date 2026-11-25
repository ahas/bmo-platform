//! The cards the gateway's door asks (docs/own-llm.md, 「The gateway」): a request on a registration naming
//! a card service whose engine stands nowhere asks the arbiter's master for one card of the service and
//! waits for it with no clock. The demand is the door's line's, one for each service and each rank and pool
//! its requests name: every request of those that comes while it waits or stands waits on the same grant, and
//! once the last of them has ended it is given back, granted or still waiting.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use harness_machine::card_master::{CardTerms, DEMAND_WAIT, Demand, DemandAsk, Master, MasterFault};
use serde_json::Value;
use tokio::sync::watch;

/// The reference every demand the door asks is shown under.
pub const DOOR_REF: &str = "agency gateway door";

// How long a look that found nothing new waits before the next: the master unreached, or the demand's card
// taken, which the master answers at once.
const AGAIN: Duration = Duration::from_secs(1);

// What the requests on a line are answered: none while the card is asked, then the node of the card
// granted or the master's refusal.
type Answer = Option<Result<String, String>>;

// The requests out on one service, the ask that holds their card, and its demand once granted.
struct Line {
  // Which ask this is: a request that joined an earlier one, since ended, touches nothing of this one.
  ask: u64,
  requests: usize,
  card: watch::Receiver<Answer>,
  demand: Option<i64>,
  // Told once the last request has gone while the card is still asked, so the ask gives back what it
  // asked.
  gone: watch::Sender<bool>,
}

// A line's key: the card service and the terms its requests name.
type LineKey = (String, CardTerms);

/// The door's lines, one for each card service and naming it has requests out on.
pub struct Door {
  master: Master,
  lines: Mutex<HashMap<LineKey, Line>>,
  asks: AtomicU64,
  /// Told once the demands a launcher that ended left have been given back, before which none is asked.
  swept: watch::Sender<bool>,
}

/// One request standing on a line's card, leaving it as it is dropped: its answer carried whole, or its
/// caller gone.
pub struct Seat {
  door: Arc<Door>,
  line: LineKey,
  ask: u64,
}

impl Drop for Seat {
  fn drop(&mut self) {
    let Some(demand) = self.door.leave(&self.line, self.ask) else {
      return;
    };
    // Outside a runtime — the process ending — the next launcher gives it back as it starts.
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
      let master = self.door.master.clone();
      runtime.spawn(async move { give_back(&master, demand).await });
    }
  }
}

// The master's sentence out of a refusal, `{code, error}`, else the refusal as it came.
fn sentence(fault: &MasterFault) -> String {
  match fault {
    MasterFault::Failed { error, .. } => serde_json::from_str::<Value>(error)
      .ok()
      .and_then(|said| said.get("error")?.as_str().map(str::to_owned))
      .unwrap_or_else(|| fault.to_string()),
    other => other.to_string(),
  }
}

async fn give_back(master: &Master, demand: i64) {
  while let Err(fault) = master.give_back(demand).await {
    if matches!(fault, MasterFault::Failed { .. }) {
      eprintln!("llm-gateway: the door's demand {demand} was not given back: {fault}");
      return;
    }
    tokio::time::sleep(AGAIN).await;
  }
}

impl Door {
  /// The door on the master, which first gives back every demand held under the door's reference.
  pub fn new(master: Master) -> Arc<Door> {
    let door = Arc::new(Door {
      master,
      lines: Mutex::new(HashMap::new()),
      asks: AtomicU64::new(0),
      swept: watch::channel(false).0,
    });
    tokio::spawn({
      let door = Arc::clone(&door);
      async move {
        door.give_back_left().await;
        door.swept.send_replace(true);
      }
    });
    door
  }

  fn lines(&self) -> std::sync::MutexGuard<'_, HashMap<LineKey, Line>> {
    self.lines.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
  }

  /// The card of `service` a request stands on: the one its line stands on or waits for, or a new ask
  /// where no line is out. Answered with the node granted once it is, however long that takes, or with the
  /// master's sentence where it refuses the demand.
  pub async fn card(self: &Arc<Self>, service: &str, terms: &CardTerms) -> Result<(Seat, String), String> {
    let line: LineKey = (service.to_owned(), terms.clone());
    let (ask, mut card) = self.join(&line);
    let seat = Seat {
      door: Arc::clone(self),
      line,
      ask,
    };
    let answer = card.wait_for(Option::is_some).await.ok().and_then(|answer| answer.clone());

    match answer {
      Some(Ok(node)) => Ok((seat, node)),
      Some(Err(refused)) => Err(refused),
      None => Err(format!("the card of {service} was asked by none")),
    }
  }

  // Every demand the master holds under the door's reference given back: a launcher that ended left them,
  // since a demand lives no longer than the requests waiting on it, and this one asks none before. Read
  // again while the master cannot be reached.
  async fn give_back_left(&self) {
    let demands = loop {
      match self.master.demands().await {
        Ok(demands) => break demands,
        Err(MasterFault::Unreachable(_)) => tokio::time::sleep(AGAIN).await,
        Err(fault) => {
          eprintln!("llm-gateway: the master's demands could not be read: {fault}");
          return;
        }
      }
    };
    for demand in demands.iter().filter(|demand| demand.reference.as_deref() == Some(DOOR_REF)) {
      give_back(&self.master, demand.id).await;
    }
  }

  // The request joins the line out on the service, or starts one where none is, whose ask then runs on its
  // own: no request's going away cuts it.
  fn join(self: &Arc<Self>, key: &LineKey) -> (u64, watch::Receiver<Answer>) {
    let mut lines = self.lines();

    if let Some(line) = lines.get_mut(key) {
      line.requests += 1;
      return (line.ask, line.card.clone());
    }

    let ask = self.asks.fetch_add(1, Ordering::Relaxed);
    let (answer, card) = watch::channel(None);
    let (gone, left) = watch::channel(false);

    lines.insert(
      key.clone(),
      Line {
        ask,
        requests: 1,
        card: card.clone(),
        demand: None,
        gone,
      },
    );
    tokio::spawn(Arc::clone(self).ask_for_line(ask, key.clone(), answer, left));

    (ask, card)
  }

  // A request of `ask` gone: where it was the last out, the demand to give back where one was granted, or
  // else the ask told to give back what it asked.
  fn leave(&self, key: &LineKey, ask: u64) -> Option<i64> {
    let mut lines = self.lines();
    let line = lines.get_mut(key).filter(|line| line.ask == ask)?;

    line.requests -= 1;
    if line.requests > 0 {
      return None;
    }

    let line = lines.remove(key)?;

    if line.demand.is_none() {
      let _ = line.gone.send(true);
    }

    line.demand
  }

  // The ask's answer laid down where its requests are still out: a granted demand kept for them, or a
  // refusal ending the line so the next request asks anew. False where the last request has gone, whatever
  // the ask holds being its own to give back.
  fn answered(&self, key: &LineKey, ask: u64, answer: &Result<(i64, String), String>) -> bool {
    let mut lines = self.lines();
    let Some(line) = lines.get_mut(key).filter(|line| line.ask == ask) else {
      return false;
    };

    match answer {
      Ok((demand, _)) => line.demand = Some(*demand),
      Err(_) => {
        lines.remove(key);
      }
    }

    true
  }

  // The ask a line's card is held by, on its own: its demand asked, waited on until it is granted, and the
  // answer laid down for the requests out. Where the last request goes before then, or the ask ends in a
  // refusal after a demand was asked, the demand asked last is given back here.
  async fn ask_for_line(
    self: Arc<Self>,
    ask: u64,
    key: LineKey,
    answer: watch::Sender<Answer>,
    mut gone: watch::Receiver<bool>,
  ) {
    let asked = AtomicI64::new(0);
    let outcome = match self.ask(&key).await {
      Ok(demand) => {
        asked.store(demand.id, Ordering::Release);
        self.granted(&key, demand, &asked, &mut gone).await
      }
      Err(refused) => Some(Err(refused)),
    };
    let kept = outcome.as_ref().is_some_and(|outcome| self.answered(&key, ask, outcome));

    if !(kept && matches!(outcome, Some(Ok(_)))) {
      let demand = asked.load(Ordering::Acquire);

      if demand > 0 {
        give_back(&self.master, demand).await;
      }
    }

    if kept {
      let _ = answer.send(outcome.map(|outcome| outcome.map(|(_, node)| node)));
    }
  }

  // A demand for one card of the service, asked once the left demands are given back and again while the
  // master cannot be reached; the master's sentence where it refuses.
  async fn ask(&self, (service, terms): &LineKey) -> Result<Demand, String> {
    let _ = self.swept.subscribe().wait_for(|swept| *swept).await;
    let ask = DemandAsk {
      service_name: service.clone(),
      count: 1,
      reference: Some(DOOR_REF.to_owned()),
      rank: terms.rank,
      pool: terms.pool.clone(),
      ..DemandAsk::default()
    };

    loop {
      match self.master.ask(&ask).await {
        Ok(demand) => return Ok(demand),
        Err(fault @ MasterFault::Failed { status, .. }) if status != 502 && status != 503 => {
          return Err(sentence(&fault));
        }
        Err(_) => tokio::time::sleep(AGAIN).await,
      }
    }
  }

  // The demand read until a card is granted it, or none where the last request has gone first. A demand the
  // master no longer holds — a master restarted — is asked again, and `asked` then names the one asked anew.
  async fn granted(
    &self,
    key: &LineKey,
    mut demand: Demand,
    asked: &AtomicI64,
    gone: &mut watch::Receiver<bool>,
  ) -> Option<Result<(i64, String), String>> {
    loop {
      if let Some(card) = demand.card() {
        return Some(Ok((demand.id, card.node_name.clone())));
      }

      let read = tokio::select! {
        biased;
        Ok(_) = gone.wait_for(|gone| *gone) => return None,
        read = self.master.demand(demand.id, DEMAND_WAIT) => read,
      };

      match read {
        Ok(Some(now)) => {
          demand = now;
          if demand.taken.as_ref().is_some_and(|taken| !taken.is_null()) {
            tokio::time::sleep(AGAIN).await;
          }
        }
        Ok(None) => match self.ask(key).await {
          Ok(again) => {
            asked.store(again.id, Ordering::Release);
            demand = again;
          }
          Err(refused) => return Some(Err(refused)),
        },
        Err(_) => tokio::time::sleep(AGAIN).await,
      }
    }
  }
}
