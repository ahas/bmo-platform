//! The runner's keeping of an owner's personal apps (docs/person-nodes.md, 「Personal apps」, 「Personal screens」 and
//! 「Shared apps」): an app's folder of 'my apps' built confined into a new version, which keeps the source it was
//! built from, its screen's index written, a version's description and its source read, the tool's pointer moved and
//! the versions beyond those kept taken away, what each tool keeps listed, a tool taken away, one file of a version's
//! screen read, a shared app's source received into its folder once its hash is the one given, and a received app
//! adopted as the person's own. Each is asked on the passage's first line, `{"owner": <account>, "<what>": {…}}`, and
//! answered by one line.

use std::collections::BTreeMap;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::mcp::{MCP_FILE, MCP_TOOLS, McpFile};
use crate::runner::{Answered, Kept, Runner, apps_dir, machine_words, plain_name, program_stands};

/// The manifest's file in an app's folder and in each version.
pub const MANIFEST: &str = "tool.toml";

/// How many versions older than the one pointed at a tool keeps.
pub const KEEP: usize = 3;

/// The file inside a version that keeps the source it was placed from, and the file inside an app's folder that
/// marks it a received copy of a shared app.
pub const SOURCE_KEPT: &str = ".bmo-source.json";
pub const RECEIVED_MARK: &str = ".bmo-received.json";

/// The most bytes a version's kept source holds, the most a company's catalogue takes.
pub const SOURCE_MOST: usize = 8 * 1024 * 1024;

/// The hash of a source's files as the company catalogue reckons it (the tool host's `docs/catalogue.md`, 「The
/// source hash」): the SHA-256 of the files in the byte order of their paths, each its path, a zero byte, its length
/// in decimal, a zero byte and its bytes.
pub fn source_hash(files: &BTreeMap<String, String>) -> String {
  use sha2::Digest as _;

  let mut hasher = sha2::Sha256::new();

  for (path, text) in files {
    hasher.update(path.as_bytes());
    hasher.update([0]);
    hasher.update(text.len().to_string().as_bytes());
    hasher.update([0]);
    hasher.update(text.as_bytes());
  }

  hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

// Every file of an app's folder by its path, as text: the files its own git repository keeps or would keep where
// the folder is one, else every file, its `.git` and the received mark left out either way; none where a file is not
// UTF-8 text or the whole holds more than a catalogue takes.
fn source_files(folder: &Path) -> Option<BTreeMap<String, String>> {
  match git_listed(folder) {
    Some(paths) => listed_files(folder, &paths),
    None => walked_files(folder),
  }
}

// The paths the folder's own git repository keeps or would keep: tracked, or untracked and not ignored, so what its
// ignore rules leave out — a test run's caches — stays out. None where the folder is no repository of its own or git
// does not answer.
fn git_listed(folder: &Path) -> Option<Vec<String>> {
  if !folder.join(".git").exists() {
    return None;
  }

  let listed = std::process::Command::new("git")
    .arg("-C")
    .arg(folder)
    .args(["ls-files", "-z", "--cached", "--others", "--exclude-standard"])
    .stdin(Stdio::null())
    .stderr(Stdio::null())
    .output()
    .ok()?;

  if !listed.status.success() {
    return None;
  }

  let text = String::from_utf8(listed.stdout).ok()?;

  Some(
    text
      .split('\0')
      .filter(|path| !path.is_empty() && *path != RECEIVED_MARK)
      .filter(|path| folder.join(path).is_file())
      .map(str::to_owned)
      .collect(),
  )
}

// The listed files read as text.
fn listed_files(folder: &Path, paths: &[String]) -> Option<BTreeMap<String, String>> {
  let mut files = BTreeMap::new();
  let mut total = 0;

  for path in paths {
    let text = String::from_utf8(std::fs::read(folder.join(path)).ok()?).ok()?;

    total += text.len();

    if total > SOURCE_MOST {
      return None;
    }

    files.insert(path.clone(), text);
  }

  Some(files)
}

// Every file under the folder read as text, its `.git` and the received mark left out.
fn walked_files(folder: &Path) -> Option<BTreeMap<String, String>> {
  fn walk(base: &Path, dir: &Path, files: &mut BTreeMap<String, String>, total: &mut usize) -> Option<()> {
    for entry in std::fs::read_dir(dir).ok()? {
      let entry = entry.ok()?;
      let name = entry.file_name();

      if dir == base && (name == ".git" || name == RECEIVED_MARK) {
        continue;
      }

      let kind = entry.file_type().ok()?;

      if kind.is_dir() {
        walk(base, &entry.path(), files, total)?;
      } else if kind.is_file() {
        let text = String::from_utf8(std::fs::read(entry.path()).ok()?).ok()?;
        let path = entry.path().strip_prefix(base).ok()?.to_string_lossy().into_owned();

        *total += text.len();

        if *total > SOURCE_MOST {
          return None;
        }

        files.insert(path, text);
      }
    }

    Some(())
  }

  let mut files = BTreeMap::new();

  walk(folder, folder, &mut files, &mut 0)?;
  Some(files)
}

/// Where a confined build reads the app's folder and writes the new version.
const SOURCE_AT: &str = "/src";
const OUT_AT: &str = "/out";

/// Where a confined build reads the screen kit, and the runtime it runs the kit's build with.
const KIT_AT: &str = "/kit";
const RUNTIME_AT: &str = "/runtime";

/// The folder of a version a screen stands in, the files its description carries, its index and its module
/// (bmo-pocket's docs/api.md, 「`[screen]`」).
pub const SCREEN_FOLDER: &str = "screen";
pub const SCREEN_INDEX: &str = "files.json";
const SCREEN_DESCRIBED: [&str; 3] = ["words.json", "layer.json", SCREEN_INDEX];
const SCREEN_MODULE: &str = "module.js";

/// What a keeping asks, by the one field beside `owner` its line names.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum Asked {
  Place {
    tool: String,
    /// For an MCP server's folder, the addresses its listing may reach.
    #[serde(default)]
    network: Vec<String>,
    /// For an MCP server's folder, the secrets its listing is handed, each file's bytes in base64 by its name.
    #[serde(default)]
    secrets: BTreeMap<String, String>,
  },
  Describe {
    tool: String,
    version: String,
  },
  Point {
    tool: String,
    version: String,
  },
  Versions {},
  Take {
    tool: String,
  },
  File {
    tool: String,
    version: String,
    path: String,
  },
  Source {
    tool: String,
    version: String,
  },
  Receive {
    tool: String,
    files: BTreeMap<String, String>,
    hash: String,
    app: String,
  },
  Adopt {
    tool: String,
  },
}

