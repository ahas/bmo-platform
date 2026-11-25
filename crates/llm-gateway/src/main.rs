//! The model gateway the unit `bmo-llm-gateway` runs (docs/own-llm.md, 「The gateway」).
//!
//!   llm-gateway [--port <port>]
//!
//! The port is 18030 unless named; a check stands a launcher of its own on another. The store is the one the
//! server runs on, found as every reader of it finds it (docs/store.md, 「Connecting」).

use std::collections::HashMap;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use harness_machine::card_master::{Master, MasterAddress, entry_of};
use harness_machine::members::{self, Member};
use llm_gateway::child::{Place, start, write_private};
use llm_gateway::costs::Records;
use llm_gateway::door::Door;
use llm_gateway::front::{Front, Status, router};
use llm_gateway::keys::read_key;
use llm_gateway::supply::{self, Supply};
use llm_gateway::table::{
  HOOKS, Personal, Table, gateway_table_with_personal, laid_instances, owners, personal_members,
  table_digest,
};
use sqlx::Connection;
use store::open_read;
use store::registry::{Registration, registrations};
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};

fn home() -> PathBuf {
  PathBuf::from(std::env::var_os("HOME").filter(|home| !home.is_empty()).unwrap_or_else(|| "/".into()))
}

fn named(variable: &str) -> Option<String> {
  std::env::var(variable)
    .ok()
    .map(|value| value.trim().to_owned())
    .filter(|value| !value.is_empty())
}

// The tree a deployed binary stands in, `<tree>/bin/llm-gateway`, unless `BMO_CODE_ROOT` names one. The
// tree is the one the launcher was started by, not the one that path leads to: its unit starts it
// through the deploy link, and a deployment that leaves the launcher as it stands leaves it running, so
// only through the link does it read the hooks of the release that is live.
fn code_root() -> PathBuf {
  named("BMO_CODE_ROOT").map(PathBuf::from).unwrap_or_else(|| {
    started_by()
      .and_then(|exe| Some(exe.parent()?.parent()?.to_owned()))
      .unwrap_or_else(|| PathBuf::from("."))
  })
}

// The path the launcher was started by where it is a whole one, a link in it left as it is; else where
// the running binary stands.
fn started_by() -> Option<PathBuf> {
  std::env::args_os()
    .next()
    .map(PathBuf::from)
    .filter(|path| path.is_absolute())
    .or_else(|| std::env::current_exe().ok())
}

/// The port named after `--port`, or 18030; anything else named is no port. A port of 0 is one the
/// system chooses, which the launcher names as it starts serving.
fn port_of(args: &[String]) -> Option<u16> {
  match args.iter().position(|arg| arg == "--port") {
    None => Some(18_030),
    Some(at) => args.get(at + 1)?.parse::<u16>().ok(),
  }
}

/// The store's address, read as the server reads it, so the gateway serves the registry the server runs on.
fn store_address(data_root: &Path) -> Option<String> {
  store::place::place(data_root)
}

/// The registry as it stands, read without a write: the store is opened read-only each time, so the
/// gateway never holds a lock the bot waits on.
async fn registry(address: &str) -> Result<Vec<Registration>, sqlx::Error> {
  let mut store = open_read(address).await?;
  let read = registrations(&mut store).await;
  let _ = store.close().await;
  read
}

/// The table of the registry and the members. A registration naming a card service is served by the
/// instances the arbiter's master reports standing, its picture read on the same beat; a picture that
/// cannot be read leaves the last good table standing (docs/own-llm.md, 「An engine the arbiter stands」).
/// The front is told each registration naming a card service as the registry reads, whatever the table.
async fn registry_table(
  front: &Front,
  registry: Vec<Registration>,
  master: Option<&Master>,
  members: &[Member],
  personal: &[Personal],
) -> Option<Table> {
  front.know(
    registry
      .iter()
      .filter_map(|one| Some((one.name.clone(), one.card_service.clone()?)))
      .collect::<HashMap<_, _>>(),
  );
  let picture = match (master, registry.iter().any(|one| one.card_service.is_some())) {
    (Some(master), true) => match master.picture().await {
      Ok(picture) => Some(picture),
      Err(fault) => {
        eprintln!("llm-gateway: {fault}; the last good table stands");
        return None;
      }
    },
    _ => None,
  };
  let laid = laid_instances(registry, picture.as_ref());
  match gateway_table_with_personal(&laid, members, personal) {
    Ok(table) => Some(table),
    Err(why) => {
      eprintln!("llm-gateway: the table was refused: {why}");
      None
    }
  }
}

