//! The services agency calls (docs/api.md, 「Calls agency makes」): the node service and the brain. Each is
//! reached through the client its repository publishes, `bmo-node-client` and `bmo-brain-client`, and by a
//! default that calls nothing where the deployment names no address.

pub mod brain;
pub mod gate;
pub mod node;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use url::Url;

use self::brain::Brain;
use self::gate::Gate;
use self::node::NodeService;
use crate::config::PeerAddresses;

/// A call's answer as a client hands it back.
pub type Asking<'a, T> = Pin<Box<dyn Future<Output = Result<T, PeerFault>> + Send + 'a>>;

/// The variable a service's address and its token are named in.
#[derive(Clone, Copy, Debug)]
pub struct Variables {
  pub url: &'static str,
  pub token: &'static str,
}

/// Where a service is reached, and the token it is called with where one is named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerAddress {
  pub url: Url,
  pub token: Option<String>,
}

impl PeerAddress {
  /// The address `variables` name through `variable`: none where the address is unset or blank, and a
  /// fault naming the variable where it does not read as an HTTP URL.
  pub fn read(
    variables: &Variables,
    variable: impl Fn(&str) -> Option<String>,
  ) -> Result<Option<Self>, String> {
    let set =
      |name: &str| variable(name).map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
    let Some(named) = set(variables.url) else {
      return Ok(None);
    };
    let url =
      Url::parse(&named).map_err(|why| format!("{}: {named} is not an address: {why}", variables.url))?;

    if !matches!(url.scheme(), "http" | "https") {
      return Err(format!("{}: {named} is not an HTTP address", variables.url));
    }

    Ok(Some(Self {
      url,
      token: crate::places::token_or_file(
        set(variables.token),
        set(&crate::places::file_variable(variables.token)),
      ),
    }))
  }
}

/// Why a call came back without its answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerFault {
  /// The service was not reached, or its answer did not arrive.
  Unreachable { peer: &'static str, detail: String },
  /// The service understood the call and would not carry it out.
  Refused {
    peer: &'static str,
    code: String,
    message: String,
  },
  /// The service answered with a status that is not a success.
  Failed {
    peer: &'static str,
    status: u16,
    code: String,
    error: String,
  },
  /// The service answered in a shape the call does not read.
  Malformed { peer: &'static str, detail: String },
}

impl std::fmt::Display for PeerFault {
  fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Unreachable { peer, detail } => write!(out, "the {peer} could not be reached: {detail}"),
      Self::Refused {
        peer,
        code,
        message,
      } => write!(out, "the {peer} refused: {message} ({code})"),
      Self::Failed {
        peer,
        status,
        code,
        error,
      } => write!(out, "the {peer} failed with {status} {code}: {error}"),
      Self::Malformed { peer, detail } => write!(out, "the {peer} answered out of shape: {detail}"),
    }
  }
}

impl std::error::Error for PeerFault {}

/// The services agency calls, each by the client its address chose.
#[derive(Clone)]
pub struct Peers {
  pub node: Arc<dyn NodeService>,
  pub brain: Arc<dyn Brain>,
  pub gate: Arc<dyn Gate>,
}

impl Peers {
  /// Each service called over HTTP where its address is named, and answered by its default where not.
  pub fn of(addresses: &PeerAddresses) -> Self {
    Self {
      node: node::client(addresses.node.clone()),
      brain: brain::client(addresses.brain.clone(), addresses.brain_account.clone()),
      gate: gate::client(addresses.gate.clone()),
    }
  }
}