#[derive(Deserialize)]
struct Line {
  owner: String,
  #[serde(flatten)]
  asked: Asked,
}

// What of a manifest the runner reads: the tool's name and build, and the files its entries name. The tool host
// reads the whole of it when the description is put at home.
#[derive(Deserialize)]
struct Manifest {
  tool: ToolPart,
  #[serde(default)]
  entry: Vec<EntryPart>,
  // Its presence says the version holds a screen; what it holds is the tool host's to read.
  #[serde(default)]
  screen: Option<toml::Value>,
}

#[derive(Deserialize)]
struct ToolPart {
  name: String,
  #[serde(default)]
  build: Option<String>,
}

#[derive(Deserialize)]
struct EntryPart {
  input: String,
  #[serde(default)]
  output: Option<String>,
  #[serde(default)]
  manual: Option<String>,
}

impl Manifest {
  fn read(folder: &Path) -> Result<Self, String> {
    let text = std::fs::read_to_string(folder.join(MANIFEST))
      .map_err(|why| format!("{} holds no {MANIFEST}: {why}", folder.display()))?;

    toml::from_str(&text).map_err(|why| format!("its {MANIFEST} does not read: {why}"))
  }

  // The manifest and every file its entries name, each by its path in the folder.
  fn named_files(&self) -> Vec<String> {
    let mut files = vec![MANIFEST.to_owned()];

    for entry in &self.entry {
      files.extend(
        std::iter::once(entry.input.clone())
          .chain(entry.output.clone())
          .chain(entry.manual.clone()),
      );
    }

    files.sort();
    files.dedup();
    files
  }

  // The files its description carries: those it names, and its screen's words, layer and index where it has one.
  fn described_files(&self) -> Vec<String> {
    let mut files = self.named_files();

    if self.screen.is_some() {
      files.extend(SCREEN_DESCRIBED.map(|file| format!("{SCREEN_FOLDER}/{file}")));
      files.sort();
    }

    files
  }
}

// Every file under a folder, by its path below it, in order.
fn files_below(dir: &Path, under: &str, found: &mut Vec<String>) -> std::io::Result<()> {
  for entry in std::fs::read_dir(dir)? {
    let entry = entry?;
    let name = entry.file_name().to_string_lossy().into_owned();
    let path = if under.is_empty() { name } else { format!("{under}/{name}") };
    let kind = entry.file_type()?;

    if kind.is_dir() {
      files_below(&entry.path(), &path, found)?;
    } else if kind.is_file() {
      found.push(path);
    }
  }

  found.sort();
  Ok(())
}

