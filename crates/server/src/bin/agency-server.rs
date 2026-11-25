//! Agency's server: the store settled with the deployment's seeds and the ground stood on it, the places
//! stood in this machine's herdr, the clocks started, the cache siren's quota source on its loopback port,
//! and the control routes on the control port under the server's token, for as long as the process runs
//! (docs/api.md).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use agency_server::clock::every;
use agency_server::config::Config;
use agency_server::control::claude_accounts::Judging;
use agency_server::places::{Ground, Places};
use agency_server::verbs::reload::{Redeploy, TreeRedeploy};
use agency_server::verbs::{CONTROL_PORT, CONTROL_TOKEN, DEFAULT_CONTROL_PORT};
use agency_server::watches::machine;
use agency_server::{Service, archive_place, control, now_ms};
use clap::Parser;
use harness_providers::Machine;
use policy::keepalive::KEEPALIVE;
use policy::machine_room::MACHINE_ROOM;
use policy::sleep::SLEEP;

/// The variable naming the quota source's loopback port, and the port it takes where none is named.
const QUOTA_PORT: &str = "AGENCY_QUOTA_PORT";
const DEFAULT_QUOTA_PORT: u16 = 9200;

// How often the queue's executions are reconciled with what the machine runs.
const RECONCILE_PASS: Duration = Duration::from_secs(60);

/// Where the server stands.
#[derive(Parser)]
#[command(
  name = "agency-server",
  about = "Agency's server: sessions, places, the co-worker pool and the control routes."
)]
struct Args {
  /// The data root the server stands on.
  #[arg(long, env = "BMO_DATA_DIR")]
  data_root: Option<PathBuf>,
  /// The control port every caller speaks to.
  #[arg(long, env = CONTROL_PORT, default_value_t = DEFAULT_CONTROL_PORT)]
  control_port: u16,
  /// The token every caller carries; no front stands without it.
  #[arg(long, env = CONTROL_TOKEN, hide_env_values = true)]
  token: Option<String>,
  /// The cache siren's quota source, a loopback listener of its own beside the control port.
  #[arg(long, env = QUOTA_PORT, default_value_t = DEFAULT_QUOTA_PORT)]
  quota_port: u16,
}

#[tokio::main]
async fn main() -> ExitCode {
  let args = Args::parse();

  match serve(args).await {
    // Drained: the process ends now, with nothing left of it waited on (docs/sessions.md, 「A restart hands
    // the session over」); the turns it carried stay kept in flight for the next server.
    Ok(()) => {
      println!("agency: drained");
      std::process::exit(0)
    }
    Err(why) => {
      eprintln!("agency: {why}");
      ExitCode::FAILURE
    }
  }
}

