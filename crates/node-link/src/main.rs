//! `node-link connect <spec>` holds one node's link from this machine; `node-link serve` is its far end,
//! which the link's SSH session starts on the node (docs/work-nodes.md, 「The link」).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use node_link::{connect, serve};

#[derive(Parser)]
#[command(name = "node-link", about = "The link to a work node")]
struct Cli {
  #[command(subcommand)]
  verb: Verb,
}

#[derive(Subcommand)]
enum Verb {
  /// Hold the link the spec describes, making it again whenever it drops.
  Connect { spec: PathBuf },
  /// On the node: a script of this machine's CLI tree asked of the home bridge the run's `BMO_HOME_BRIDGE`
  /// names, its output, errors and exit code the run's own.
  Home {
    script: String,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
  },
  /// One call of a personal tool, asked on standard input and answered on standard output: the owner's tool run
  /// confined by bubblewrap to its version's folder and its own data folder (docs/person-nodes.md, 「Personal tools
  /// on a PC」).
  Runner {
    /// The folder the owners' tools stand in.
    #[arg(long)]
    root: PathBuf,
    /// The one account whose tools this runner runs; any where none is named.
    #[arg(long)]
    owner: Option<String>,
    /// The screen kit an app's build sees, read alone (docs/person-nodes.md, 「Personal screens」).
    #[arg(long)]
    kit: Option<PathBuf>,
  },
  /// Inside a personal tool's confinement: its loopback at `listen` joined to the call's proxy socket `to` while
  /// the command after `--` runs, ending as it ends (docs/person-nodes.md, 「The network and secrets of a call」).
  Forward {
    #[arg(long)]
    listen: String,
    #[arg(long)]
    to: PathBuf,
    #[arg(last = true, required = true)]
    run: Vec<String>,
  },
  /// Inside an MCP server's confinement, in its version's folder: the call on standard input made of the server,
  /// or with `--list` its tools listed (docs/person-nodes.md, 「MCP servers」).
  Mcp {
    #[arg(long)]
    list: bool,
  },
  /// The far end, on the node: watch, send, write and signal over standard input and output.
  Serve {
    /// The workplace's root on the node.
    #[arg(long)]
    root: PathBuf,
    /// The socket this session's home bridge was forwarded to.
    #[arg(long)]
    bridge: Option<PathBuf>,
    /// The node's herdr session, by its name and its socket, started where it does not answer.
    #[arg(long)]
    herdr_session: Option<String>,
    #[arg(long)]
    herdr_socket: Option<PathBuf>,
    /// On a person's PC: the loopback address the mirror is spoken on, one session after another, and the road
    /// home dialed over it; the ports the near end names are listened on at the same address.
    #[arg(long)]
    listen: Option<String>,
    /// On a person's PC: the loopback address the herdr socket is reached at.
    #[arg(long)]
    herdr_listen: Option<String>,
    /// On a node of this machine running as a user of its own: the socket the herdr socket is reached at.
    #[arg(long)]
    herdr_unix: Option<PathBuf>,
  },
}

#[tokio::main]
async fn main() -> ExitCode {
  match Cli::parse().verb {
    Verb::Connect { spec } => match connect::connect(spec).await {
      Ok(()) => ExitCode::SUCCESS,
      Err(error) => {
        eprintln!("node-link: {error}");
        ExitCode::FAILURE
      }
    },
    Verb::Home { script, args } => {
      let socket = std::env::var_os("BMO_HOME_BRIDGE").filter(|named| !named.is_empty());
      let Some(socket) = socket else {
        eprintln!("node-link home: no home bridge is named: BMO_HOME_BRIDGE is not set");
        return ExitCode::from(2);
      };
      let code = node_link::bridge::ask(
        std::path::Path::new(&socket),
        &script,
        &args,
        tokio::io::stdin(),
        tokio::io::stdout(),
        tokio::io::stderr(),
      )
      .await;

      ExitCode::from(u8::try_from(code).unwrap_or(1))
    }
    Verb::Runner { root, owner, kit } => {
      let bwrap = std::env::var(node_link::runner::BWRAP_VARIABLE).unwrap_or_else(|_| "bwrap".to_owned());
      let stand_ins = std::env::var(node_link::proxy::RESOLVE_VARIABLE)
        .map(|named| node_link::proxy::stand_ins(&named))
        .unwrap_or_default();
      let runner = node_link::runner::Runner {
        root,
        owner,
        bwrap,
        kit,
        stand_ins,
      };

      node_link::runner::run(runner, tokio::io::stdin(), tokio::io::stdout()).await;
      ExitCode::SUCCESS
    }
    Verb::Forward { listen, to, run } => node_link::proxy::forward(&listen, &to, &run).await,
    Verb::Mcp { list } => node_link::mcp::adapt(list).await,
    Verb::Serve {
      root,
      bridge,
      herdr_session,
      herdr_socket,
      listen,
      herdr_listen,
      herdr_unix,
    } => match serve::serve(serve::Serve {
      root,
      bridge,
      herdr_session,
      herdr_socket,
      listen,
      herdr_listen,
      herdr_unix,
    })
    .await
    {
      Ok(()) => ExitCode::SUCCESS,
      Err(error) => {
        eprintln!("node-link serve: {error}");
        ExitCode::FAILURE
      }
    },
  }
}
