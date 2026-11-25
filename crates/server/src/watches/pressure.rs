//! The pressures the node service tells on this machine, published again with the job behind each place,
//! which only agency knows (docs/agent-control.md, 「Pressure told as a fact」; docs/api.md, 「Events」).

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use policy::machine_room::MACHINE_ROOM;
use serde::Serialize;
use serde_json::Value;

use super::machine::{AtWork, at_work, counted};
use crate::Refusal;
use crate::events::Event;
use crate::peers::PeerFault;
use crate::peers::node::{Alert, Machine, Standing};
use crate::places::Ground;

/// A pressure on this machine, as agency publishes it for the consumer that tells the supervisor.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachinePressure {
  #[serde(flatten)]
  pub told: Told,
  pub reading: Option<Value>,
  pub holders: Vec<Holder>,
  pub largest: Vec<Largest>,
  pub jobs: u64,
  pub browser_jobs: u64,
}

impl Event for MachinePressure {
  const NAME: &'static str = "machine-pressure";
}

/// What the pressure is, with its figures.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "cause", rename_all = "lowercase")]
pub enum Told {
  #[serde(rename_all = "camelCase")]
  Memory {
    full_pct: f64,
    alert_pct: f64,
    minutes: f64,
  },
  #[serde(rename_all = "camelCase")]
  Disk { free_gb: f64, limit_gb: f64 },
}

/// What one place holds, with the worker at work in it and the job and item its turn named.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Holder {
  pub place: String,
  pub megabytes: f64,
  pub browsers: u64,
  pub worker: Option<String>,
  pub job_id: Option<i64>,
  pub todo_id: Option<i64>,
}

/// One of the largest processes, with the job of the place it stands in.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Largest {
  pub pid: i64,
  pub name: String,
  pub megabytes: f64,
  pub place: Option<String>,
  pub job_id: Option<i64>,
}

fn worker_in<'a>(at_work: &'a [AtWork], place: Option<&str>) -> Option<&'a AtWork> {
  place.and_then(|place| at_work.iter().find(|worker| worker.worker == place))
}

/// One alert the machine tells, as agency publishes it with each place named by the worker at work in it
/// and its job.
pub fn named(machine: &Machine, alert: &Alert, at_work: &[AtWork]) -> MachinePressure {
  let standing = machine.standing.clone().unwrap_or_else(Standing::unread);
  let told = match alert {
    Alert::Memory {
      full_pct,
      alert_pct,
      minutes,
    } => Told::Memory {
      full_pct: *full_pct,
      alert_pct: *alert_pct,
      minutes: *minutes as f64,
    },
    Alert::Disk { free_gb, limit_gb } => Told::Disk {
      free_gb: *free_gb,
      limit_gb: *limit_gb,
    },
  };
  let reading = standing.reading.as_ref().and_then(|reading| serde_json::to_value(reading).ok());
  let (jobs, browser_jobs) = counted(at_work, &standing);
  let holders = standing
    .scopes
    .iter()
    .map(|place| {
      let worker = worker_in(at_work, Some(&place.scope));

      Holder {
        worker: worker.map(|worker| worker.worker.clone()),
        job_id: worker.and_then(|worker| worker.job).map(|job| job.job),
        todo_id: worker.and_then(|worker| worker.job).and_then(|job| job.item),
        place: place.scope.clone(),
        megabytes: place.megabytes as f64,
        browsers: u64::try_from(place.browsers).unwrap_or(0),
      }
    })
    .collect();
  let largest = standing
    .largest
    .iter()
    .map(|process| Largest {
      job_id: worker_in(at_work, process.scope.as_deref())
        .and_then(|worker| worker.job)
        .map(|job| job.job),
      pid: process.pid,
      name: process.name.clone(),
      megabytes: process.megabytes as f64,
      place: process.scope.clone(),
    })
    .collect();

  MachinePressure {
    told,
    reading,
    holders,
    largest,
    jobs,
    browser_jobs,
  }
}

// One alert published with its jobs named.
async fn tell(ground: &Ground, machine: &Machine, alert: &Alert) -> Result<(), Refusal> {
  ground.events.publish(named(machine, alert, &at_work(&ground.db).await?));

  Ok(())
}

/// The node service's changes listened to for as long as the server runs, the stream opened again a while
/// after it drops or cannot be opened; a stream that cannot be opened is said once until it opens. An
/// alert already published is not published again until it has ended and risen again.
pub async fn relay(ground: Arc<Ground>) {
  let reopen = Duration::from_secs(MACHINE_ROOM.clocks.pressure_reopen_seconds);
  let mut said_closed = false;
  let mut told: Vec<&'static str> = vec![];

  loop {
    match ground.peers.node.pressures().await {
      Ok(mut changed) => {
        said_closed = false;

        while let Some(machine) = changed.next().await {
          match machine {
            Ok(machine) => {
              let standing = machine.standing.clone().unwrap_or_else(Standing::unread);
              let fresh: Vec<Alert> =
                standing.alerts.iter().filter(|alert| !told.contains(&alert.cause())).cloned().collect();

              for alert in &fresh {
                if let Err(refusal) = tell(&ground, &machine, alert).await {
                  eprintln!("agency: a pressure on the machine could not be told: {}", refusal.detail);
                }
              }

              told = standing.alerts.iter().map(Alert::cause).collect();
            }
            Err(fault @ PeerFault::Malformed { .. }) => eprintln!("agency: {fault}"),
            Err(fault) => {
              eprintln!("agency: the node service's changes dropped: {fault}");
              break;
            }
          }
        }
      }
      Err(fault) if !said_closed => {
        said_closed = true;
        eprintln!("agency: the node service's changes could not be opened: {fault}");
      }
      Err(_) => {}
    }

    tokio::time::sleep(reopen).await;
  }
}