/// A version's screen given its index: every file of its `screen/` folder but the index, by its path below it, with
/// the SHA-256 of its bytes, written over whatever stood there; refused where the folder holds no module, layer or
/// words.
pub fn write_index(version: &Path) -> Result<(), String> {
  let screen = version.join(SCREEN_FOLDER);

  for needed in [SCREEN_MODULE, "layer.json", "words.json"] {
    if !screen.join(needed).is_file() {
      return Err(format!("its screen holds no {SCREEN_FOLDER}/{needed}"));
    }
  }

  let mut found = Vec::new();

  files_below(&screen, "", &mut found).map_err(|why| format!("its screen could not be read: {why}"))?;

  let mut index = serde_json::Map::new();

  for path in found.into_iter().filter(|path| path != SCREEN_INDEX) {
    let bytes =
      std::fs::read(screen.join(&path)).map_err(|why| format!("{SCREEN_FOLDER}/{path}: {why}"))?;

    index.insert(path, json!(format!("{:x}", Sha256::digest(&bytes))));
  }

  std::fs::write(screen.join(SCREEN_INDEX), serde_json::Value::Object(index).to_string())
    .map_err(|why| format!("its screen's index could not be written: {why}"))
}

// One file of a version's screen, its bytes in base64; a path climbing out of the screen's folder is refused.
fn screen_file(version: &Path, path: &str) -> Answered {
  if !inside(path) {
    return Answered::Refused {
      why: format!("{path:?} is no file inside a screen's folder"),
    };
  }

  match std::fs::read(version.join(SCREEN_FOLDER).join(path)) {
    Ok(bytes) => Answered::File {
      bytes: base64::engine::general_purpose::STANDARD.encode(bytes),
    },
    Err(why) => Answered::Refused {
      why: format!("{SCREEN_FOLDER}/{path} of the version could not be read: {why}"),
    },
  }
}

// A path inside a folder, made of folder and file names alone, so it never climbs out.
fn inside(path: &str) -> bool {
  !path.is_empty()
    && Path::new(path).components().all(|part| matches!(part, std::path::Component::Normal(_)))
}

/// The folders of one owner's tool: its pointer, its versions and its app's source.
struct Owned {
  root: PathBuf,
  apps: PathBuf,
  owner: String,
}

impl Owned {
  fn of(runner: &Runner, owner: &str) -> Self {
    Self {
      root: runner.root.join("owners").join(owner),
      apps: apps_dir(&runner.root),
      owner: owner.to_owned(),
    }
  }

  fn versions(&self, tool: &str) -> PathBuf {
    self.root.join("tools").join(tool)
  }

  fn pointer(&self, tool: &str) -> PathBuf {
    self.root.join("registry").join(format!("{tool}.json"))
  }

  fn current(&self, tool: &str) -> Option<String> {
    let text = std::fs::read_to_string(self.pointer(tool)).ok()?;
    let pointer: serde_json::Value = serde_json::from_str(&text).ok()?;

    pointer["version"].as_str().map(str::to_owned)
  }

  // The versions standing of a tool, staged ones left out, oldest first: a version's name opens on the moment it
  // was placed.
  fn standing(&self, tool: &str) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(self.versions(tool))
      .into_iter()
      .flatten()
      .flatten()
      .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
      .filter_map(|entry| entry.file_name().into_string().ok())
      .filter(|name| plain_name(name))
      .collect();

    found.sort_by(|one, other| (one.len(), one).cmp(&(other.len(), other)));
    found
  }

  // What a tool keeps: its version pointed at and those older than it, newest first; every version where no
  // pointer stands.
  fn kept(&self, tool: &str) -> Kept {
    let current = self.current(tool);
    let mut older: Vec<String> = self
      .standing(tool)
      .into_iter()
      .take_while(|version| Some(version) != current.as_ref())
      .collect();

    older.reverse();
    Kept {
      current,
      kept: older,
    }
  }
}

// A version's name: the moment it is placed, and a suffix no other place of the same moment holds.
fn fresh() -> String {
  let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
  let suffix = (now.subsec_nanos() ^ std::process::id().rotate_left(16)) & 0x00ff_ffff;

  format!("{}-{suffix:06x}", now.as_millis())
}

