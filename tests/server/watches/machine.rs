//! This machine's room for the launcher: the machine's five limits over the standing the node service
//! reports, agency's job-count limits over the workers at work here, the launcher's notice of a held
//! launch kept, and the room's return told once nothing holds a launch.

use agency_server::events::Bus;
use agency_server::peers::node::{
  KernelReading, NodeRow, NodeService, Pressure as MemoryPressure, PressureLine, ScopeUse, Standing,
};
use agency_server::peers::{Asking, PeerFault};
use agency_server::watches::machine::{
  AtWork, LaunchHeld, MachineRoomReturned, at_work, keep_worker_job, launch_held, room, tell_launch_held,
  watch_held,
};
use agent_session::JobMark;
use serde_json::json;

use crate::support::rows::{repository, supervisor};
use crate::support::scratch::Scratch;

/// A node service reporting the standing the test names, which cannot be asked at all where it is `down`.
struct Machine {
  standing: Standing,
  down: bool,
}

fn unreachable() -> PeerFault {
  PeerFault::Unreachable {
    peer: "node service",
    detail: "refused".into(),
  }
}

// A reading with every figure the four limits are judged on.
fn reading(
  memory_full: Option<f64>,
  mem_available_gb: f64,
  load1: f64,
  disk_free_gb: f64,
) -> KernelReading {
  let line = |avg60: f64| PressureLine {
    avg10: avg60,
    avg60,
    avg300: avg60,
  };

  KernelReading {
    at: 1_700_000_000_000,
    load1,
    threads: 24,
    mem_total_gb: 32.0,
    mem_available_gb,
    swap_total_gb: 16.0,
    swap_free_gb: 8.0,
    disk_free_gb: Some(disk_free_gb),
    memory: memory_full.map(|full| MemoryPressure {
      some: line(full),
      full: Some(line(full)),
    }),
    cpu: None,
  }
}

// The standing agency reads: a roomy reading, and the worker w1 running two browsers in its place.
fn standing(reading: KernelReading) -> Standing {
  Standing {
    reading: Some(reading),
    alerts: vec![],
    browsers: 3,
    scopes: vec![
      ScopeUse {
        scope: "w1".into(),
        megabytes: 800,
        browsers: 2,
      },
      ScopeUse {
        scope: "w2".into(),
        megabytes: 300,
        browsers: 0,
      },
      ScopeUse {
        scope: "chat".into(),
        megabytes: 200,
        browsers: 1,
      },
    ],
    largest: vec![],
  }
}

impl NodeService for Machine {
  fn machine_standing(&self) -> Asking<'_, Standing> {
    let answer = if self.down { Err(unreachable()) } else { Ok(self.standing.clone()) };

    Box::pin(async move { answer })
  }

  fn link_up<'a>(&'a self, _node: &'a str) -> Asking<'a, bool> {
    Box::pin(async { Ok(false) })
  }

  fn node_rows(&self) -> Asking<'_, Vec<NodeRow>> {
    Box::pin(async { Ok(vec![]) })
  }

  fn ssh_login<'a>(&'a self, _node: &'a str) -> Asking<'a, Option<Vec<String>>> {
    Box::pin(async { Ok(None) })
  }
}

// Checkouts w1 and w2 at work here, w3 idle, a repository's checkout at work on a node, and a supervisor
// with a run open.
async fn workers(scratch: &Scratch) {
  for name in ["w1", "w2", "w3"] {
    repository(scratch.db(), name, &format!("/checkouts/{name}"), &[]).await;
  }

  repository(scratch.db(), "far", "/remote/far", &[("node", "n1".into())]).await;
  supervisor(scratch.db(), "chat", &[]).await;

  for (id, agent, ended) in [
    ("e1", "w1", None),
    ("e2", "w2", None),
    ("e3", "w3", Some(5)),
    ("e4", "far", None),
    ("e5", "chat", None),
  ] {
    scratch
      .run(
        r#"INSERT INTO "agent_executions" ("id", "body") VALUES ($1, $2)"#,
        vec![id.into(), json!({ "id": id, "agent": agent, "endedAt": ended }).to_string().into()],
      )
      .await;
  }
}

#[tokio::test]
async fn the_jobs_at_work_are_the_workers_here_with_a_run_open_each_with_the_job_its_turn_named() {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  workers(&scratch).await;
  keep_worker_job(
    db,
    "w1",
    Some(JobMark {
      job: 41,
      item: Some(1802),
    }),
  )
  .await;
  keep_worker_job(
    db,
    "w2",
    Some(JobMark {
      job: 42,
      item: None,
    }),
  )
  .await;
  keep_worker_job(db, "w2", None).await;

  assert_eq!(
    at_work(db).await.unwrap(),
    vec![
      AtWork {
        worker: "w1".into(),
        job: Some(JobMark {
          job: 41,
          item: Some(1802)
        })
      },
      AtWork {
        worker: "w2".into(),
        job: None
      },
    ]
  );
}

