//! The far end's side of the road home on a person's PC (docs/person-nodes.md, 「The link over jake」): the ports
//! the near end names, listened on at the PC's loopback beside the programs, and the node's bridge socket; each
//! connection taken there held while the near end is asked over the mirror to dial it home, and joined to the
//! stream the near end opens for it.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::frame::{Frame, Home, Refused};

/// How long a connection taken waits to be dialed home before it is closed.
pub const DIAL_WAIT: Duration = Duration::from_secs(20);

/// How often a port the far end could not listen on is tried again.
pub const LISTEN_AGAIN: Duration = Duration::from_secs(30);

/// A connection either end joins: a TCP connection or one to a unix socket.
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

enum Listening {
  Standing(JoinHandle<()>),
  Refused(String),
}

impl Drop for Listening {
  fn drop(&mut self) {
    if let Self::Standing(accepting) = self {
      accepting.abort();
    }
  }
}

/// A mirror session standing on the far end, and the ports it asked to be carried.
struct Standing {
  session: mpsc::UnboundedSender<Frame>,
  ports: Vec<u16>,
}

/// What the far end carries home: the ports its standing sessions asked for and how each stands, the sessions
/// standing, the newest of which dials, and the connections waiting to be dialed, by the number each was given.
pub struct Carrier {
  host: String,
  sessions: Mutex<Vec<Standing>>,
  listens: Mutex<BTreeMap<u16, Listening>>,
  told: Mutex<Option<Vec<Refused>>>,
  held: Mutex<HashMap<u64, Box<dyn Stream>>>,
  next: AtomicU64,
}

impl Carrier {
  /// A carrier listening at the loopback address `host`, the one the mirror is spoken on.
  pub fn new(host: &str) -> Arc<Self> {
    Arc::new(Self {
      host: host.to_owned(),
      sessions: Mutex::new(Vec::new()),
      listens: Mutex::new(BTreeMap::new()),
      told: Mutex::new(None),
      held: Mutex::new(HashMap::new()),
      next: AtomicU64::new(1),
    })
  }

  /// A mirror session standing from now on, the newest, which dials until a newer one stands.
  pub fn attach(&self, session: mpsc::UnboundedSender<Frame>) {
    if let Ok(mut sessions) = self.sessions.lock() {
      sessions.push(Standing {
        session,
        ports: Vec::new(),
      });
    }
  }

  /// A mirror session ended: it dials no more, and the newest still standing dials in its place.
  pub fn detach(&self, session: &mpsc::UnboundedSender<Frame>) {
    if let Ok(mut sessions) = self.sessions.lock() {
      sessions.retain(|standing| !standing.session.same_channel(session));
    }
  }

  /// The session that dials: the newest standing.
  fn dialing(&self) -> Option<mpsc::UnboundedSender<Frame>> {
    self
      .sessions
      .lock()
      .ok()
      .and_then(|sessions| sessions.last().map(|newest| newest.session.clone()))
  }

  fn say_to_all(&self, frame: &Frame) {
    if let Ok(sessions) = self.sessions.lock() {
      for standing in sessions.iter() {
        let _ = standing.session.send(frame.clone());
      }
    }
  }

  /// The ports a session asks for recorded as its own, where one asks; every port a standing session asks for
  /// listened on, one already standing kept and one no session asks for any more let go; and which could not be
  /// listened on told to the session that asked, and to every standing session where that changed.
  pub async fn carry(self: &Arc<Self>, asked: Option<(&mpsc::UnboundedSender<Frame>, &[u16])>) {
    let wanted: Vec<u16> = {
      let asking = match asked {
        Some((session, ports)) => {
          let Ok(mut sessions) = self.sessions.lock() else { return };

          for standing in sessions.iter_mut().filter(|standing| standing.session.same_channel(session)) {
            standing.ports = ports.to_vec();
          }

          Some(
            sessions
              .iter()
              .flat_map(|standing| standing.ports.iter().copied())
              .collect::<Vec<u16>>(),
          )
        }
        None => None,
      };
      let Ok(mut listens) = self.listens.lock() else { return };

      if let Some(ports) = asking {
        listens.retain(|port, _| ports.contains(port));

        for port in ports {
          listens.entry(port).or_insert_with(|| Listening::Refused(String::new()));
        }
      }

      listens
        .iter()
        .filter(|(_, listening)| matches!(listening, Listening::Refused(_)))
        .map(|(port, _)| *port)
        .collect()
    };

    for port in wanted {
      let listening = match tokio::net::TcpListener::bind((self.host.as_str(), port)).await {
        Ok(listener) => Listening::Standing(tokio::spawn(take_port(listener, self.clone(), port))),
        Err(why) => Listening::Refused(why.to_string()),
      };

      if let Ok(mut listens) = self.listens.lock()
        && let Some(slot) = listens.get_mut(&port)
      {
        *slot = listening;
      }
    }

    let refused: Vec<Refused> = match self.listens.lock() {
      Ok(listens) => listens
        .iter()
        .filter_map(|(port, listening)| match listening {
          Listening::Refused(why) => Some(Refused {
            port: *port,
            why: why.clone(),
          }),
          Listening::Standing(_) => None,
        })
        .collect(),
      Err(_) => return,
    };
    let changed = match self.told.lock() {
      Ok(mut told) => {
        let changed = told.as_ref() != Some(&refused);

        *told = Some(refused.clone());
        changed
      }
      Err(_) => true,
    };

    let told = Frame::Carried { refused };

    if changed {
      self.say_to_all(&told);
    } else if let Some((session, _)) = asked {
      let _ = session.send(told);
    }
  }

  /// A connection taken, held under a number of its own while the session is asked to dial it home, and closed
  /// where no session stands or nothing dials it in time.
  pub fn hold(self: &Arc<Self>, connection: Box<dyn Stream>, to: Home) {
    let id = self.next.fetch_add(1, Ordering::Relaxed);
    let Some(dialing) = self.dialing() else { return };

    if let Ok(mut held) = self.held.lock() {
      held.insert(id, connection);
    }

    if dialing.send(Frame::Dial { id, to }).is_err() {
      self.take(id);
      return;
    }

    let carrier = self.clone();

    tokio::spawn(async move {
      tokio::time::sleep(DIAL_WAIT).await;
      carrier.take(id);
    });
  }

  /// The connection held under a number, taken from the held.
  pub fn take(&self, id: u64) -> Option<Box<dyn Stream>> {
    self.held.lock().ok().and_then(|mut held| held.remove(&id))
  }
}

// Each connection taken on a carried port held to be dialed home.
async fn take_port(listener: tokio::net::TcpListener, carrier: Arc<Carrier>, port: u16) {
  while let Ok((connection, _)) = listener.accept().await {
    carrier.hold(Box::new(connection), Home::Port(port));
  }
}

/// Each connection taken on the node's bridge socket held to be dialed home.
pub async fn take_bridge(listener: tokio::net::UnixListener, carrier: Arc<Carrier>) {
  while let Ok((connection, _)) = listener.accept().await {
    carrier.hold(Box::new(connection), Home::Bridge);
  }
}

/// The ports that could not be listened on tried again every while, the session told where that changed.
pub async fn listen_again(carrier: Arc<Carrier>) {
  loop {
    tokio::time::sleep(LISTEN_AGAIN).await;
    carrier.carry(None).await;
  }
}
