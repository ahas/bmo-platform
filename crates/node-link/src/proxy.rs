//! The one road a confined personal tool has to the network (docs/person-nodes.md, 「The network and secrets of a
//! call」): a proxy the runner serves for one call on a socket bound into the tool's confinement, which reaches the
//! addresses the tool's manifest lists and no other, and the forwarder that stands inside the confinement, a listener
//! on the confinement's own loopback joined to that socket. The confinement keeps a network of its own with no way
//! out but the socket, so a raw socket, a name of its own resolving or an address written whole reaches nothing.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{HeaderName, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};

/// The variable a test names stand-in addresses in, `name=address` parted by commas: a name it lists is reached at
/// that address whatever it is, the address of this machine among them, so a stand-in server answers for it.
pub const RESOLVE_VARIABLE: &str = "NODE_LINK_RESOLVE";

/// Where the forwarder listens inside the confinement, and the address a confined tool's proxy variables name.
pub const LISTEN_AT: &str = "127.0.0.1:3128";

/// The variables a confined tool reaching the network finds its proxy in, each in the two cases programs read.
pub const PROXY_VARIABLES: [&str; 6] =
  ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"];

/// How the folder of a call's proxy is named, before what makes it its own.
const CALL_FOLDER: &str = "bmo-call-";

/// The headers a proxy reads for itself and sends no further.
const OWN_HEADERS: [&str; 2] = ["proxy-authorization", "proxy-connection"];

/// One address a personal tool may reach, as its manifest writes it (bmo-pocket's docs/api.md, 「`[permissions]`」):
/// a host name, or every name under a domain where it opens on `*.`, on the one port it names, or on 80 and 443.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
  host: String,
  subdomains: bool,
  port: Option<u16>,
}

impl Address {
  /// The address as a manifest writes it, or why it is none: a name of two labels at least, never an IP address
  /// or this machine's own name.
  pub fn read(written: &str) -> Result<Self, String> {
    let (name, port) = match written.rsplit_once(':') {
      Some((name, port)) => match port.parse::<u16>() {
        Ok(port) if port > 0 => (name, Some(port)),
        _ => return Err(format!("{written:?} names no port between 1 and 65535")),
      },
      None => (written, None),
    };
    let (host, subdomains) = match name.strip_prefix("*.") {
      Some(domain) => (domain, true),
      None => (name, false),
    };
    let host = host.to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    let label = |label: &&str| {
      !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label.chars().all(|one| one.is_ascii_alphanumeric() || one == '-')
    };

    if host.len() > 253 || labels.len() < 2 || !labels.iter().all(label) {
      return Err(format!("{written:?} is no host name"));
    }

    if labels.last().is_some_and(|last| last.chars().all(|one| one.is_ascii_digit())) {
      return Err(format!("{written:?} is an IP address, and a tool reaches a name alone"));
    }

    if labels.last() == Some(&"localhost") {
      return Err(format!("{written:?} names this machine, which no tool reaches"));
    }

    Ok(Self {
      host,
      subdomains,
      port,
    })
  }

  /// Whether it lets a call reach `host` at `port`.
  pub fn admits(&self, host: &str, port: u16) -> bool {
    let named = if self.subdomains {
      host.strip_suffix(&self.host).is_some_and(|head| head.len() > 1 && head.ends_with('.'))
    } else {
      host == self.host
    };

    named && self.port.map_or(port == 80 || port == 443, |own| own == port)
  }
}

/// What one call's proxy lets through: the addresses listed, and the stand-in addresses a test names.
#[derive(Clone, Debug, Default)]
pub struct Reach {
  pub addresses: Vec<Address>,
  pub stand_ins: BTreeMap<String, IpAddr>,
}

impl Reach {
  /// The addresses a manifest lists, or why one does not read.
  pub fn of(listed: &[String], stand_ins: BTreeMap<String, IpAddr>) -> Result<Self, String> {
    let addresses = listed.iter().map(|one| Address::read(one)).collect::<Result<_, _>>()?;

    Ok(Self {
      addresses,
      stand_ins,
    })
  }

