//! A turn and a host run carried from a request file and what each came to printed as JSON, what a check
//! carries one through: `resident-turn`, one turn put into an agent's standing program through the door its
//! provider declares (docs/agent-control.md, 「What a resident turn reads」), and `host-run`, one run of a
//! provider's program in its agent's host (docs/agent-control.md, 「The host」). A twin check puts the same
//! turn or run through the TypeScript side and compares what each came to.

use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{Answer, VerbError};
use crate::door::asked::{Asked as TranscriptAsked, SurfaceAsked};
use crate::host_run::{self, Command};
use crate::places::Ground;

fn read(request: &Path) -> Result<Value, VerbError> {
  std::fs::read(request)
    .map_err(|why| why.to_string())
    .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|why| why.to_string()))
    .map_err(|why| VerbError::Refused(format!("{}: {why}", request.display())))
}

/// One turn through the door the request's provider declares: Claude Code's transcript where its turns
/// are read from the transcript, the surface's own where it carries them; a provider named by nobody is
/// Claude Code.
pub async fn resident_turn(data_root: &Path, request: &Path) -> Result<Answer, VerbError> {
  let asked = read(request)?;
  let provider = asked.get("provider").and_then(Value::as_str).unwrap_or("claude-code").to_owned();
  let carried = harness_providers::provider(&provider)
    .and_then(|declared| declared.declared().resident.as_ref())
    .is_some_and(|resident| resident.turns != policy::providers::Turns::Transcript);
  let ground = super::ground(data_root).await?;
  let came = if carried {
    surface_turn(&ground, &provider, asked).await
  } else {
    transcript_turn(&ground, asked).await
  }
  .map_err(VerbError::Refused)?;

  Ok(vec![came.to_string()].into())
}

// One turn through Claude Code's transcript door: what it came to and what became of the words said into it.
async fn transcript_turn(ground: &Ground, asked: Value) -> Result<Value, String> {
  let asked: TranscriptAsked = serde_json::from_value(asked).map_err(|why| why.to_string())?;
  let (turn, mut scripted) = asked.turn(ground).await?;
  let ending = crate::door::turn::put(ground, &turn, &mut scripted).await?;

  Ok(json!({ "ending": ending, "said": scripted.said }))
}

// One turn of a surface that carries its own: what it came to, or what it was thrown out by.
async fn surface_turn(ground: &Ground, provider: &str, asked: Value) -> Result<Value, String> {
  let asked: SurfaceAsked = serde_json::from_value(asked).map_err(|why| why.to_string())?;
  let (turn, mut scripted) = asked.turn(ground).await?;

  Ok(match crate::door::surface::turn(ground, provider, &turn, &mut scripted).await {
    Ok(ending) => json!({ "ending": ending, "said": scripted.said }),
    Err(why) => json!({ "thrown": why, "said": scripted.said }),
  })
}

/// One run as the request names it: whose host takes it, the line and the provider, the program, who its
/// row says it is, and the file whose appearing is its caller stopping it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostAsked {
  agent: String,
  line: String,
  provider: String,
  command: Command,
  #[serde(default)]
  root: Option<String>,
  #[serde(default)]
  parent: Option<String>,
  #[serde(default)]
  role: Option<String>,
  #[serde(default)]
  model: Option<String>,
  #[serde(default)]
  effort: Option<String>,
  #[serde(default)]
  session_id: Option<String>,
  #[serde(default)]
  stop_file: Option<std::path::PathBuf>,
}

// The caller of a host run that stops it by putting a file down.
struct StopOnFile(Option<std::path::PathBuf>);

impl host_run::Heard for StopOnFile {
  async fn heard(&mut self, _lines: &[String]) {}

  fn stopping(&mut self) -> bool {
    self.0.as_ref().is_some_and(|file| file.exists())
  }
}

/// One run carried in its agent's host: how its program ended, what it wrote and its row as it was left, or
/// what it was thrown out by.
pub async fn host_run(data_root: &Path, request: &Path) -> Result<Answer, VerbError> {
  let asked: HostAsked = serde_json::from_value(read(request)?)
    .map_err(|why| VerbError::Refused(format!("{}: {why}", request.display())))?;
  let ground = super::ground(data_root).await?;
  let mut caller = StopOnFile(asked.stop_file);
  let ran = host_run::run_heard(
    &ground,
    host_run::Asked {
      agent: &asked.agent,
      line: &asked.line,
      provider: &asked.provider,
      command: asked.command,
      root: asked.root,
      parent: asked.parent,
      role: asked.role,
      model: asked.model,
      effort: asked.effort,
      session: asked.session_id,
    },
    &mut caller,
  )
  .await;
  let came = match ran {
    Ok(ran) => json!({
      "exitCode": ran.exit_code,
      "cause": ran.cause.as_str(),
      "stdout": ran.stdout,
      "stderr": ran.stderr,
      "row": ran.row.body,
    }),
    Err(failure) => json!({ "thrown": failure.said() }),
  };

  Ok(vec![came.to_string()].into())
}