// A folder copied whole into another, its `.git` left out.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
  std::fs::create_dir_all(to)?;

  for entry in std::fs::read_dir(from)? {
    let entry = entry?;

    if entry.file_name() == ".git" {
      continue;
    }

    let (path, into) = (entry.path(), to.join(entry.file_name()));
    let kind = entry.file_type()?;

    if kind.is_dir() {
      copy_tree(&path, &into)?;
    } else if kind.is_file() {
      std::fs::copy(&path, &into)?;
    }
  }

  Ok(())
}

// A version's description: its manifest and the files it names, as text, by path.
fn description(version: &Path) -> Result<BTreeMap<String, String>, String> {
  let manifest = Manifest::read(version)?;
  let mut files = BTreeMap::new();

  for file in manifest.described_files() {
    if !inside(&file) {
      return Err(format!("its manifest names {file}, which is not inside its folder"));
    }

    let text = std::fs::read_to_string(version.join(&file)).map_err(|why| format!("{file}: {why}"))?;

    files.insert(file, text);
  }

  Ok(files)
}

/// The words bubblewrap runs a build with: the machine's own confinement, the app's folder read alone at `/src`,
/// where it starts, and the new version's folder written at `/out`, which `BMO_TOOL_OUT` names; `sh` running the
/// manifest's build. Where a screen kit is given, the kit read alone at `/kit` and the runtime at `/runtime/bun`,
/// `/runtime` first on the search path (docs/person-nodes.md, 「Personal screens」).
pub fn build_words(source: &Path, out: &Path, build: &str, kit: Option<(&Path, &Path)>) -> Vec<String> {
  let mut words = machine_words();

  words.extend(["--ro-bind".into(), source.to_string_lossy().into_owned(), SOURCE_AT.into()]);
  words.extend(["--bind".into(), out.to_string_lossy().into_owned(), OUT_AT.into()]);

  let path = match kit {
    Some((kit, runtime)) => {
      words.extend(["--ro-bind".into(), kit.to_string_lossy().into_owned(), KIT_AT.into()]);
      words.extend([
        "--ro-bind".into(),
        runtime.to_string_lossy().into_owned(),
        format!("{RUNTIME_AT}/bun"),
      ]);
      format!("{RUNTIME_AT}:{}", crate::runner::confined_path())
    }
    None => crate::runner::confined_path().to_owned(),
  };

  words.extend(["--chdir".into(), SOURCE_AT.into()]);

  for (name, value) in [
    ("PATH", path.as_str()),
    ("HOME", "/tmp"),
    ("TMPDIR", "/tmp"),
    ("LANG", "C.UTF-8"),
    ("BMO_TOOL_OUT", OUT_AT),
  ] {
    words.extend(["--setenv".into(), name.into(), value.into()]);
  }

  words.extend(["--".into(), "sh".into(), "-c".into(), build.to_owned()]);
  words
}

// The next line a build wrote on its errors, none once they end.
async fn next_line(
  errors: &mut Option<tokio::io::Lines<BufReader<tokio::process::ChildStderr>>>,
) -> Option<String> {
  match errors {
    Some(lines) => lines.next_line().await.ok().flatten(),
    None => None,
  }
}

// A staged folder taken away as it is dropped, unless it was kept.
struct Staged(Option<PathBuf>);

impl Drop for Staged {
  fn drop(&mut self) {
    if let Some(dir) = &self.0 {
      let _ = std::fs::remove_dir_all(dir);
    }
  }
}

// What a place reads an app's folder by: its manifest, or an MCP server's `mcp.toml` the manifest is made for.
enum Shape {
  App(Manifest),
  Mcp(McpFile),
}

// What a place carries for an MCP server's listing: the addresses it may reach, and the secrets it is handed.
type Carried<'a> = (&'a [String], &'a BTreeMap<String, String>);