#[tokio::test]
async fn the_room_holds_on_the_machines_limits_and_then_on_agency_s_job_counts() {
  let scratch = Scratch::new().await;
  let db = scratch.db();

  workers(&scratch).await;

  let roomy = Machine {
    standing: standing(reading(Some(1.0), 12.0, 10.0, 100.0)),
    down: false,
  };

  assert_eq!(
    room(db, &roomy).await.unwrap().held,
    Vec::<String>::new(),
    "a roomy machine holds nothing"
  );

  let (jobs, browser_jobs, browsers) = {
    let room = room(db, &roomy).await.unwrap();

    (room.jobs, room.browser_jobs, room.browsers)
  };

  assert_eq!((jobs, browser_jobs, browsers), (2, 1, 3));

  // 8GB available, less the 1.2GB w1 and the 1.7GB w2 still lack of their 2GB shares and 2GB for one more.
  let growing = Machine {
    standing: standing(reading(Some(1.0), 8.0, 10.0, 100.0)),
    down: false,
  };

  assert_eq!(
    room(db, &growing).await.unwrap().held,
    vec![
      "available memory 8GB less 2.9GB the 2 jobs at work are still to grow into and 2GB for one more < 4GB"
    ],
    "the jobs at work are reckoned to grow"
  );

  let short = Machine {
    standing: standing(reading(Some(5.04), 3.5, 50.0, 12.0)),
    down: false,
  };

  assert_eq!(
    room(db, &short).await.unwrap().held,
    vec![
      "memory pressure 5% >= 5%".to_owned(),
      "available memory 3.5GB < 4GB".to_owned(),
      "load 50 >= 24 threads x 2".to_owned(),
      "disk free 12GB < 20GB".to_owned(),
    ]
  );

  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES
         ('machine_limit:launch_jobs', '2'), ('machine_limit:launch_browser_jobs', 'none'),
         ('machine_limit:launch_disk_free_gb', '10')"#,
      vec![],
    )
    .await;

  assert_eq!(
    room(db, &short).await.unwrap().held,
    vec![
      "memory pressure 5% >= 5%".to_owned(),
      "available memory 3.5GB < 4GB".to_owned(),
      "load 50 >= 24 threads x 2".to_owned(),
      "jobs 2 >= 2".to_owned(),
    ],
    "a settings row replaces a limit, and a machine that is only short of room within it holds nothing"
  );

  let unread = room(
    db,
    &Machine {
      standing: Standing::unread(),
      down: true,
    },
  )
  .await
  .unwrap();

  assert_eq!(
    unread.held,
    vec!["jobs 2 >= 2".to_owned()],
    "a standing that cannot be read holds nothing of the machine's part"
  );
  assert_eq!((unread.browser_jobs, unread.browsers), (0, 0));
}

#[tokio::test]
async fn a_held_launch_is_let_go_and_its_return_told_once_nothing_holds_and_a_withdrawal_tells_nothing() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let bus = Bus::default();
  let mut returned = bus.subscribe::<MachineRoomReturned>();
  let notice = LaunchHeld {
    at: 1_000,
    held: vec!["memory pressure 6% >= 5%".into()],
  };
  let roomy = || Machine {
    standing: standing(reading(Some(1.0), 8.0, 10.0, 100.0)),
    down: false,
  };
  let held_by_memory = Machine {
    standing: standing(reading(Some(6.0), 8.0, 10.0, 100.0)),
    down: false,
  };

  assert!(!watch_held(db, &roomy(), &bus).await.unwrap(), "no notice, nothing to tell");

  tell_launch_held(db, &notice).await.unwrap();
  assert_eq!(launch_held(db).await.unwrap(), Some(notice.clone()));
  assert!(!watch_held(db, &held_by_memory, &bus).await.unwrap());
  assert_eq!(launch_held(db).await.unwrap(), Some(notice.clone()), "still held, the notice stands");
  assert!(returned.ready().is_none());

  assert!(watch_held(db, &roomy(), &bus).await.unwrap());
  assert_eq!(
    returned.ready(),
    Some(MachineRoomReturned {
      at: 1_000,
      held: vec!["memory pressure 6% >= 5%".into()]
    })
  );
  assert_eq!(launch_held(db).await.unwrap(), None);

  tell_launch_held(db, &notice).await.unwrap();
  tell_launch_held(
    db,
    &LaunchHeld {
      at: 2_000,
      held: vec![],
    },
  )
  .await
  .unwrap();
  assert_eq!(launch_held(db).await.unwrap(), None);
  assert!(!watch_held(db, &roomy(), &bus).await.unwrap());
  assert!(returned.ready().is_none(), "a withdrawn notice is let go without a word");
}