async fn serve(args: Args) -> Result<(), String> {
  let token = args
    .token
    .filter(|token| !token.is_empty())
    .ok_or(format!("{CONTROL_TOKEN} is not set"))?;
  let data_root = args.data_root.unwrap_or_else(agency_server::verbs::data_root);
  let place = archive_place(&data_root).ok_or_else(|| {
    format!(
      "no store is named for {}: set BMO_ARCHIVE_URL, or name it in env/agency.env or env/archive-url",
      data_root.display()
    )
  })?;
  let db = store::connect(&place, store::pool(agency_server::STORE_CONNECTIONS))
    .await
    .map_err(|why| format!("the store: {why}"))?;

  let machine = Machine::of_process(&data_root);

  agency_server::door::record_secrets::install(&data_root);
  store::seed::settle(&db, &agency_server::seeds::deployment(&machine))
    .await
    .map_err(|fault| format!("the store: {fault}"))?;

  // The supervisors' homes stand in their own folder before any program is stood on one.
  let machine_config = harness_providers::claude_code::ClaudeCode::machine_config_dir(&machine);

  for line in agency_server::supervisor_homes::carried(&data_root, &machine_config) {
    println!("agency: the supervisors' homes: {line}");
  }

  let config = Config::read(|name| std::env::var(name).ok())?;
  let service = Service::new(db.clone(), config);

  // The stream's events are kept in the store and numbered above every number a process before this one
  // gave, before anything publishes one (docs/api.md, 「events」).
  service
    .events
    .durable(&db, agency_server::events::STORED)
    .await
    .map_err(|fault| format!("the event stream's store: {fault}"))?;

  let herdr = herdr::socket_path(machine.var("XDG_CONFIG_HOME"), machine.var("HOME"))
    .map(|socket| herdr::Herdr::here(socket, herdr::silences_path(machine.var("BMO_HERDR_SILENCES"))));
  let watch = herdr.clone().map(herdr::picture::Watch::start);
  let places = Places::new(Ground::new(&service, &machine, herdr), Arc::default());
  let ground = places.ground.clone();

  // A stop drains the server rather than ending it where it stands.
  agency_server::drain::on_stop(ground.drain.clone()).map_err(|why| format!("the stop signals: {why}"))?;

  places.clone().stand(watch.clone());
  // The gate's kinds read before the routes stand, so an agent's switches list its gates from the
  // gate's own declaration and not from a copy agency would keep.
  agency_server::gate_kinds::refresh(&ground).await;
  // Every turn's liveness row the last process left open is closed with what is found of its program now,
  // and the same written to the log, before anything that process left is picked up or taken down
  // (docs/agent-control.md, 「Telling that a turn's program is gone」).
  if let Err(why) = agency_server::door::liveness::close_left_open(&ground.db).await {
    eprintln!("agency: the turns' liveness rows the last process left open: {why:?}");
  }

  // A turn the last process left in flight is handed back where its program's record holds its end, and
  // waited on where the program goes on with it, before anything else reads what that process left.
  if let Err(why) = agency_server::turn_pickup::at_start(&ground).await {
    eprintln!("agency: the turns left in flight: {}", why.detail);
  }

  // A thrown-away session does not outlive the process that started it, so the places such sessions on a
  // road carrying its own turns left are ended, but for one carrying a turn just picked up, which ends with
  // that turn.
  let ended = agency_server::apart_places::sweep(&ground).await;

  if ended > 0 {
    println!("agency: {ended} places of thrown-away sessions the last process left ended");
  }

  // Execution rows the last process left open are closed on what they left, and a job whose process
  // went away with the last one is ended before the routes stand. The same pass runs on its clock.
  if let Err(why) = agency_server::queue::reconcile::pass(&ground.db, &ground.roots.data).await {
    eprintln!("agency: the executions: {}", why.detail);
  }

  // The session contract over the places stands before any job is launched again, its sessions numbered past
  // every number a process before this one gave (docs/sessions.md, 「A restart hands the session over」).
  let front = control::Front::new(token, ground.clone())?.with_places(places, watch.clone());

  if let Some(sessions) = front.sessions() {
    sessions.number_past_kept().await;
  }

  // A job whose turn had not gone out when the last process went away is launched again from what its row
  // keeps, before the pass that ends a job left running reads it (docs/api.md, 「assignments and jobs」).
  let resumed = agency_server::control::work::resume_launches(&ground, front.sessions()).await;

  if resumed > 0 {
    println!("agency: {resumed} jobs whose turn had not gone out launched again");
  }

  if let Err(why) = agency_server::control::work::reap_stale(&ground).await {
    eprintln!("agency: the jobs left running: {}", why.error);
  }

  // A running job keeps its item's lease, whatever the harness that took it does; the same pass runs on its own
  // clock.
  if let Err(why) = agency_server::job_leases::pass(&ground.db, ground.vars()).await {
    eprintln!("agency: the running jobs' leases: {}", why.detail);
  }

  // A run whose turn ended without closing it is closed as cut; the same pass runs on the executions' clock.
  match agency_server::run_sweep::pass(&ground).await {
    Ok(0) => {}
    Ok(closed) => println!("agency: {closed} runs left open by turns that had ended closed as cut"),
    Err(why) => eprintln!("agency: the runs left open: {}", why.detail),
  }

  // The demands a process before this one asked on the arbiter's master and left behind, given back:
  // no run of this process holds any yet (docs/own-llm.md, 「An engine the arbiter stands」).
  if let Some(master) = agency_server::cards::master_of(&ground) {
    match agency_server::cards::give_back_left(&ground.db, &master).await {
      Ok(0) => {}
      Ok(given) => println!("agency: {given} demands left on the arbiter's master given back"),
      Err(why) => eprintln!("agency: the demands left on the arbiter's master: {}", why.detail),
    }
  }

  // A role session is held in memory from its start to its end, so none of the process before stands: every
  // lease bound to one is given back as ended with it (docs/role-slots.md, 「The line」).
  match agency_server::order_run::dispatch::release_every_session(&ground.db, &ground.events).await {
    Ok(0) => {}
    Ok(released) => {
      println!("agency: {released} slot leases bound to the role sessions of the process before given back")
    }
    Err(why) => eprintln!("agency: the slot leases bound to role sessions: {why}"),
  }

  // Every notice owed to an orderer or the owner stands in the outbox: each standing one is told again as the
  // server starts, and each one still owed is tried at once (docs/agent-control.md, 「An item's change is told to
  // its orderer」).
  // A registration a consumer made at a point that outlives a restart stands again before any run asks it
  // (docs/api.md, 「hooks」).
  match agency_server::hooks::restore(&ground.hooks, &ground.db).await {
    Ok(0) => {}
    Ok(restored) => println!("agency: {restored} kept hook registrations stand again"),
    Err(why) => eprintln!("agency: the kept hook registrations: {why}"),
  }

  // What a person's runs owe the supply hook is sent until it is taken, the reservations that expired while the
  // server was down captured first (docs/supply.md, 「Reservations」).
  tokio::spawn(agency_server::supply::reservations::keep(ground.clone()));

  match agency_server::outbox::restarted(&ground.db).await {
    Ok(0) => {}
    Ok(due) => println!("agency: {due} notices owed to orderers are due again with this start"),
    Err(why) => eprintln!("agency: the notices owed: {why}"),
  }

  clocks(&ground);
  tokio::spawn(agency_server::order_run::clocks::keep_bound(
    ground.db.clone(),
    ground.events.clone(),
    agency_server::queue::running::Running::new(ground.db.clone(), &ground.machine(), watch.clone()),
  ));

  // The agents' documents are written afresh on every start, so the runners' directories carry the text
  // this release reads.
  let redeploy = TreeRedeploy {
    bun: ground.roots.bun.clone(),
    tree: ground.code_root(),
  };

  tokio::task::spawn_blocking(move || redeploy.redeploy());

  let quota = ground.clone();

  tokio::spawn(async move {
    if let Err(why) = control::quota::stand(args.quota_port, quota).await {
      eprintln!("agency: the quota source: {why}");
    }
  });

  // The replacements the supervisors' sessions owe, made while they are quiet: once as the server starts, so
  // a deployment that moved a document is settled before anyone asks, and on the keepalive clock after
  // (docs/sessions.md, 「Session replacement」).
  if let Some(sessions) = front.sessions() {
    let at_start = sessions.clone();

    tokio::spawn(async move {
      every(
        "sessions' owed replacements",
        Duration::from_secs(5),
        Duration::from_secs(KEEPALIVE.tick_seconds),
        move || {
          let sessions = at_start.clone();

          async move {
            sessions.settle_pass().await;

            Ok::<(), String>(())
          }
        },
      )
      .await
    });
  }

  // The supervisors' nightly sleep, on a clock of its own, one sleep after another (docs/sleep.md).
  if let Some(sessions) = front.sessions() {
    let night = Arc::new(agency_server::sleep::Nightly {
      ground: ground.clone(),
      sessions,
    });

    tokio::spawn(async move {
      every(
        "supervisors' sleep",
        Duration::from_secs(SLEEP.start_delay_seconds),
        Duration::from_secs(SLEEP.tick_seconds),
        move || {
          let night = night.clone();

          async move {
            agency_server::sleep::pass(&night.ground.db, night.as_ref(), &SLEEP, now_ms())
              .await
              .map(|_| ())
              .map_err(|fault| fault.detail)
          }
        },
      )
      .await
    });
  }

  let listener = control::bind(args.control_port)
    .await
    .map_err(|why| format!("the control port: {why}"))?;

  println!(
    "agency: control routes on http://{}",
    listener.local_addr().map_err(|why| why.to_string())?
  );
  control::serve(listener, front).await.map_err(|why| format!("the control port: {why}"))
}