  /// Where `host` at `port` is reached, or why it is not: a name listed, resolved to addresses outside this
  /// machine, or to its stand-in.
  pub async fn reach(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    let host = host
      .trim_start_matches('[')
      .trim_end_matches(']')
      .trim_end_matches('.')
      .to_ascii_lowercase();

    if host.parse::<IpAddr>().is_ok() {
      return Err(format!("{host} is an address, and this app reaches the names its manifest lists alone"));
    }

    if !self.addresses.iter().any(|address| address.admits(&host, port)) {
      return Err(format!("{host}:{port} is not among the addresses this app may reach"));
    }

    if let Some(stand_in) = self.stand_ins.get(&host) {
      return Ok(vec![SocketAddr::new(*stand_in, port)]);
    }

    let found: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
      .await
      .map_err(|why| format!("{host} was not found: {why}"))?
      .filter(|found| !this_machine(found.ip()))
      .collect();

    if found.is_empty() {
      return Err(format!("{host} resolves to no address outside this machine"));
    }

    Ok(found)
  }
}

/// The stand-in addresses a test names in [`RESOLVE_VARIABLE`], every pair that reads.
pub fn stand_ins(named: &str) -> BTreeMap<String, IpAddr> {
  named
    .split(',')
    .filter_map(|pair| pair.split_once('='))
    .filter_map(|(name, address)| Some((name.trim().to_ascii_lowercase(), address.trim().parse().ok()?)))
    .collect()
}

/// Whether an address is this machine's or its links' alone: a loopback, an unspecified, a link-local, a multicast
/// or the broadcast address, written in either family.
pub fn this_machine(address: IpAddr) -> bool {
  match address {
    IpAddr::V4(v4) => {
      v4.is_loopback()
        || v4.is_unspecified()
        || v4.is_link_local()
        || v4.is_multicast()
        || v4.is_broadcast()
    }
    IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
      Some(v4) => this_machine(IpAddr::V4(v4)),
      None => {
        v6.is_loopback()
          || v6.is_unspecified()
          || v6.is_multicast()
          || (v6.segments()[0] & 0xffc0) == 0xfe80
      }
    },
  }
}

type Answer = Response<BoxBody<Bytes, hyper::Error>>;

fn said(status: StatusCode, why: String) -> Answer {
  let mut answer =
    Response::new(Full::new(Bytes::from(why + "\n")).map_err(|never| match never {}).boxed());

  *answer.status_mut() = status;
  answer
}

// The first address of those found that answers.
async fn dial(found: &[SocketAddr]) -> Result<TcpStream, String> {
  let mut last = "no address".to_owned();

  for address in found {
    match TcpStream::connect(address).await {
      Ok(stream) => return Ok(stream),
      Err(why) => last = format!("{address}: {why}"),
    }
  }

  Err(last)
}

// One request asked of the proxy: a tunnel to the address a `CONNECT` names, or a plain HTTP request naming its
// address whole, sent on and its answer carried back; refused 403 where the address is not reached and 400 where
// the request names none.
async fn proxied(asked: Request<Incoming>, reach: Arc<Reach>) -> Result<Answer, Infallible> {
  if asked.method() == Method::CONNECT {
    let Some(authority) = asked.uri().authority().cloned() else {
      return Ok(said(StatusCode::BAD_REQUEST, "a CONNECT names a host and a port".to_owned()));
    };
    let found = match reach.reach(authority.host(), authority.port_u16().unwrap_or(443)).await {
      Ok(found) => found,
      Err(why) => return Ok(said(StatusCode::FORBIDDEN, why)),
    };
    let mut there = match dial(&found).await {
      Ok(there) => there,
      Err(why) => return Ok(said(StatusCode::BAD_GATEWAY, why)),
    };

    tokio::spawn(async move {
      if let Ok(upgraded) = hyper::upgrade::on(asked).await {
        let _ = tokio::io::copy_bidirectional(&mut TokioIo::new(upgraded), &mut there).await;
      }
    });

    return Ok(Response::new(Empty::new().map_err(|never| match never {}).boxed()));
  }

  let uri = asked.uri().clone();
  let (Some("http"), Some(authority)) = (uri.scheme_str(), uri.authority()) else {
    return Ok(said(
      StatusCode::BAD_REQUEST,
      "a request names its address whole, http://<host>/<path>, or tunnels by CONNECT".to_owned(),
    ));
  };
  let found = match reach.reach(authority.host(), authority.port_u16().unwrap_or(80)).await {
    Ok(found) => found,
    Err(why) => return Ok(said(StatusCode::FORBIDDEN, why)),
  };
  let there = match dial(&found).await {
    Ok(there) => there,
    Err(why) => return Ok(said(StatusCode::BAD_GATEWAY, why)),
  };
  let (mut sender, connection) = match hyper::client::conn::http1::handshake(TokioIo::new(there)).await {
    Ok(handshake) => handshake,
    Err(why) => return Ok(said(StatusCode::BAD_GATEWAY, why.to_string())),
  };

  tokio::spawn(connection);

  let (mut parts, body) = asked.into_parts();

  parts.uri = uri
    .path_and_query()
    .map_or_else(|| "/".parse().unwrap_or_default(), |path| path.as_str().parse().unwrap_or_default());

  for own in OWN_HEADERS {
    parts.headers.remove(HeaderName::from_static(own));
  }

  if !parts.headers.contains_key(hyper::header::HOST)
    && let Ok(host) = HeaderValue::from_str(authority.as_str())
  {
    parts.headers.insert(hyper::header::HOST, host);
  }

  match sender.send_request(Request::from_parts(parts, body)).await {
    Ok(answer) => Ok(answer.map(BodyExt::boxed)),
    Err(why) => Ok(said(StatusCode::BAD_GATEWAY, why.to_string())),
  }
}