// The app's folder placed as a new version: built confined where its manifest names a build, copied whole
// otherwise; each line the build writes on its errors said as it comes. The caller going away ends the build. An
// MCP server's folder is placed as an app's, then its tools listed and its manifest made of them; where it asks for
// what the place does not carry, nothing is placed and what it asks for is answered.
async fn place(
  runner: &Runner,
  owned: &Owned,
  tool: &str,
  carried: Carried<'_>,
  input: &mut (impl AsyncRead + Unpin),
  said: &mut (impl AsyncWrite + Unpin),
) -> Answered {
  let source = owned.apps.join(tool);
  let shape = if source.join(MCP_FILE).is_file() {
    McpFile::read(&source).map(Shape::Mcp)
  } else {
    Manifest::read(&source).map(Shape::App)
  };
  let shape = match shape {
    Ok(shape) => shape,
    Err(why) => {
      return Answered::Refused {
        why: format!("the app {tool}: {why}"),
      };
    }
  };
  let (named, build, files, screen) = match &shape {
    Shape::App(manifest) => (
      &manifest.tool.name,
      manifest.tool.build.as_deref(),
      manifest.named_files(),
      manifest.screen.is_some(),
    ),
    Shape::Mcp(file) => (&file.server.name, file.server.build.as_deref(), vec![MCP_FILE.to_owned()], false),
  };

  if named != tool {
    let why = match &shape {
      Shape::App(_) => format!("the app {tool}'s manifest names the tool {named}, not its folder's name"),
      Shape::Mcp(_) => {
        format!("the app {tool}'s {MCP_FILE} names the server {named}, not its folder's name")
      }
    };

    return Answered::Refused { why };
  }

  if let Shape::Mcp(file) = &shape {
    let asks = file.asks();
    let (network, secrets) = carried;

    if asks.network.iter().any(|address| !network.contains(address))
      || asks.secrets.iter().any(|secret| !secrets.contains_key(secret))
    {
      return Answered::Wants {
        network: asks.network,
        secrets: asks.secrets,
      };
    }
  }

  let version = fresh();
  let staged = owned.versions(tool).join(format!(".staging-{version}"));
  let mut staging = Staged(Some(staged.clone()));

  if let Err(why) = std::fs::create_dir_all(&staged) {
    return Answered::Refused {
      why: format!("{}: {why}", staged.display()),
    };
  }

  match build.filter(|build| !build.trim().is_empty()) {
    Some(build) => {
      if !program_stands(&runner.bwrap) {
        return Answered::Refused {
          why: format!(
            "bubblewrap ({}) is not on this machine, and an app builds confined or not at all",
            runner.bwrap
          ),
        };
      }

      // The kit and the runtime the runner finds, where both stand.
      let runtime = crate::runner::program_path("bun");
      let kit = runner.kit.as_deref().filter(|kit| kit.is_dir()).zip(runtime.as_deref());
      let built = tokio::process::Command::new(&runner.bwrap)
        .args(build_words(&source, &staged, build, kit))
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();
      let mut child = match built {
        Ok(child) => child,
        Err(why) => {
          return Answered::Refused {
            why: format!("bubblewrap ({}) could not be started: {why}", runner.bwrap),
          };
        }
      };
      let mut errors = child.stderr.take().map(|stderr| BufReader::new(stderr).lines());
      let mut last = Vec::new();
      let mut rest = Vec::new();
      let gone = input.read_to_end(&mut rest);

      tokio::pin!(gone);

      loop {
        tokio::select! {
          line = next_line(&mut errors) => match line {
            Some(line) => {
              let _ = said.write_all(Answered::Said { line: line.clone() }.line().as_bytes()).await;
              last.push(line);

              if last.len() > 5 {
                last.remove(0);
              }
            }
            None => break,
          },
          _ = &mut gone => {
            let _ = child.start_kill();
            let _ = child.wait().await;

            return Answered::Refused { why: "the caller went away before the build ended".to_owned() };
          }
        }
      }

      let ended = child.wait().await.ok();

      if !ended.is_some_and(|status| status.success()) {
        return Answered::Refused {
          why: format!("the app {tool}'s build failed: {}", last.join("\n")),
        };
      }

      for file in files {
        if !inside(&file) {
          return Answered::Refused {
            why: format!("its manifest names {file}, which is not inside its folder"),
          };
        }

        let to = staged.join(&file);

        if let Some(parent) = to.parent()
          && let Err(why) = std::fs::create_dir_all(parent)
        {
          return Answered::Refused {
            why: format!("{}: {why}", parent.display()),
          };
        }

        if let Err(why) = std::fs::copy(source.join(&file), &to) {
          return Answered::Refused {
            why: format!("{file} could not be copied: {why}"),
          };
        }
      }
    }
    None => {
      if let Err(why) = copy_tree(&source, &staged) {
        return Answered::Refused {
          why: format!("{} could not be copied: {why}", source.display()),
        };
      }
    }
  }

  if screen && let Err(why) = write_index(&staged) {
    return Answered::Refused {
      why: format!("the app {tool}'s placed version: {why}"),
    };
  }

  if let Shape::Mcp(file) = &shape
    && let Err(why) = listed_into(runner, owned, tool, &staged, file, carried, input, said).await
  {
    return Answered::Refused {
      why: format!("the MCP server {tool}: {why}"),
    };
  }

  // The source the version was placed from, kept inside it, so a version shared later is shared as it was built.
  if let Some(source) = source_files(&source)
    && let Ok(text) = serde_json::to_string(&source)
  {
    let _ = std::fs::write(staged.join(SOURCE_KEPT), text);
  }

  let files = match description(&staged) {
    Ok(files) => files,
    Err(why) => {
      return Answered::Refused {
        why: format!("the app {tool}'s placed version: {why}"),
      };
    }
  };
  let placed = owned.versions(tool).join(&version);

  if let Err(why) = std::fs::rename(&staged, &placed) {
    return Answered::Refused {
      why: format!("{} could not be placed: {why}", placed.display()),
    };
  }

  staging.0 = None;
  Answered::Placed { version, files }
}

