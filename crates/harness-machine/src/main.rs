//! The machine's part of the registry, the model catalogue and the supervisors as the TypeScript verbs
//! and sidecars ask it (docs/harness-machine.md, 「The binary」): one line of JSON on the standing output;
//! a refusal the owner reads as JSON on the standing error with status 2, any other failure's reason
//! there with status 1. A key is read from standing input alone and never printed.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use harness_machine::engine_health::{self, Address};
use harness_machine::model_use::{self, Install, Start};
use harness_machine::supervisor_prompt::{self, Base, Composition, Roots, Supervisor};
use harness_machine::{Fault, gateway, keys};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
  name = "harness-machine",
  about = "Reads, writes and starts on this machine what the registry, the model catalogue and the supervisors need"
)]
struct Cli {
  /// The data root the key files, the gateway's status and the supervisors' folders stand under.
  #[arg(long, global = true, default_value = ".")]
  data_root: PathBuf,
  /// The tree the agent documents are read out of.
  #[arg(long, global = true, default_value = ".")]
  asset_root: PathBuf,
  #[command(subcommand)]
  ask: Ask,
}

#[derive(Subcommand)]
enum Ask {
  /// The registered models' key files.
  Key {
    #[command(subcommand)]
    ask: KeyAsk,
  },
  /// The gateway's status as its launcher last wrote it, or null.
  Gateway,
  /// An engine's health, asked at the addresses read from standard input
  /// (`[{"url", "keyFile"}]`), or null.
  Health,
  /// A model's use on its engine.
  ModelUse {
    #[command(subcommand)]
    ask: UseAsk,
  },
  /// A supervisor's system prompt, composed out of what standard input holds:
  /// `{"agent", "label": <label> | null, "callNames": [<name>], "prompts": <folder> | null,
  /// "settings": {"persona"}, "base": {"text", "model"} | null, "ownHarness"}`, where `prompts` is the prompt
  /// folder the agent's row names.
  SupervisorSystem,
  /// The persona a prompt folder's file holds, or an empty text where it has none.
  SupervisorPersona { folder: String },
}

#[derive(Subcommand)]
enum KeyAsk {
  /// Writes the key read from standard input for one address and prints `{"keyFile"}`.
  Write { registration: String, node: String },
  /// Takes away a key file agency wrote.
  Remove { file: String },
  /// Prints, for each key file name read from standard input (a JSON array, null for none), whether
  /// it is usable, none or unusable.
  States,
}

#[derive(Subcommand)]
enum UseAsk {
  /// Starts a choice's unit, which runs the catalogue verb's run-use on it.
  Start {
    #[arg(long)]
    engine: String,
    #[arg(long)]
    use_id: i64,
    /// The archive the choice was made in.
    #[arg(long)]
    archive: String,
    /// The PATH the run is handed.
    #[arg(long)]
    path: String,
    #[arg(long)]
    source_root: PathBuf,
    /// The tree whose verbs' binary the catalogue verb is run out of.
    #[arg(long)]
    code_root: PathBuf,
  },
  /// Runs an engine's installer with an entry's name into a log, and prints `{"ok", "last"}`.
  Install {
    #[arg(long)]
    engine: String,
    #[arg(long)]
    entry: String,
    #[arg(long)]
    log: PathBuf,
    #[arg(long)]
    archive: String,
    #[arg(long)]
    source_root: PathBuf,
    #[arg(long, default_value = "bun")]
    bun: PathBuf,
  },
  /// Prints the last lines of a choice's log.
  Log { log: PathBuf },
}

/// What a supervisor's prompt is composed from.
#[derive(Deserialize)]
struct SupervisorAsk {
  agent: String,
  /// The label it stands as in the rooms, none for its name.
  #[serde(default)]
  label: Option<String>,
  /// The words beside its label that call it in a room.
  #[serde(default, rename = "callNames")]
  call_names: Vec<String>,
  /// The prompt folder the agent's row names under `prompts/agents/`, none where it names none.
  #[serde(default)]
  prompts: Option<String>,
  settings: Composition,
  base: Option<Base>,
  /// Whether the supervisor's program is a harness of its own, whose own prompt stands where the platform's
  /// harness opening would; by default it runs on Claude Code and takes the platform's.
  #[serde(default, rename = "ownHarness")]
  own_harness: bool,
  /// What stands between its harness and its role and whether its persona is left out, as its caller resolved
  /// them; by default no platform rules, no organisation's instructions and its persona kept.
  #[serde(default)]
  layers: supervisor_prompt::Layers,
}

fn standing_input() -> Result<String, Fault> {
  let mut text = String::new();

  std::io::stdin().read_to_string(&mut text)?;

  Ok(text)
}

fn read_input<T: serde::de::DeserializeOwned>() -> Result<T, Fault> {
  serde_json::from_str(&standing_input()?)
    .map_err(|error| Fault::Failed(format!("standing input does not read: {error}")))
}

async fn answer(cli: Cli) -> Result<Value, Fault> {
  let data_root = cli.data_root.as_path();
  let roots = Roots {
    assets: cli.asset_root.clone(),
    data: cli.data_root.clone(),
  };

  match cli.ask {
    Ask::Key { ask } => match ask {
      KeyAsk::Write { registration, node } => {
        let key_file = keys::write(data_root, &registration, &node, &standing_input()?)?;

        Ok(json!({ "keyFile": key_file }))
      }
      KeyAsk::Remove { file } => {
        keys::remove(data_root, Some(&file), true);

        Ok(Value::Null)
      }
      KeyAsk::States => {
        let files: Vec<Option<String>> = read_input()?;

        Ok(json!(
          files.iter().map(|file| keys::state(data_root, file.as_deref())).collect::<Vec<_>>()
        ))
      }
    },
    Ask::Gateway => Ok(json!(gateway::status(data_root))),
    Ask::Health => {
      let addresses: Vec<Address> = read_input()?;

      Ok(json!(engine_health::health(data_root, &addresses).await))
    }
    Ask::ModelUse { ask } => match ask {
      UseAsk::Start {
        engine,
        use_id,
        archive,
        path,
        source_root,
        code_root,
      } => {
        let start = Start {
          engine,
          use_id,
          archive,
          path,
          source_root,
          code_root,
        };

        start.run().map_err(Fault::Failed)?;

        Ok(Value::Null)
      }
      UseAsk::Install {
        engine,
        entry,
        log,
        archive,
        source_root,
        bun,
      } => {
        let install = Install {
          engine,
          entry,
          log,
          archive,
          source_root,
          bun,
        };

        Ok(json!(install.run().await?))
      }
      UseAsk::Log { log } => Ok(json!(model_use::log_tail(Some(&log)))),
    },
    Ask::SupervisorSystem => {
      let asked: SupervisorAsk = read_input()?;

      let supervisor = Supervisor {
        name: &asked.agent,
        label: asked.label.as_deref(),
        call_names: &asked.call_names,
        prompts: asked.prompts.as_deref(),
      };

      Ok(json!(supervisor_prompt::system(
        &roots,
        supervisor,
        &asked.settings,
        asked.base.as_ref(),
        !asked.own_harness,
        &asked.layers
      )?))
    }
    Ask::SupervisorPersona { folder } => {
      roots.prompt_folder(&folder)?;

      Ok(json!(roots.persona(Some(&folder), None).unwrap_or_default()))
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
    Err(Fault::Refused(refusal)) => {
      eprintln!("{}", json!(refusal));
      ExitCode::from(2)
    }
    Err(Fault::Failed(error)) => {
      eprintln!("{error}");
      ExitCode::FAILURE
    }
  }
}
