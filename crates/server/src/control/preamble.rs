//! The preamble bundle a job's prompt carries under its title (docs/worker-bench.md, 「Preamble」): agency's
//! header naming the job's checkout, its workbench and its dev server, and the repository's preamble below it
//! as the owner wrote it, put into the worker's language (docs/worker-language.md, 「What agency translates on the
//! way in」). It is read afresh each time a session is handed the job's prompt and is never kept on the job's row.

use std::path::{Path, PathBuf};

use policy::fill_marks;
use policy::job_prompt::JOB_PROMPT;

use crate::places::checkouts::git_at;
use crate::places::loan_branch::{job_branch, standing_branch};
use crate::places::{Ground, VERBS_BINARY};
use crate::verbs::dev_servers::DEV_SERVER_LOG;

/// The verb a job starts, restarts and stops its checkout's dev server with (docs/worker-bench.md, 「A job
/// starts, restarts and stops it」).
const DEV_SERVER_VERB: &str = "dev-server";

/// What a job's checkout tells the job, as it stands when the job's prompt is handed over.
#[derive(Debug, Clone)]
pub struct Bench {
  pub checkout: String,
  pub repository: String,
  pub path: String,
  pub item: i64,
  /// Its number among the repository's support checkouts; none for a base or a node checkout.
  pub support: Option<i64>,
  /// What git says of its directory.
  pub tree: Tree,
  /// What stands for it on this machine; none for a checkout on a work node.
  pub here: Option<Here>,
  /// The repository's preamble, the owner's text; none where it is empty.
  pub preamble: Option<String>,
}

/// What git says of a checkout's directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tree {
  /// A git checkout on this machine, a clone or a worktree alike: where it stands and its origin's address,
  /// any user and password taken out of it; none where it has no origin.
  Git { head: Head, origin: Option<String> },
  /// A checkout on a work node, taken to be a git checkout, since git is run in the checkout itself.
  OnNode,
  /// A directory that is no git checkout.
  Plain,
}

/// Where a git checkout stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
  /// On the branch, by its short name.
  Branch(String),
  /// No branch, at the commit by its short name.
  Detached(String),
}

impl Tree {
  /// What git says of the directory now: a git checkout where it holds `.git`, a worktree's file among them.
  pub fn read(cwd: &str) -> Self {
    if !Path::new(cwd).join(".git").exists() {
      return Self::Plain;
    }

    let head = match standing_branch(cwd) {
      Some(branch) => Head::Branch(branch),
      None => Head::Detached(git_at(cwd, &["rev-parse", "--short", "HEAD"]).unwrap_or_default()),
    };
    let origin = git_at(cwd, &["remote", "get-url", "origin"])
      .ok()
      .filter(|address| !address.is_empty())
      .map(|address| without_credentials(&address));

    Self::Git { head, origin }
  }

  fn branch(&self) -> Option<&str> {
    match self {
      Self::Git {
        head: Head::Branch(branch),
        ..
      } => Some(branch),
      _ => None,
    }
  }
}

// A remote's address with any user and password taken out, since the prompt is read by the job's model; an
// address that is no URL, a path or git's `host:path` form, carries no password and is shown as it stands.
fn without_credentials(address: &str) -> String {
  let Ok(mut url) = url::Url::parse(address) else {
    return address.to_owned();
  };

  if url.cannot_be_a_base() {
    return address.to_owned();
  }

  let _ = url.set_username("");
  let _ = url.set_password(None);

  url.to_string()
}

/// What stands for a checkout on this machine.
#[derive(Debug, Clone)]
pub struct Here {
  pub workbench: PathBuf,
  /// The verbs' binary, where the repository has a dev-server command.
  pub dev: Option<PathBuf>,
}

impl Bench {
  /// The bench of the checkout a job of the item runs in, read from its row and its directory now; none where
  /// the checkout is no repository's, or its row cannot be read.
  pub async fn of_job(ground: &Ground, checkout: &str, item: i64) -> Option<Self> {
    let worker = match crate::queue::workers::worker_row(&ground.db, checkout, Path::new("")).await {
      Ok(Some(worker)) if !worker.builtin => worker,
      Ok(_) => return None,
      Err(refusal) => {
        eprintln!("checkout {checkout}: its preamble was not read: {}", refusal.detail);

        return None;
      }
    };
    let repository = match crate::repositories::named(&ground.db, worker.repository()).await {
      Ok(Some(repository)) => repository,
      Ok(None) => return None,
      Err(fault) => {
        eprintln!("checkout {checkout}: its preamble was not read: {}", fault.detail);

        return None;
      }
    };
    let on_node = worker.node.is_some();
    let has_dev = repository.dev_command.as_deref().is_some_and(|command| !command.trim().is_empty());
    // Git is run in the checkout itself, so a checkout on a node is taken to be one and is not read.
    let tree = match on_node {
      true => Tree::OnNode,
      false => {
        let cwd = worker.cwd.clone();

        tokio::task::spawn_blocking(move || Tree::read(&cwd)).await.unwrap_or(Tree::Plain)
      }
    };
    let here = (!on_node).then(|| Here {
      workbench: crate::places::dev::workbench(ground, &worker.name),
      dev: has_dev.then(|| ground.code_root().join("bin").join(VERBS_BINARY)),
    });

    Some(Self {
      checkout: worker.name.clone(),
      repository: repository.name.clone(),
      path: worker.cwd.clone(),
      item,
      support: worker.support_slot,
      tree,
      here,
      preamble: repository.preamble.filter(|text| !text.trim().is_empty()),
    })
  }

