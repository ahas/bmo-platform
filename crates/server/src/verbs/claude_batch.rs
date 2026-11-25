//! `bmo-agency claude batch` (docs/agents.md, 「Runs apart」), the gate project's claude road: one batch on
//! Claude Code bare of this machine's setup, as a run apart — the interactive program in a pane of its own
//! on a fresh session, pinned to the login directory named, its answer and what it spent read off its
//! record.
//!
//! The batch comes in on standard input and its fixed part from a file, which the program takes as its
//! whole system prompt. An answer is printed as one JSON object, the text and the spend model by model; any
//! other ending is printed as its failure's words on the error stream, and the verb ends failed.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Args;
use harness_providers::claude_code::ClaudeCode;
use policy::roles::ROLES;
use serde_json::{Value, json};

use super::{Stop, VerbError, ground};
use crate::door::apart::{self, Apart, answered, new_session};
use crate::door::record::Reply;
use crate::door::turn::{Ending, Watch};
use crate::lines::claude::Command;
use crate::places::Ground;

// The name a batch's run goes by: its agent, its line and its work key.
const NAME: &str = "gate-batch";

const PROVIDER: &str = "claude-code";

/// Every variable that could carry a login other than the one the named directory holds. Any of them
/// reaching the program would decide the account before the directory is ever read.
pub const OTHER_LOGINS: [&str; 3] =
  ["CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

// The folder under the data root a batch works in, which holds nothing: one folder for every batch, so the
// login's directory keeps one project of them.
const WORKDIR: &str = "claude-batch";

#[derive(Args)]
pub struct ClaudeBatch {
  #[arg(long)]
  model: String,
  #[arg(long)]
  effort: String,
  /// The login directory the batch is pinned to, named to the program as its configuration directory.
  #[arg(long)]
  config_dir: PathBuf,
  /// The file holding the batch's fixed part, which the program takes as its whole system prompt.
  #[arg(long)]
  system_file: PathBuf,
  /// What a person watching herdr reads the run as.
  #[arg(long)]
  title: String,
}

/// The program's words for one batch: safe mode, so none of the machine's instructions, hooks, plugins,
/// skills or MCP servers ride along; no setting source, so no memory file is read and the program never
/// stops at the external-imports question the home's own one raises (docs/agent-control.md, 「A run under
/// a login directory stops at the external-imports question」); no MCP configuration and no tool at all;
/// the fixed part as the system prompt; no fallback model, so an answer is the model's named or none; and
/// the session the run opens, which nothing resumes.
pub fn batch_args(model: &str, effort: &str, system_file: &Path, session: &str) -> Vec<String> {
  let mut args = vec![ClaudeCode::PROGRAM.to_owned()];

  args.extend(
    [
      "--model",
      model,
      "--effort",
      effort,
      "--safe-mode",
      "--setting-sources",
      "",
      "--strict-mcp-config",
      "--tools",
      "",
      "--system-prompt-file",
    ]
    .map(str::to_owned),
  );
  args.push(system_file.to_string_lossy().into_owned());
  args.extend(["--session-id".to_owned(), session.to_owned()]);
  args
}

/// The program's environment for one batch: the launching process's, every other login's variable unset,
/// and the login directory named outright.
pub fn batch_env(
  process: &BTreeMap<String, String>,
  config_dir: &Path,
) -> BTreeMap<String, Option<String>> {
  let mut env: BTreeMap<String, Option<String>> =
    process.iter().map(|(name, value)| (name.clone(), Some(value.clone()))).collect();

  for variable in OTHER_LOGINS {
    env.insert(variable.to_owned(), None);
  }

  env.insert("CLAUDE_CONFIG_DIR".to_owned(), Some(config_dir.to_string_lossy().into_owned()));
  env
}

/// An answer as the verb prints it: the text, the model that wrote it, and each model's tokens as they were
/// spent, the program's own small calls — the one titling its session — among them.
pub fn printed(reply: &Reply) -> String {
  let spend: Vec<Value> = reply
    .spend
    .iter()
    .map(|one| {
      json!({
        "model": one.model,
        "inputTokens": one.input_tokens,
        "outputTokens": one.output_tokens,
        "cacheReadTokens": one.cache_read_tokens,
        "cacheWriteTokens": one.cache_write_tokens,
      })
    })
    .collect();

  json!({ "text": reply.text, "model": reply.model, "spend": spend }).to_string()
}

/// A run's ending as the verb hands it back: the reply where it answered, and otherwise its failure's
/// words, with the program's own error beside them where it gave one the words do not already carry.
pub fn reply_of(ending: Ending) -> Result<Reply, String> {
  let said = match &ending {
    Ending::Errored { error, .. } => Some(error.trim().to_owned()),
    _ => None,
  };

  answered(ending, NAME).map_err(|failure| {
    let message = failure.message();

    match said.filter(|said| !said.is_empty() && !message.contains(said.as_str())) {
      Some(said) => format!("{message} — {said}"),
      None => message,
    }
  })
}

// The batch's run as it goes: nothing read of it but its answer, and stopped once the verb is told to stop,
// so a caller that gives up on the verb still has its pane closed.
struct Stoppable(Arc<AtomicBool>);

impl Watch for Stoppable {
  fn stopping(&mut self) -> bool {
    self.0.load(Ordering::SeqCst)
  }
}

// A path the program is handed, absolute and there, since the program starts in a folder of its own.
fn present(path: &Path, what: &str) -> Result<PathBuf, VerbError> {
  std::fs::canonicalize(path).map_err(|why| {
    VerbError::Refused(format!("claude-batch: the {what} {} is not there: {why}", path.display()))
  })
}

/// One batch, from standard input, run apart and printed.
pub async fn run(asked: ClaudeBatch, data_root: &Path) -> Result<Vec<String>, VerbError> {
  let stop = Arc::new(AtomicBool::new(false));
  let mut heard = Stop::listen()?;
  let told_stop = Arc::clone(&stop);

  tokio::spawn(async move {
    let signal = heard.next().await;

    eprintln!("claude-batch: {signal}, stopping");
    told_stop.store(true, Ordering::SeqCst);
  });

  let config_dir = present(&asked.config_dir, "login directory")?;
  let system_file = present(&asked.system_file, "system prompt file")?;
  let mut batch = String::new();

  std::io::stdin().read_to_string(&mut batch).map_err(|why| {
    VerbError::Refused(format!("claude-batch: the batch could not be read from standard input: {why}"))
  })?;

  if batch.trim().is_empty() {
    return Err(VerbError::Refused("claude-batch: no batch came in on standard input".to_owned()));
  }

  let workdir = data_root.join(WORKDIR);

  std::fs::create_dir_all(&workdir).map_err(|why| {
    VerbError::Refused(format!(
      "claude-batch: the working folder {} could not be made: {why}",
      workdir.display()
    ))
  })?;

  let ground: Ground = ground(data_root).await?;
  let session = new_session();
  let command = Command {
    args: batch_args(&asked.model, &asked.effort, &system_file, &session),
    cwd: workdir.to_string_lossy().into_owned(),
    env: batch_env(ground.env(), &config_dir),
  };
  let ending = apart::run(
    &ground,
    Apart {
      agent: NAME,
      role: ROLES.renewal.as_str(),
      line: NAME,
      provider: PROVIDER,
      title: asked.title,
      command,
      session,
      prompt: batch,
      model: asked.model,
      effort: asked.effort,
      account: None,
      parent: None,
      root: None,
      run: None,
      resume: None,
      resume_else: None,
    },
    &mut Stoppable(stop),
  )
  .await
  .map_err(|why| VerbError::Refused(format!("claude-batch: {why}")))?;
  let reply = reply_of(ending).map_err(VerbError::Refused)?;

  Ok(vec![printed(&reply)])
}
