//! A pressure the node service tells, published again with the worker at work in each place and the job
//! and item its turn named.

use agency_server::peers::node::{
  Alert, KernelReading, Largest, Machine, MachineKind, Pressure, PressureLine, ScopeUse, Standing,
};
use agency_server::watches::machine::AtWork;
use agency_server::watches::pressure::named;
use agent_session::JobMark;
use serde_json::json;

fn machine(standing: Standing) -> Machine {
  Machine {
    name: "local".into(),
    label: "This machine".into(),
    kind: MachineKind::Local,
    tracking: true,
    cpu_limit: None,
    read_at: Some(1_700_000_000_000),
    hardware: None,
    failure: None,
    standing: Some(standing),
  }
}

fn at_work() -> Vec<AtWork> {
  vec![
    AtWork {
      worker: "w1".into(),
      job: Some(JobMark {
        job: 41,
        item: Some(1802),
      }),
    },
    AtWork {
      worker: "w2".into(),
      job: None,
    },
  ]
}

fn line(avg60: f64) -> PressureLine {
  PressureLine {
    avg10: avg60,
    avg60,
    avg300: avg60,
  }
}

fn reading() -> KernelReading {
  KernelReading {
    at: 1_700_000_000_000,
    load1: 30.0,
    threads: 24,
    mem_total_gb: 32.0,
    mem_available_gb: 8.0,
    swap_total_gb: 16.0,
    swap_free_gb: 8.0,
    disk_free_gb: Some(100.0),
    memory: Some(Pressure {
      some: line(12.5),
      full: Some(line(12.5)),
    }),
    cpu: None,
  }
}

#[test]
fn a_memory_pressure_names_the_job_behind_each_place_and_counts_the_jobs() {
  let standing = Standing {
    reading: Some(reading()),
    alerts: vec![],
    browsers: 2,
    scopes: vec![
      ScopeUse {
        scope: "w1".into(),
        megabytes: 900,
        browsers: 2,
      },
      ScopeUse {
        scope: "chat".into(),
        megabytes: 400,
        browsers: 0,
      },
      ScopeUse {
        scope: "w2".into(),
        megabytes: 100,
        browsers: 0,
      },
    ],
    largest: vec![
      Largest {
        pid: 7,
        name: "chrome".into(),
        megabytes: 400,
        scope: Some("w1".into()),
      },
      Largest {
        pid: 9,
        name: "postgres".into(),
        megabytes: 300,
        scope: None,
      },
    ],
  };
  let alert = Alert::Memory {
    full_pct: 12.5,
    alert_pct: 10.0,
    minutes: 2,
  };

  assert_eq!(
    json!(named(&machine(standing), &alert, &at_work())),
    json!({
      "cause": "memory",
      "fullPct": 12.5,
      "alertPct": 10.0,
      "minutes": 2.0,
      "reading": {
        "at": 1_700_000_000_000_i64,
        "load1": 30.0,
        "threads": 24,
        "memTotalGb": 32.0,
        "memAvailableGb": 8.0,
        "swapTotalGb": 16.0,
        "swapFreeGb": 8.0,
        "diskFreeGb": 100.0,
        "memory": {
          "some": { "avg10": 12.5, "avg60": 12.5, "avg300": 12.5 },
          "full": { "avg10": 12.5, "avg60": 12.5, "avg300": 12.5 }
        },
        "cpu": null
      },
      "holders": [
        { "place": "w1", "megabytes": 900.0, "browsers": 2, "worker": "w1", "jobId": 41, "todoId": 1802 },
        { "place": "chat", "megabytes": 400.0, "browsers": 0, "worker": null, "jobId": null, "todoId": null },
        { "place": "w2", "megabytes": 100.0, "browsers": 0, "worker": "w2", "jobId": null, "todoId": null }
      ],
      "largest": [
        { "pid": 7, "name": "chrome", "megabytes": 400.0, "place": "w1", "jobId": 41 },
        { "pid": 9, "name": "postgres", "megabytes": 300.0, "place": null, "jobId": null }
      ],
      "jobs": 2,
      "browserJobs": 1
    })
  );
}

#[test]
fn a_disk_pressure_carries_the_standings_places_and_counts_the_jobs_with_a_browser() {
  let standing = Standing {
    reading: None,
    alerts: vec![],
    browsers: 4,
    scopes: vec![
      ScopeUse {
        scope: "w1".into(),
        megabytes: 900,
        browsers: 1,
      },
      ScopeUse {
        scope: "w2".into(),
        megabytes: 300,
        browsers: 3,
      },
    ],
    largest: vec![],
  };
  let alert = Alert::Disk {
    free_gb: 12.0,
    limit_gb: 20.0,
  };

  assert_eq!(
    json!(named(&machine(standing), &alert, &at_work())),
    json!({
      "cause": "disk",
      "freeGb": 12.0,
      "limitGb": 20.0,
      "reading": null,
      "holders": [
        { "place": "w1", "megabytes": 900.0, "browsers": 1, "worker": "w1", "jobId": 41, "todoId": 1802 },
        { "place": "w2", "megabytes": 300.0, "browsers": 3, "worker": "w2", "jobId": null, "todoId": null }
      ],
      "largest": [],
      "jobs": 2,
      "browserJobs": 2
    })
  );
}