  /// The item's branch, where the checkout stands on it.
  pub fn item_branch(&self) -> Option<&str> {
    self.tree.branch().filter(|branch| *branch == job_branch(self.item))
  }

  /// Whether the job's prompt carries a bundle at all: a preamble, a dev server here, a directory that is no git
  /// checkout, a support checkout or a checkout on the item's branch.
  fn told(&self) -> bool {
    self.preamble.is_some()
      || self.here.as_ref().is_some_and(|here| here.dev.is_some())
      || self.tree == Tree::Plain
      || self.support.is_some()
      || self.item_branch().is_some()
  }

  /// The bundle: the header, then the repository's preamble under its own heading; empty where nothing is told.
  pub fn words(&self) -> String {
    if !self.told() {
      return String::new();
    }

    let words = &JOB_PROMPT.bench;
    let mut lines = vec![fill_marks(
      &words.checkout,
      &[("checkout", &self.checkout), ("repository", &self.repository), ("path", &self.path)],
    )];

    if let Some(slot) = self.support {
      lines.push(fill_marks(
        &words.support,
        &[("slot", &slot.to_string()), ("repository", &self.repository)],
      ));
    }

    if let Some(line) = self.tree_line() {
      lines.push(line);
    }

    if let Some(branch) = self.item_branch() {
      lines.push(fill_marks(&words.branch, &[("branch", branch)]));
    }

    if let Some(here) = &self.here {
      lines.push(self.workbench_line(&here.workbench));
    }

    if let Some(here) = &self.here
      && let Some(binary) = &here.dev
    {
      lines.push(self.dev_line(&here.workbench, binary));
    }

    let mut text = format!("{}\n\n{}\n", words.heading, lines.join("\n\n"));

    if let Some(preamble) = &self.preamble {
      text.push_str(&format!("\n{}\n\n{}\n", words.preamble_heading, preamble.trim_end()));
    }

    text
  }

  /// The bench with the owner's preamble put into the worker's language, or under the untranslated mark where
  /// the gate did not render it whole in time.
  pub async fn in_english(mut self, gate: &dyn crate::peers::gate::Gate) -> Self {
    if let Some(preamble) = self.preamble.take() {
      self.preamble = Some(crate::worker_english::english_one(gate, &preamble).await);
    }

    self
  }

  // What git says of the directory, told where it was read here.
  fn tree_line(&self) -> Option<String> {
    let words = &JOB_PROMPT.bench;

    match &self.tree {
      Tree::Git { head, origin } => {
        let head = match head {
          Head::Branch(branch) => fill_marks(&words.on_branch, &[("branch", branch)]),
          Head::Detached(commit) => fill_marks(&words.detached, &[("commit", commit)]),
        };
        let origin = match origin {
          Some(origin) => fill_marks(&words.origin, &[("origin", origin)]),
          None => words.no_origin.clone(),
        };

        Some(fill_marks(&words.git, &[("path", &self.path), ("head", &head), ("origin", &origin)]))
      }
      Tree::OnNode => None,
      Tree::Plain => Some(fill_marks(&words.not_checkout, &[("path", &self.path)])),
    }
  }

  fn workbench_line(&self, workbench: &Path) -> String {
    let names = &JOB_PROMPT.workbench;
    let scratch = workbench.join(&names.scratch).join(self.item.to_string());

    fill_marks(
      &JOB_PROMPT.bench.workbench,
      &[
        ("workbench", &workbench.display().to_string()),
        ("scratch", &format!("{}/", scratch.display())),
        ("database", &format!("{}{}_", names.item_database, self.item)),
      ],
    )
  }

  fn dev_line(&self, workbench: &Path, binary: &Path) -> String {
    let line = |action: &str| format!("{} {DEV_SERVER_VERB} {action} {}", binary.display(), self.checkout);

    fill_marks(
      &JOB_PROMPT.bench.dev,
      &[
        ("start", &line("start")),
        ("restart", &line("restart")),
        ("stop", &line("stop")),
        ("log", &workbench.join(DEV_SERVER_LOG).display().to_string()),
      ],
    )
  }
}

/// A job's prompt with its title in the worker's language and its checkout's bundle under it, read now and its
/// preamble put into the worker's language; the prompt with its title alone so put where the bundle says nothing.
pub async fn bundled(ground: &Ground, checkout: &str, item: i64, prompt: &str) -> String {
  let gate = ground.peers.gate.as_ref();

  match Bench::of_job(ground, checkout, item).await {
    Some(bench) => {
      let (prompt, bench) =
        tokio::join!(crate::control::work::title_in_english(gate, prompt), bench.in_english(gate));

      crate::control::work::under_title(&prompt, &bench.words())
    }
    None => crate::control::work::title_in_english(gate, prompt).await,
  }
}