/// The proxy served on `listener` until it is dropped: each connection an HTTP/1.1 session of its own, its requests
/// held to `reach`.
pub async fn serve(listener: UnixListener, reach: Reach) {
  let reach = Arc::new(reach);

  while let Ok((stream, _)) = listener.accept().await {
    let reach = reach.clone();

    tokio::spawn(async move {
      let service = hyper::service::service_fn(move |asked| proxied(asked, reach.clone()));
      let _ = hyper::server::conn::http1::Builder::new()
        .serve_connection(TokioIo::new(stream), service)
        .with_upgrades()
        .await;
    });
  }
}

/// The folder one call's proxy socket stands in, readable by this machine's user alone, and taken away with the
/// call.
pub struct CallFolder(pub PathBuf);

impl CallFolder {
  /// A new folder of its own under the machine's temporary folder, the folders of calls more than a day old taken
  /// away first, which a runner ended before its call's end left.
  pub fn make() -> Result<Self, String> {
    use std::os::unix::fs::DirBuilderExt;

    let base = std::env::temp_dir();
    let day_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(24 * 60 * 60);

    for left in std::fs::read_dir(&base).into_iter().flatten().flatten() {
      let stale = left.file_name().to_string_lossy().starts_with(CALL_FOLDER)
        && left.metadata().and_then(|standing| standing.modified()).is_ok_and(|at| at < day_ago);

      if stale {
        let _ = std::fs::remove_dir_all(left.path());
      }
    }

    for tried in 0..16_u32 {
      let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
      let folder = base.join(format!("{CALL_FOLDER}{}-{at}-{tried}", std::process::id()));

      if std::fs::DirBuilder::new().mode(0o700).create(&folder).is_ok() {
        return Ok(Self(folder));
      }
    }

    Err(format!("no folder for the call's proxy could be made in {}", base.display()))
  }

  /// The proxy's socket in it.
  pub fn socket(&self) -> PathBuf {
    self.0.join("proxy.sock")
  }
}

impl Drop for CallFolder {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.0);
  }
}

/// The forwarder, inside the confinement: the confinement's loopback at `listen` joined to the proxy's socket `to`,
/// each connection a connection of its own there, while `run` runs; it ends as `run` ends, with its code, or by the
/// signal that ended it.
pub async fn forward(listen: &str, to: &Path, run: &[String]) -> ExitCode {
  let listener = match TcpListener::bind(listen).await {
    Ok(listener) => listener,
    Err(why) => {
      eprintln!("node-link forward: {listen} could not be listened on: {why}");
      return ExitCode::from(1);
    }
  };
  let to = to.to_owned();

  tokio::spawn(async move {
    while let Ok((mut stream, _)) = listener.accept().await {
      let to = to.clone();

      tokio::spawn(async move {
        if let Ok(mut proxy) = UnixStream::connect(&to).await {
          let _ = tokio::io::copy_bidirectional(&mut stream, &mut proxy).await;
        }
      });
    }
  });

  let Some((program, words)) = run.split_first() else {
    eprintln!("node-link forward: no command is named");
    return ExitCode::from(1);
  };
  let ended = match tokio::process::Command::new(program).args(words).status().await {
    Ok(ended) => ended,
    Err(why) => {
      eprintln!("node-link forward: {program} could not be started: {why}");
      return ExitCode::from(127);
    }
  };

  if let Some(code) = ended.code() {
    return ExitCode::from(u8::try_from(code).unwrap_or(1));
  }

  if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&ended) {
    // SAFETY: this process's own handling of the signal put back and the signal raised on itself, so it ends as the
    // command it ran ended.
    unsafe {
      libc::signal(signal, libc::SIG_DFL);
      libc::raise(signal);
    }
  }

  ExitCode::from(1)
}
