//! The job guard: what agency's shell hook refuses of a shell command on a line whose programs are jobs'
//! (docs/live-claude-guard.md). It refuses, in this order, what no opening lets through — Claude Code in
//! print mode, a removal the CLI would stop on, a shell habit that stalls a job, a change to the statement
//! document the job's item does not open, a write into another worker's checkout, a write of a file an
//! umbrella's folder tracks — then a heavy command
//! started outside the testbed, then a command that starts Claude Code, writes into its session records, forks
//! a session or puts words into a pane, unless the job has opened the live-Claude guard for itself, in which
//! case the command is recorded against the opening and let through. Of a file tool's write it refuses the
//! statement document, another worker's checkout and an umbrella's tracked files alone.
//!
//! What it needs beyond the command's text it asks of its ground: the testbed's reading of the line, the
//! configuration directories of the accounts, the job's opening, the other workers' checkouts and whether the
//! job's item opens the statement document. A ground that cannot answer lets a command through, but for the
//! statement document, which it leaves refused.

use super::ShellCall;
use super::habits::habit_refusal;
use super::records::{CliRecords, command_refusal, print_mode_refusal};
use super::removal::removal_refusal;
use super::testbed::{Classified, testbed_refusal};
use super::writes::{
  Checkouts, Standing, foreign_command_write, foreign_file_write, foreign_refusal, may_write,
  sealed_command_write, sealed_file_write, sealed_refusal, structure_command_write, structure_file_write,
  structure_refusal,
};

/// The file tools that write, whose calls the guard reads for the file they name.
pub const FILE_WRITERS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// The word every account's record directory ends in: a command that writes into one names it in its text.
const RECORDS_WORD: &str = "projects";

/// What the job guard asks beside the command's text.
pub trait JobGround {
  /// The testbed's reading of the line, or nothing where it cannot be asked.
  fn classify(&self, line: &str) -> impl Future<Output = Option<Classified>>;
  /// Every configuration directory a login keeps its session records under: the accounts' and the
  /// machine's own.
  fn config_dirs(&self) -> impl Future<Output = Vec<String>>;
  /// The latest live-Claude opening the job running this program made, or nothing.
  fn opening(&self) -> impl Future<Output = Option<i64>>;
  /// A command let through because of the opening, recorded against it.
  fn record(&self, opening: i64, what: &str) -> impl Future<Output = ()>;
  /// The job's own checkout and those it borrowed, every other worker's on the job's machine, and the files its
  /// own folder tracks where that is an umbrella's; none where the job or the store cannot be read.
  fn checkouts(&self) -> impl Future<Output = Checkouts>;
  /// Whether the job's item opens the statement document: the owner asked for it and its words name the
  /// document; false where it cannot be read.
  fn opens_structure(&self) -> impl Future<Output = bool>;
}

// Whether a command's text could name a path of another checkout by itself: every spelling of one holds a slash
// or a tilde. A command holding neither still may by a relative word, read from where the shell stands, where it
// writes at all.
fn names_a_path(command: &str) -> bool {
  command.contains('/') || command.contains('~')
}

// Why the job may not write where the command or the file tool would, or nothing: the statement document its
// item does not open, then another worker's checkout or an umbrella's own folder. The ground is asked only where
// the text names either.
async fn place_refusal(
  structure: Option<String>,
  foreign: impl FnOnce(&Checkouts) -> Option<String>,
  may_name_checkout: bool,
  ground: &impl JobGround,
) -> Option<String> {
  if let Some(why) = structure
    && !ground.opens_structure().await
  {
    return Some(structure_refusal(&why));
  }

  if !may_name_checkout {
    return None;
  }

  let checkouts = ground.checkouts().await;

  foreign(&checkouts)
}

/// Why a job may not make this file tool's call on `path`, or nothing.
pub async fn file_refusal(path: &str, ground: &impl JobGround) -> Option<String> {
  place_refusal(
    structure_file_write(path),
    |checkouts| {
      foreign_file_write(path, checkouts)
        .map(|checkout| foreign_refusal(checkout, "it edits a file there"))
        .or_else(|| sealed_file_write(path, checkouts).map(|(tree, why)| sealed_refusal(tree, &why)))
    },
    true,
    ground,
  )
  .await
}

/// What a refusal of a command the job has not opened the live-Claude guard for says.
pub fn live_claude_refusal(refused: &str) -> String {
  format!(
    "refused before this job has said what for: {refused}, which writes a whole prefix to the cache again. To open it for this job: bmo live-claude --purpose '<what you measure and why>'. Nobody is asked; every run it lets through is recorded and the count goes into the item's report (docs/live-claude-guard.md, 「A job opens it by saying what for」)."
  )
}

/// Why a job's shell may not run this command as written, or nothing; `home` is the home the command's
/// paths are spelled under, and `cwd` the directory the shell stands in, where the hook was handed one.
pub async fn job_refusal(
  command: &str,
  call: ShellCall,
  home: &str,
  cwd: Option<&str>,
  ground: &impl JobGround,
) -> Option<String> {
  if let Some(refused) = print_mode_refusal(command)
    .or_else(|| removal_refusal(command))
    .or_else(|| habit_refusal(command, call))
  {
    return Some(refused);
  }

  let standing = Standing {
    at: cwd.map(str::to_owned),
    home: home.to_owned(),
  };

  if let Some(refused) = place_refusal(
    structure_command_write(command),
    |checkouts| {
      foreign_command_write(command, checkouts, &standing)
        .map(|(checkout, why)| foreign_refusal(checkout, &why))
        .or_else(|| {
          sealed_command_write(command, checkouts, &standing).map(|(tree, why)| sealed_refusal(tree, &why))
        })
    },
    names_a_path(command) || (cwd.is_some() && may_write(command)),
    ground,
  )
  .await
  {
    return Some(refused);
  }

  if let Some(refused) = ground.classify(command).await.as_ref().and_then(testbed_refusal) {
    return Some(refused);
  }

  // Which accounts there are is the store's, asked only where the text could name their records at all.
  let dirs = match command.contains(RECORDS_WORD) {
    true => ground.config_dirs().await,
    false => vec![],
  };
  let refused = command_refusal(command, &CliRecords::of(&dirs, home))?;
  let Some(opening) = ground.opening().await else {
    return Some(live_claude_refusal(&refused));
  };

  ground
    .record(opening, &format!("shell: {}", command.chars().take(200).collect::<String>()))
    .await;

  None
}
