//! `agent-host serve` stands for a data root; `agent-host supervise <envelope>` is one execution's
//! supervisor, which the host starts; `agent-host guard <group>` is the guardian a supervisor starts
//! beside its child (docs/agent-control.md, 「The host」); `agent-host trust <.claude.json> <folder>` is the
//! folder trust a work node's setup writes (docs/agent-control.md, 「A first interactive run stops at the
//! trust-this-folder check」).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use agent_host::host::{Ended, HostOptions, serve};
use agent_host::supervise::{guard, supervise};
use agent_host::trust::{mark_onboarded, trust_folder};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "agent-host", about = "The agent host")]
struct Cli {
  #[command(subcommand)]
  verb: Verb,
}

#[derive(Subcommand)]
enum Verb {
  /// Stand for a data root: claim it, take what launchers put in, and supervise it.
  Serve {
    /// The data root to serve.
    #[arg(long, env = "BMO_DATA_DIR")]
    data_root: Option<PathBuf>,
    /// The tree the host's code was built from, whose revision the claim names.
    #[arg(long)]
    code_root: Option<PathBuf>,
    /// How often the inbox and the executions are looked at, in milliseconds.
    #[arg(long, default_value_t = 20)]
    poll_ms: u64,
  },
  /// Supervise one execution's envelope.
  Supervise { envelope: PathBuf },
  /// Kill a process group once standard input closes.
  Guard { group: i32 },
  /// Trust a folder in a login's configuration file, ending non-zero where the trust does not hold; with
  /// `--onboarded`, the login's first-run screens marked as passed in the same file first.
  Trust {
    file: PathBuf,
    folder: String,
    #[arg(long)]
    onboarded: bool,
  },
}

// The deployment's own data root, under the home.
fn own_data_root() -> Option<PathBuf> {
  std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share/bmo/data"))
}

// The tree a deployed binary stands in: `<tree>/bin/agent-host`.
fn own_code_root() -> PathBuf {
  std::env::current_exe()
    .ok()
    .and_then(|exe| Some(exe.parent()?.parent()?.to_owned()))
    .unwrap_or_else(|| PathBuf::from("."))
}

fn main() -> ExitCode {
  let outcome = match Cli::parse().verb {
    Verb::Serve {
      data_root,
      code_root,
      poll_ms,
    } => {
      let Some(data_root) = data_root.filter(|root| !root.as_os_str().is_empty()).or_else(own_data_root)
      else {
        eprintln!("agent-host: no data root: set BMO_DATA_DIR or HOME");
        return ExitCode::FAILURE;
      };
      let options = HostOptions {
        code_root: code_root.unwrap_or_else(own_code_root),
        poll: Duration::from_millis(poll_ms.max(1)),
        data_root,
      };

      serve(&options).map(|ended| match ended {
        Ended::Duplicate => {
          eprintln!("agent-host: {} is served by another host", options.data_root.display())
        }
        Ended::RootGone => eprintln!("agent-host: {} is gone", options.data_root.display()),
        Ended::Stopped => {}
      })
    }
    Verb::Supervise { envelope } => supervise(&envelope),
    Verb::Guard { group } => guard(group),
    Verb::Trust {
      file,
      folder,
      onboarded,
    } => {
      if onboarded {
        let marked = mark_onboarded(&file);

        println!("first-run screens: {} in {}", marked.word(), file.display());

        if !marked.holds() {
          return ExitCode::FAILURE;
        }
      }

      let trust = trust_folder(&file, &folder);

      println!("{folder}: {} in {}", trust.word(), file.display());

      return if trust.holds() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }
  };

  match outcome {
    Ok(()) => ExitCode::SUCCESS,
    Err(why) => {
      eprintln!("agent-host: {why}");
      ExitCode::FAILURE
    }
  }
}
