//! Agency told that documents moved (docs/sessions.md, 「Telling a session a document moved」): the agents'
//! documents written again where a rulebook or an agent document changed, every conversational agent whose
//! system prompt a changed document is part of marked as having its prompt moved, a worker whose prefix
//! moved told how to fold it, and the move told to the running server, which publishes it for the consumer
//! holding the rooms to tell the sessions there.
//!
//! A document that ships with the code — the rulebooks and the agent documents — is live only in the deploy
//! tree, so it is named relative to the tree and told only by the deploy tree's own binary, which reads the
//! deployed text; run from a checkout, those are refused rather than told as text the service does not read.
//! A document in the brain keeps its absolute path and is told from wherever this runs. Agency holds no
//! persona: the base persona is read from the agent's brain at each launch, or handed in with the turn
//! where the brain gives none, so no persona file is a document of its own.

use std::path::{Component, Path, PathBuf};

use sea_orm::DatabaseConnection;
use serde_json::json;
use store::run;

use super::{VerbError, tell_server};
use crate::lines::prefix::touches_prefix;

/// What `bmo-agency reload` takes, as its refusal of a call naming no document says.
pub const RELOAD_USAGE: &str = "usage: bmo-agency reload --path <file> [--path <file>...] [--now]";

// The tree's document directories a session relies on, relative to its root.
const DOCUMENT_DIRS: [&str; 3] = ["prompts/rules", "prompts/platform", "prompts/agents"];

// The platform's harness opening, a document of the tree standing outside those directories, relative to the
// prompts.
const HARNESS: &str = "harness.md";

// The rulebooks and the supervisor's own documents a chat session is given as its system prompt, the platform's
// rulebooks a supervisor of the platform administrator's carries among them.
const CHAT_DOCUMENTS: [&str; 8] = [
  "rules/answer-content.md",
  "rules/answer-shape.md",
  "rules/plain-writing.md",
  "rules/agent-words.md",
  "rules/self-preservation.md",
  "platform/environment.md",
  "platform/preservation.md",
  "platform/operations.md",
];
const CHAT_SYSTEM_DOCUMENTS: [&str; 2] = ["agents/chat/context.md", "agents/chat/manual.md"];

/// A path made whole without the file system: `.` dropped and `..` taking its parent away, as Node
/// resolves.
pub fn lexical(path: &Path) -> PathBuf {
  let mut whole = PathBuf::new();

  for part in path.components() {
    match part {
      Component::CurDir => {}
      Component::ParentDir => {
        whole.pop();
      }
      other => whole.push(other.as_os_str()),
    }
  }

  whole
}

/// Where the verb stands: the tree it reads shipped documents out of, the brain, and the checkouts a whole
/// path into one names a tree document by.
pub struct Places {
  pub tree: PathBuf,
  pub brain: PathBuf,
  pub deploy: PathBuf,
  pub checkouts: Vec<PathBuf>,
  pub here: PathBuf,
}

impl Places {
  /// The places of a process standing on a ground: its code tree, read as the tree, the checkout and here
  /// alike.
  pub fn of_ground(ground: &crate::places::Ground) -> Self {
    let tree = ground.code_root();

    Self {
      brain: ground.roots.data.join("brain"),
      deploy: ground.roots.deploy.clone(),
      checkouts: vec![tree.clone()],
      here: tree.clone(),
      tree,
    }
  }

  // The path relative to the brain root, for a whole path that lies inside the brain and is not the root.
  fn brain_relative(&self, path: &str) -> Option<String> {
    let path = Path::new(path);

    if !path.is_absolute() {
      return None;
    }

    let inside = lexical(path).strip_prefix(lexical(&self.brain)).ok()?.to_path_buf();

    (!inside.as_os_str().is_empty()).then(|| inside.to_string_lossy().into_owned())
  }

  /// The path a document is named by: the brain's whole, a tree's relative to its root. A whole path into a
  /// checkout names the same document the tree holds under that name.
  pub fn named(&self, value: &str) -> String {
    let whole = lexical(&self.here.join(value));
    let whole_text = whole.to_string_lossy().into_owned();

    if self.brain_relative(&whole_text).is_some() {
      return whole_text;
    }

    if !Path::new(value).is_absolute() {
      return lexical(Path::new(value)).to_string_lossy().into_owned();
    }

    for root in std::iter::once(&self.tree).chain(&self.checkouts) {
      if let Ok(inside) = whole.strip_prefix(lexical(root)) {
        return inside.to_string_lossy().into_owned();
      }
    }

    pathdiff(&whole, &lexical(&self.tree))
  }
}

// One path relative to another, both whole, as Node's `relative` words it.
fn pathdiff(path: &Path, base: &Path) -> String {
  let ours: Vec<_> = path.components().collect();
  let theirs: Vec<_> = base.components().collect();
  let shared = ours.iter().zip(&theirs).take_while(|(one, other)| one == other).count();
  let mut out = PathBuf::new();

  for _ in shared..theirs.len() {
    out.push("..");
  }

  for part in &ours[shared..] {
    out.push(part.as_os_str());
  }

  out.to_string_lossy().into_owned()
}

/// Of the repository-relative paths named, the documents a session relies on that ship with the code: these
/// go live with the deployment and nowhere earlier.
pub fn tree_documents(paths: &[String]) -> Vec<String> {
  paths
    .iter()
    .filter(|path| {
      (path.ends_with(".md") && DOCUMENT_DIRS.iter().any(|dir| path.starts_with(&format!("{dir}/"))))
        || path.as_str() == format!("prompts/{HARNESS}")
    })
    .cloned()
    .collect()
}