// What the console reads of the gateway, written whole each time it changes.
fn write_status(path: &Path, status: &Status) {
  let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as u64);
  let mut written = serde_json::to_value(status).expect("the status is plain data");
  written["at"] = at.into();
  let _ = write_private(path, &format!("{written}\n"));
}

/// What the last good readings of the members file and the registry held, which stand where a beat's
/// reading fails.
#[derive(Default)]
struct Last {
  members: Vec<Member>,
  registry: Vec<Registration>,
}

async fn refresh(
  front: &Front,
  place: &Place,
  store: &str,
  master: Option<&Master>,
  members_path: &Path,
  last: &mut Last,
) {
  let current = match members::read(members_path) {
    Ok(current) => {
      last.members.clone_from(&current);
      current
    }
    Err(why) => {
      eprintln!("llm-gateway: the members file does not read: {why}; the last good members stand");
      last.members.clone()
    }
  };
  // The call keys, the people's own keys and the reservations, read on the same beat; a reading that fails
  // leaves the last good one standing.
  match supply::read(store).await {
    Ok(reading) => front.supply().know(reading),
    Err(fault) => eprintln!("llm-gateway: the call keys and supply could not be read: {fault}"),
  }
  let personal = personal_members(&current, &front.supply().personal());
  // A registry not settled yet — the gateway coming up before the bot — reads as nothing to serve for now,
  // and one that cannot be read leaves the last good one standing for the owners and no new table.
  let read = registry(store).await;
  if let Ok(registry) = &read {
    last.registry.clone_from(registry);
  }
  // Who owns what is read on every beat, whatever becomes of the table (docs/own-llm.md, 「The gateway's
  // members」).
  front.own(owners(&last.registry, &current, &personal));
  let registry = match read {
    Ok(registry) => registry,
    Err(fault) => {
      eprintln!("llm-gateway: the registry could not be read: {fault}");
      return;
    }
  };
  let Some(table) = registry_table(front, registry, master, &current, &personal).await else {
    return;
  };
  let digest = table_digest(&table, |key_file| read_key(&place.env_dir, key_file), &place.hook_texts());
  let started = start(place, &table, &digest);
  if let Err(fault) = front.take(&digest, table.names.clone(), started).await {
    eprintln!("llm-gateway: the table {digest} did not start: {fault}");
  }
}

