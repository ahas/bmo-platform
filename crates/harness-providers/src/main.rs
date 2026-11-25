//! The providers' answers about this machine as the TypeScript verbs and sidecars ask them
//! (docs/provider-presence.md): one line of JSON on the standing output, and a failure's reason on the
//! standing error with a non-zero status.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use harness_providers::{Machine, presence, provider};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
  name = "harness-providers",
  about = "Says whether each harness provider can answer on this machine, and which models it offers"
)]
struct Cli {
  /// The data root the providers' files stand under.
  #[arg(long, global = true, default_value = ".")]
  data_root: PathBuf,
  #[command(subcommand)]
  ask: Ask,
}

#[derive(Subcommand)]
enum Ask {
  /// Every provider with the sentence it says where it cannot answer here:
  /// `{"providers": [{"name", "missing"}]}`.
  Presence,
  /// One provider's catalogue as this machine holds it.
  Models {
    provider: String,
    /// The login's configuration directory a catalogue kept under one is read from.
    #[arg(long)]
    config_dir: Option<PathBuf>,
  },
}

async fn answer(cli: Cli) -> Result<Value, String> {
  let machine = Machine::of_process(&cli.data_root);

  match cli.ask {
    Ask::Presence => Ok(json!({
      "providers": presence(&machine)
        .into_iter()
        .map(|(name, missing)| {
          json!({
            "name": name,
            "missing": missing.as_ref().map(|reason| &reason.text),
            "missingCode": missing.as_ref().map(|reason| &reason.code),
          })
        })
        .collect::<Vec<_>>(),
    })),
    Ask::Models {
      provider: name,
      config_dir,
    } => {
      let asked = provider(&name).ok_or_else(|| format!("no provider named {name} in this build"))?;

      asked.catalogue(&machine, config_dir.as_deref()).await
    }
  }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
  match answer(Cli::parse()).await {
    Ok(said) => {
      println!("{said}");
      ExitCode::SUCCESS
    }
    Err(error) => {
      eprintln!("{error}");
      ExitCode::FAILURE
    }
  }
}