// An MCP server's staged version given its manifest: its tools listed by the runner's own binary run confined as a
// call runs, `node-link mcp --list`, reaching the addresses and handed the secrets the place carries, each line it
// writes on its errors said masked as it comes; then the manifest made of them, its schemas and the entries' server
// tools written into the version. A listing that fails or lists nothing is refused, as is the caller going away.
#[allow(clippy::too_many_arguments)]
async fn listed_into(
  runner: &Runner,
  owned: &Owned,
  tool: &str,
  staged: &Path,
  file: &McpFile,
  (network, secrets): Carried<'_>,
  input: &mut (impl AsyncRead + Unpin),
  said: &mut (impl AsyncWrite + Unpin),
) -> Result<(), String> {
  if !program_stands(&runner.bwrap) {
    return Err(format!(
      "bubblewrap ({}) is not on this machine, and an MCP server is listed confined or not at all",
      runner.bwrap
    ));
  }

  let data = crate::runner::data_dir(&runner.root, &owned.owner, tool);

  std::fs::DirBuilder::new()
    .recursive(true)
    .mode(0o700)
    .create(&data)
    .map_err(|why| format!("{}: {why}", data.display()))?;

  let run = [crate::runner::OWN_BINARY_AT, "mcp", "--list"].map(str::to_owned);
  let mut launched = crate::runner::launch(runner, staged, &data, &run, network, secrets, Stdio::null())?;
  let masks = launched.masks.clone();
  let mut child = launched.take_child();
  let mut errors = child.stderr.take().map(|stderr| BufReader::new(stderr).lines());
  let reading = child.stdout.take().map(|mut stdout| {
    tokio::spawn(async move {
      let mut out = Vec::new();
      let _ = stdout.read_to_end(&mut out).await;
      out
    })
  });
  let mut last = Vec::new();
  let mut rest = Vec::new();
  let gone = input.read_to_end(&mut rest);

  tokio::pin!(gone);

  loop {
    tokio::select! {
      line = next_line(&mut errors) => match line {
        Some(line) => {
          let line = crate::runner::masked(&masks, line);
          let _ = said.write_all(Answered::Said { line: line.clone() }.line().as_bytes()).await;

          last.push(line);

          if last.len() > 5 {
            last.remove(0);
          }
        }
        None => break,
      },
      _ = &mut gone => {
        let _ = child.start_kill();
        let _ = child.wait().await;

        return Err("the caller went away before its tools were listed".to_owned());
      }
    }
  }

  let out = match reading {
    Some(reading) => reading.await.unwrap_or_default(),
    None => Vec::new(),
  };
  let ended = child.wait().await.ok();

  launched.end();

  if !ended.is_some_and(|status| status.success()) {
    return Err(format!("its tools could not be listed: {}", last.join("\n")));
  }

  let out = crate::runner::masked(&masks, String::from_utf8_lossy(&out).into_owned());
  let listing: crate::mcp::Listing =
    serde_json::from_str(out.trim()).map_err(|why| format!("its listing does not read: {why}"))?;
  let made = crate::mcp::made(file, &listing.tools)?;
  let names = serde_json::to_string_pretty(&made.names)
    .map_err(|why| format!("its {MCP_TOOLS} could not be written: {why}"))?;

  for (path, text) in std::iter::once((MANIFEST.to_owned(), made.manifest))
    .chain(made.schemas)
    .chain(std::iter::once((MCP_TOOLS.to_owned(), names)))
  {
    let to = staged.join(&path);

    to.parent()
      .map_or(Ok(()), std::fs::create_dir_all)
      .and_then(|()| std::fs::write(&to, text))
      .map_err(|why| format!("{path} could not be written: {why}"))?;
  }

  Ok(())
}