#[tokio::main]
async fn main() -> ExitCode {
  let args: Vec<String> = std::env::args().skip(1).collect();
  let Some(port) = port_of(&args) else {
    eprintln!("llm-gateway: --port takes a port number.");
    return ExitCode::FAILURE;
  };
  let data_root = named("BMO_DATA_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| home().join(".local/share/bmo/data"));
  let Some(store) = store_address(&data_root) else {
    eprintln!(
      "llm-gateway: no store is named, neither by BMO_ARCHIVE_URL nor in {} or {}",
      data_root.join(store::place::SERVICE_ENV).display(),
      data_root.join(store::place::ADDRESS_FILE).display()
    );
    return ExitCode::FAILURE;
  };
  // The master as the server's settings name it: the launcher's own environment first, else the two
  // entries of the server's settings file, and nothing else of that file.
  let settings = data_root.join("env").join("agency.env");
  let master = match MasterAddress::read(|name| {
    std::env::var(name)
      .ok()
      .filter(|value| !value.trim().is_empty())
      .or_else(|| entry_of(&settings, name))
  }) {
    Ok(address) => address.map(Master::new),
    Err(why) => {
      eprintln!("llm-gateway: {why}");
      return ExitCode::FAILURE;
    }
  };
  let members_path = members::path(&data_root);
  let hooks_dir = code_root().join("crates/llm-gateway/hooks");
  let env_dir = data_root.join("env");
  let refresh_every = Duration::from_millis(
    named("LLM_GATEWAY_REFRESH_MS")
      .and_then(|ms| ms.parse().ok())
      .filter(|ms| *ms > 0)
      .unwrap_or(5_000),
  );
  // How long a streamed answer may stay silent: the policy's span, unless a check names a shorter one.
  let stall = named("LLM_GATEWAY_STALL_MS")
    .and_then(|ms| ms.parse().ok())
    .filter(|ms| *ms > 0)
    .map_or(Duration::from_secs(policy::gateway::GATEWAY.stall_seconds), Duration::from_millis);

  // Its own key is the only door every request passes, so without it the gateway does not stand at all.
  // It is read once here: the key the front checks and the one each child is handed.
  let Some(gateway_key) = read_key(&env_dir, "llm-gateway.key").filter(|key| !key.is_empty()) else {
    eprintln!(
      "llm-gateway: {} is missing, empty or not 0600, so the gateway does not start",
      env_dir.join("llm-gateway.key").display()
    );
    return ExitCode::FAILURE;
  };
  let listener = match TcpListener::bind(("127.0.0.1", port)).await {
    Ok(listener) => listener,
    Err(fault) => {
      eprintln!("llm-gateway: 127.0.0.1:{port} could not be listened on: {fault}");
      return ExitCode::FAILURE;
    }
  };
  let port = listener.local_addr().map_or(port, |at| at.port());
  let place = Arc::new(Place {
    folder: data_root.join("llm-gateway"),
    env_dir,
    gateway_key: gateway_key.clone(),
    hooks: HOOKS.iter().map(|hook| hooks_dir.join(format!("{hook}.py"))).collect(),
    front: format!("http://127.0.0.1:{port}"),
  });
  if let Err(fault) = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&place.folder) {
    eprintln!("llm-gateway: {} could not be made: {fault}", place.folder.display());
    return ExitCode::FAILURE;
  }
  place.clear_leftovers();

  let status_path = place.folder.join("status.json");
  // The door asks the master for the cards of the requests on an engine standing nowhere; a demand a
  // launcher that ended left under its reference is given back first.
  let door = master.clone().map(Door::new);
  // The request records are written on a connection of their own, the one write the launcher makes.
  let records = match Records::new(&store) {
    Ok(records) => records,
    Err(why) => {
      eprintln!("llm-gateway: {why}");
      return ExitCode::FAILURE;
    }
  };
  let front = Front::new(gateway_key, door, stall, Supply::new(store.clone()), records, {
    let path = status_path.clone();
    move |status| write_status(&path, status)
  });
  println!("llm-gateway: a streamed answer silent for {}ms is cut", stall.as_millis());
  println!("llm-gateway: serving on http://127.0.0.1:{port}");
  write_status(&status_path, &front.status());

  // The registry read again on a beat; a reading still under way — a child starting — lets the beats
  // it spans go by.
  let refreshing = tokio::spawn({
    let (front, place) = (Arc::clone(&front), Arc::clone(&place));
    let members_path = members_path.clone();
    async move {
      let mut last = Last::default();
      let mut beat = tokio::time::interval(refresh_every);
      beat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
      loop {
        beat.tick().await;
        refresh(&front, &place, &store, master.as_ref(), &members_path, &mut last).await;
      }
    }
  });

  let (Ok(mut terminate), Ok(mut interrupt)) =
    (signal(SignalKind::terminate()), signal(SignalKind::interrupt()))
  else {
    eprintln!("llm-gateway: the stop signals could not be listened for");
    return ExitCode::FAILURE;
  };
  // A stop drains the front (docs/own-llm.md, 「The gateway」): from the signal on no connection is taken, and
  // every request in hand ends as it would have, however long it takes, before the children are stopped.
  let stopped = async move {
    let named = tokio::select! {
      _ = terminate.recv() => "SIGTERM",
      _ = interrupt.recv() => "SIGINT",
    };

    println!("llm-gateway: {named}: draining the requests in hand");
  };

  if let Err(fault) =
    axum::serve(listener, router(Arc::clone(&front))).with_graceful_shutdown(stopped).await
  {
    eprintln!("llm-gateway: {fault}");
  }

  println!("llm-gateway: drained");
  refreshing.abort();
  front.close().await;
  let _ = std::fs::remove_file(&status_path);
  ExitCode::SUCCESS
}