// The server's own clocks: the run log pruned, the pool's admissions and leftovers, the demands of ended
// leases given back, the pool's warming, the
// accounts' usage windows read, the queue's reconciliation, the logins' watch on the keepalive pass, the held
// launch's watch, the node service's pressures listened to, and what finished items left in the workbenches
// swept.
fn clocks(ground: &Arc<Ground>) {
  tokio::spawn(agency_server::run_log::prune::keep(ground.db.clone()));

  let gated = ground.clone();

  tokio::spawn(async move {
    every("gate kinds", Duration::from_secs(60), Duration::from_secs(60), move || {
      let ground = gated.clone();

      async move {
        agency_server::gate_kinds::refresh(&ground).await;

        Ok(())
      }
    })
    .await
  });
  tokio::spawn(agency_server::order_run::clocks::keep_pool(ground.db.clone(), ground.events.clone()));
  tokio::spawn(agency_server::order_run::clocks::keep_card_demands(ground.clone()));
  tokio::spawn(agency_server::order_run::clocks::keep_leftovers(ground.clone()));
  tokio::spawn(agency_server::order_run::clocks::keep_usage_windows(ground.clone()));
  tokio::spawn(agency_server::rest::keep_watching(ground.clone()));

  let watched = ground.clone();

  tokio::spawn(async move {
    let period = Duration::from_secs(policy::pull_requests::PULL_REQUESTS.fallback_seconds);

    every("pull requests' fallback", period, period, move || {
      let ground = watched.clone();

      async move { agency_server::watches::pull_requests::fallback(&ground.db, ground.vars()).await }
    })
    .await
  });

  let leased = ground.clone();

  tokio::spawn(async move {
    let period = Duration::from_secs(policy::job_leases::JOB_LEASES.renew_seconds);

    every("running jobs' leases", period, period, move || {
      let ground = leased.clone();

      async move {
        agency_server::job_leases::pass(&ground.db, ground.vars())
          .await
          .map(|_| ())
          .map_err(|why| why.detail)
      }
    })
    .await
  });

  let turn = ground.clone();

  tokio::spawn(async move {
    let period = Duration::from_secs(policy::machine_turn::MACHINE_TURN.read_seconds);

    every("machine's turn", Duration::ZERO, period, move || {
      let ground = turn.clone();

      async move {
        agency_server::watches::machine_turn::pass(
          &ground.db,
          ground.vars(),
          &ground.roots.data,
          &ground.roots.home,
        )
        .await;

        Ok(())
      }
    })
    .await
  });

  let deployments = ground.clone();

  // Every deployment that ended without going live, told to the orderers of the items it carries.
  tokio::spawn(async move {
    let period = Duration::from_secs(policy::job_notices::JOB_NOTICES.deploy_read_seconds);

    every("deploy watch", Duration::ZERO, period, move || {
      let ground = deployments.clone();

      async move {
        agency_server::watches::deploy_failures::pass(&ground.db, ground.vars(), &ground.roots.home).await;

        Ok(())
      }
    })
    .await
  });

  let flush = ground.clone();

  // The whole deploy queue put live once the flush minutes have passed since the last flush.
  tokio::spawn(async move {
    let period = Duration::from_secs(policy::job_notices::JOB_NOTICES.deploy_flush_read_seconds);

    every("deploy flush", period, period, move || {
      let ground = flush.clone();

      async move {
        agency_server::watches::deploy_flush::pass(&ground.db, ground.vars(), &ground.roots.home).await;

        Ok(())
      }
    })
    .await
  });

  let background = ground.clone();
  let background_watch =
    Arc::new(tokio::sync::Mutex::new(agency_server::watches::background_work::Watch::default()));

  // Every supervisor's background work after the turn that launched it: told to its room as it ends or is lost,
  // and held to the call clock while it runs.
  tokio::spawn(async move {
    let period = Duration::from_secs(policy::background_work::BACKGROUND_WORK.look_seconds);

    every("background work", Duration::ZERO, period, move || {
      let ground = background.clone();
      let watch = background_watch.clone();

      async move {
        let mut watch = watch.lock().await;

        agency_server::watches::background_work::pass(
          &ground.db,
          ground.vars(),
          &ground.events,
          &ground.roots.data,
          &mut watch,
        )
        .await;

        Ok(())
      }
    })
    .await
  });

  let owed = ground.clone();

  // The outbox's sender: the standing notices whose condition ended let go first, then every row due sent.
  tokio::spawn(async move {
    let period = Duration::from_secs(policy::job_notices::JOB_NOTICES.send_seconds);

    every("notices owed", Duration::ZERO, period, move || {
      let ground = owed.clone();

      async move { agency_server::watches::notices_owed::pass(&ground.db, ground.vars()).await.map(|_| ()) }
    })
    .await
  });

  let copies = ground.clone();

  // The shared apps' copies kept on each PC heard again since the last look (docs/person-nodes.md, 「Keeping the
  // copies」).
  tokio::spawn(async move {
    let period = Duration::from_secs(policy::person_nodes::PERSON_NODES.copies_seconds);

    every("shared apps' copies", period, period, move || {
      let ground = copies.clone();

      async move { agency_server::catalogue::pass(&ground).await }
    })
    .await
  });

  let idle = ground.clone();

  tokio::spawn(async move {
    let period = Duration::from_secs(policy::queue_idle::QUEUE_IDLE.check_seconds);

    every("queue's minute pass", period, period, move || {
      let ground = idle.clone();

      async move {
        agency_server::watches::queue_idle::pass(&ground.db, ground.vars(), &ground.peers.gate).await
      }
    })
    .await
  });

  let reconciled = ground.clone();

  tokio::spawn(async move {
    every("queue reconciliation", RECONCILE_PASS, RECONCILE_PASS, move || {
      let ground = reconciled.clone();

      async move {
        agency_server::queue::reconcile::pass(&ground.db, &ground.roots.data)
          .await
          .map_err(|refusal| refusal.detail)?;
        if let Err(refusal) =
          agency_server::queue::cancels::swept(&ground.db, agency_server::now_ms()).await
        {
          eprintln!(
            "queue reconciliation: the cancellations past their grace were not swept: {}",
            refusal.detail
          );
        }
        agency_server::turn_pickup::reap_dead(&ground).await;
        agency_server::turn_keys::prune(&ground.db).await;
        agency_server::control::work::reap_stale(&ground)
          .await
          .map(|_| ())
          .map_err(|refusal| refusal.error)?;
        agency_server::run_sweep::pass(&ground)
          .await
          .map(|_| ())
          .map_err(|refusal| refusal.detail)
      }
    })
    .await
  });

  let watched = ground.clone();
  let first = Duration::from_secs(KEEPALIVE.start_delay_seconds);
  let period = Duration::from_secs(KEEPALIVE.tick_seconds);

  tokio::spawn(async move {
    every("logins' watch", first, period, move || {
      let ground = watched.clone();

      async move {
        agency_server::keepalive::logins::watch(
          Judging::of(&ground),
          &ground.events,
          &ground.roots,
          now_ms(),
        )
        .await
        .map_err(|refusal| refusal.detail)
      }
    })
    .await
  });

  // The warming of the agents' kept sessions, on the keepalive pass's own clock, one ping at a time.
  let warmed = ground.clone();

  tokio::spawn(async move {
    every("agents' warming", first, period, move || {
      let ground = warmed.clone();

      async move { agency_server::keepalive::agents::pass(&ground, now_ms).await.map(|_| ()) }
    })
    .await
  });

  let held = ground.clone();
  let period = Duration::from_secs(MACHINE_ROOM.clocks.held_watch_seconds);

  tokio::spawn(async move {
    every("held launch's watch", period, period, move || {
      let ground = held.clone();

      async move {
        machine::watch_held(&ground.db, ground.peers.node.as_ref(), &ground.events)
          .await
          .map(|_| ())
          .map_err(|refusal| refusal.detail)
      }
    })
    .await
  });

  tokio::spawn(agency_server::watches::pressure::relay(ground.clone()));

  let swept = ground.clone();
  let sweep = Arc::new(agency_server::places::item_sweep::Sweep::default());
  let period = Duration::from_secs(MACHINE_ROOM.clocks.item_sweep_seconds);

  tokio::spawn(async move {
    every("item scratch sweep", Duration::ZERO, period, move || {
      let (ground, sweep) = (swept.clone(), sweep.clone());

      async move {
        let cluster = agency_server::places::item_sweep::StoreCluster(&ground.db);

        sweep.sweep(&ground, &cluster).await.map(|_| ()).map_err(|refusal| refusal.detail)
      }
    })
    .await
  });
}