// The tool's pointer moved onto a version standing, written whole; then every version but it and the newest kept
// older than it taken away, a version still being staged left as it is.
fn point(owned: &Owned, tool: &str, version: &str) -> Answered {
  let dir = owned.versions(tool).join(version);

  if !dir.join(MANIFEST).is_file() {
    return Answered::Refused {
      why: format!("version {version} of the app {tool} is not placed on this machine"),
    };
  }

  let pointers = owned.root.join("registry");
  let pointer = json!({
    "name": tool,
    "version": version,
    "dir": dir,
    "owner": owned.owner,
    "putAt": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |at| at.as_millis() as u64),
  });
  let staged = pointers.join(format!(".{tool}.json.next"));
  let written = std::fs::create_dir_all(&pointers)
    .and_then(|()| std::fs::write(&staged, pointer.to_string()))
    .and_then(|()| std::fs::rename(&staged, owned.pointer(tool)));

  if let Err(why) = written {
    return Answered::Refused {
      why: format!("the app {tool}'s pointer could not be written: {why}"),
    };
  }

  let Kept { kept, .. } = owned.kept(tool);
  let keeping: Vec<String> = kept.into_iter().take(KEEP).collect();

  for standing in owned.standing(tool) {
    if standing != version && !keeping.contains(&standing) {
      let _ = std::fs::remove_dir_all(owned.versions(tool).join(standing));
    }
  }

  Answered::Pointed {
    version: version.to_owned(),
    kept: keeping,
  }
}

// The source a version was placed from, as the version keeps it.
fn kept_source(owned: &Owned, tool: &str, version: &str) -> Answered {
  let file = owned.versions(tool).join(version).join(SOURCE_KEPT);
  let read = std::fs::read_to_string(&file)
    .ok()
    .and_then(|text| serde_json::from_str::<BTreeMap<String, String>>(&text).ok());

  match read {
    Some(files) => Answered::Source {
      version: version.to_owned(),
      files,
    },
    None => Answered::Refused {
      why: format!(
        "version {version} of the app {tool} keeps no source to share: put it again, with text files alone"
      ),
    },
  }
}

// Whether a folder holds nothing: an empty folder of 'my apps' is no app of anyone's, so a receive writes over it.
fn empty(folder: &Path) -> bool {
  std::fs::read_dir(folder).is_ok_and(|mut entries| entries.next().is_none())
}

// The app a folder of 'my apps' is a received copy of, where it is one.
fn received_of(folder: &Path) -> Option<String> {
  let text = std::fs::read_to_string(folder.join(RECEIVED_MARK)).ok()?;

  serde_json::from_str::<serde_json::Value>(&text).ok()?["app"]
    .as_str()
    .map(str::to_owned)
}

// A shared app's source written into its folder of 'my apps' once its hash is the one given: staged beside the
// folder, marked with the app it is a copy of, and renamed over a received copy of the same app or an empty folder.
// A folder that is the person's own, or another app's copy, is refused.
fn receive(owned: &Owned, tool: &str, files: &BTreeMap<String, String>, hash: &str, app: &str) -> Answered {
  let reckoned = source_hash(files);

  if reckoned != hash {
    return Answered::Refused {
      why: format!("the source received for {tool} does not match the hash its catalogue recorded"),
    };
  }

  if let Some(path) = files.keys().find(|path| !inside(path) || path.as_str() == RECEIVED_MARK) {
    return Answered::Refused {
      why: format!("the source received for {tool} holds a file {path:?} outside the app's folder"),
    };
  }

  let folder = owned.apps.join(tool);

  if folder.exists() && !empty(&folder) && received_of(&folder).as_deref() != Some(app) {
    return Answered::Refused {
      why: format!("an app named {tool} that is no copy of {app} stands on this machine already"),
    };
  }

  let staged = owned.apps.join(format!(".receiving-{tool}-{}", fresh()));
  let written = (|| {
    for (path, text) in files {
      let to = staged.join(path);

      if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
      }

      std::fs::write(to, text)?;
    }

    std::fs::write(staged.join(RECEIVED_MARK), json!({ "app": app, "hash": hash }).to_string())?;

    if folder.exists() {
      std::fs::remove_dir_all(&folder)?;
    }

    std::fs::rename(&staged, &folder)
  })();

  if let Err(why) = written {
    let _ = std::fs::remove_dir_all(&staged);

    return Answered::Refused {
      why: format!("the source of {tool} could not be written: {why}"),
    };
  }

  Answered::Received {}
}