// What a tree document is classified by: the container every prompt document stands in dropped first.
fn document_parts(path: &str) -> Vec<&str> {
  let parts: Vec<&str> = path.split('/').collect();

  if parts.first() == Some(&"prompts") { parts[1..].to_vec() } else { parts }
}

fn is_agent_document(path: &str) -> bool {
  matches!(document_parts(path).first(), Some(&"rules" | &"platform" | &"agents"))
}

/// Whether a document is part of a chat session's turn rather than its system prompt or something read with
/// a tool: the session carries its new text on its next turn without being told.
pub fn is_chat_turn_document(path: &str) -> bool {
  let joined = document_parts(path).join("/");

  joined == "agents/chat/owner.md"
    || joined == "agents/chat/rules.md"
    || joined == "agents/chat/capabilities.md"
}

/// Whether a document is part of a chat session's own system prompt.
pub fn is_chat_prompt_document(path: &str) -> bool {
  if is_chat_turn_document(path) {
    return false;
  }

  let joined = document_parts(path).join("/");

  joined == HARNESS
    || CHAT_SYSTEM_DOCUMENTS.contains(&joined.as_str())
    || CHAT_DOCUMENTS.contains(&joined.as_str())
}

/// How the agents' documents are written again where a changed rulebook or agent document calls for it, so
/// the runners' directories carry the current text before any session is told.
pub trait Redeploy: Sync {
  fn redeploy(&self);
}

/// A reload as `bmo-agency reload` runs it: the documents named told, and what the reload came to said.
/// `paths` are as the call gave them; `now` is in unix seconds; `immediate` asks the consumer to tell every
/// session at once.
pub async fn reload(
  db: &DatabaseConnection,
  places: &Places,
  given: &[String],
  immediate: bool,
  redeploy: &dyn Redeploy,
  now: i64,
) -> Result<Vec<String>, VerbError> {
  let paths: Vec<String> = given.iter().map(|value| places.named(value)).collect();

  if paths.is_empty() {
    return Err(VerbError::Refused(RELOAD_USAGE.to_owned()));
  }

  let shipped = tree_documents(&paths);
  let same_tree = std::fs::canonicalize(&places.tree)
    .ok()
    .zip(std::fs::canonicalize(&places.deploy).ok())
    .is_some_and(|(tree, deploy)| tree == deploy);

  if !shipped.is_empty() && !same_tree {
    return Err(VerbError::Refused(format!(
      "reload: {} ship with the code and go live with the deployment, which tells every session concerned once the deploy tree holds them.\nreload: this is {}, not the deploy tree {}; nothing was told. Commit the change and close the job; the deployment carries it.",
      shipped.join(", "),
      places.tree.display(),
      places.deploy.display()
    )));
  }

  let mut said = vec![];

  if paths.iter().any(|path| is_agent_document(path)) {
    redeploy.redeploy();
  }

  // A document that is itself part of a conversational agent's system prompt is not announced: the next
  // launch composes the prompt from the file. The move is recorded instead, for each agent to settle when
  // it next speaks.
  let prompts: Vec<&String> = paths.iter().filter(|path| is_chat_prompt_document(path)).collect();

  if !prompts.is_empty() {
    let agents = crate::conversational::agents_where(db, |_| true).await?;

    for agent in &agents {
      run(
        db,
        r#"UPDATE "supervisors" SET "prompt_changed_at" = $1 WHERE "name" = $2"#,
        vec![now.into(), agent.as_str().into()],
      )
      .await?;

      for path in &prompts {
        run(
          db,
          r#"INSERT INTO "prefix_moves" ("agent", "at", "why", "detail") VALUES ($1, $2, 'reload', $3)"#,
          vec![agent.as_str().into(), now.into(), path.as_str().into()],
        )
        .await?;
      }
    }

    said.push(format!(
      "reload: prompt document(s) written for {} agent(s); each reads them on its own next turn",
      agents.len()
    ));
  }

  // What a session reads with its turns or with a tool is told by the consumer holding its rooms.
  let spoken: Vec<&String> = paths
    .iter()
    .filter(|path| !is_chat_turn_document(path) && !is_chat_prompt_document(path))
    .collect();

  if !spoken.is_empty() {
    tell_server("/documents/reload", json!({ "paths": spoken, "now": immediate })).await;
    said.push(format!(
      "reload: told agency {} moved",
      spoken.iter().map(|path| path.as_str()).collect::<Vec<_>>().join(", ")
    ));
  }

  // A role's session cannot be told: what moved is the prefix it was opened on, read only when a session is
  // opened, so the next run of each kept session pays the move once. A reload that named one says so.
  let moved = touches_prefix(&paths);

  if !moved.is_empty() {
    said.push(format!(
      "reload: this moved the worker's prefix: {}; each kept session takes it on its next run",
      moved.join(", ")
    ));
  }

  if said.is_empty() {
    said.push("reload: no session holds these documents; nothing told".to_owned());
  }

  Ok(said)
}

/// Agency's prompt-composing entry, from the code tree's root (docs/agent-documents.md).
pub const DEPLOY_AGENTS: &str = "packages/agent/deploy-agents.ts";

/// The agents' documents written again by agency's own composition, its faults said on the error stream.
pub struct TreeRedeploy {
  pub bun: PathBuf,
  pub tree: PathBuf,
}

impl Redeploy for TreeRedeploy {
  fn redeploy(&self) {
    let ran = std::process::Command::new(&self.bun)
      .arg(DEPLOY_AGENTS)
      .current_dir(&self.tree)
      .stdin(std::process::Stdio::null())
      .stdout(std::process::Stdio::null())
      .status();

    if let Err(why) = ran {
      eprintln!("reload: the agents' documents could not be written again: {why}");
    }
  }
}