// A received app made the person's own: its mark taken away.
fn adopt(owned: &Owned, tool: &str) -> Answered {
  let folder = owned.apps.join(tool);

  if received_of(&folder).is_none() {
    return Answered::Refused {
      why: format!("the app {tool} on this machine is no received copy"),
    };
  }

  match std::fs::remove_file(folder.join(RECEIVED_MARK)) {
    Ok(()) => Answered::Adopted {},
    Err(why) => Answered::Refused {
      why: format!("the app {tool} could not be adopted: {why}"),
    },
  }
}

// Every tool the owner keeps versions of, each with its version pointed at and those kept beside it.
fn versions(owned: &Owned) -> Answered {
  let tools = std::fs::read_dir(owned.root.join("tools"))
    .into_iter()
    .flatten()
    .flatten()
    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
    .filter_map(|entry| entry.file_name().into_string().ok())
    .filter(|name| plain_name(name))
    .map(|tool| {
      let kept = owned.kept(&tool);

      (tool, kept)
    })
    .collect();

  Answered::Versions { tools }
}

// The tool's pointer and every version taken away; its source and its data stand.
fn take(owned: &Owned, tool: &str) -> Answered {
  let pointer = owned.pointer(tool);
  let versions = owned.versions(tool);

  if !pointer.exists() && !versions.exists() {
    return Answered::Refused {
      why: format!("this machine keeps no app {tool}"),
    };
  }

  for removed in [std::fs::remove_file(&pointer), std::fs::remove_dir_all(&versions)] {
    if let Err(why) = removed
      && why.kind() != std::io::ErrorKind::NotFound
    {
      return Answered::Refused {
        why: format!("the app {tool} could not be taken away: {why}"),
      };
    }
  }

  Answered::Taken {}
}

/// One keeping asked on the passage's first line and answered on `output`.
pub async fn keep(
  runner: Runner,
  first: &str,
  mut input: impl AsyncRead + Unpin,
  mut output: impl AsyncWrite + Unpin,
) {
  let answered = match serde_json::from_str::<Line>(first) {
    Err(why) => Answered::Refused {
      why: format!("the asking does not read: {why}"),
    },
    Ok(line) if runner.owner.as_deref().is_some_and(|owner| owner != line.owner) => Answered::Refused {
      why: format!("this runner keeps no app of the account {}", line.owner),
    },
    Ok(line) if !plain_name(&line.owner) => Answered::Refused {
      why: format!("{:?} is no owner's name", line.owner),
    },
    Ok(line) => {
      let owned = Owned::of(&runner, &line.owner);
      let named = match &line.asked {
        Asked::Place { tool, .. }
        | Asked::Take { tool }
        | Asked::Adopt { tool }
        | Asked::Receive { tool, .. } => vec![("tool", tool)],
        Asked::Describe { tool, version }
        | Asked::Point { tool, version }
        | Asked::Source { tool, version }
        | Asked::File { tool, version, .. } => {
          vec![("tool", tool), ("version", version)]
        }
        Asked::Versions {} => Vec::new(),
      };

      match named.iter().find(|(_, name)| !plain_name(name)) {
        Some((what, name)) => Answered::Refused {
          why: format!("{name:?} is no {what}'s name"),
        },
        None => match &line.asked {
          Asked::Place {
            tool,
            network,
            secrets,
          } => place(&runner, &owned, tool, (network, secrets), &mut input, &mut output).await,
          Asked::Describe { tool, version } => match description(&owned.versions(tool).join(version)) {
            Ok(files) => Answered::Described {
              version: version.clone(),
              files,
            },
            Err(why) => Answered::Refused {
              why: format!("version {version} of the app {tool}: {why}"),
            },
          },
          Asked::Point { tool, version } => point(&owned, tool, version),
          Asked::Versions {} => versions(&owned),
          Asked::Take { tool } => take(&owned, tool),
          Asked::File {
            tool,
            version,
            path,
          } => screen_file(&owned.versions(tool).join(version), path),
          Asked::Source { tool, version } => kept_source(&owned, tool, version),
          Asked::Receive {
            tool,
            files,
            hash,
            app,
          } => receive(&owned, tool, files, hash, app),
          Asked::Adopt { tool } => adopt(&owned, tool),
        },
      }
    }
  };

  let _ = output.write_all(answered.line().as_bytes()).await;
  let _ = output.flush().await;
}
