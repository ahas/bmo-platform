//! The runner's keeping of personal apps (docs/person-nodes.md, 「Personal apps」): an app's folder of 'my apps'
//! placed as a new version, copied whole or built confined, its description answered; a version described and
//! pointed at, three older ones kept and the rest taken away; what each tool keeps listed; and a tool taken away, its
//! source and data left standing; a version's source kept and answered, a shared app's source received once its hash
//! is the one given and never over the person's own app, and a received app adopted. A refusal names what failed and
//! leaves nothing staged.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use serde_json::{Value, json};

/// A process the test started, killed and waited on as the test ends.
struct Started(Child);

impl Drop for Started {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

// A stand-in for bubblewrap: the build after `--` run unconfined in the folder bound at /src, with `BMO_TOOL_OUT` the
// folder bound at /out, so the keeping is tried where no namespace can be made.
const FAKE_BWRAP: &str = r#"#!/usr/bin/env python3
import os, sys
args = sys.argv[1:]
source = out = None
for at, word in enumerate(args):
    if word == "--ro-bind" and args[at + 2] == "/src":
        source = args[at + 1]
    if word == "--bind" and args[at + 2] == "/out":
        out = args[at + 1]
    if word == "--":
        command = args[at + 1:]
        break
os.chdir(source)
os.environ["BMO_TOOL_OUT"] = out
os.execvp(command[0], command)
"#;

struct Machine {
  dir: tempfile::TempDir,
  bwrap: PathBuf,
}

impl Machine {
  fn new() -> Self {
    let dir = tempfile::tempdir().unwrap();
    let bwrap = dir.path().join("fake-bwrap");

    fs::write(&bwrap, FAKE_BWRAP).unwrap();
    fs::set_permissions(&bwrap, fs::Permissions::from_mode(0o755)).unwrap();
    Self { dir, bwrap }
  }

  fn root(&self) -> PathBuf {
    self.dir.path().join("data").join(node_link::runner::PERSONAL_TOOLS)
  }

  // An app's folder of 'my apps', its manifest naming `name`, with what `more` adds to its `[tool]` and a git folder.
  fn app(&self, folder: &str, name: &str, more: &str) -> PathBuf {
    let dir = self.dir.path().join("data").join(node_link::runner::MY_APPS).join(folder);

    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::write(dir.join(".git").join("HEAD"), "ref: refs/heads/main").unwrap();
    fs::write(
      dir.join("tool.toml"),
      format!(
        "[tool]\nname = \"{name}\"\nprotocol = 1\nrun = [\"python3\", \"main.py\"]\n{more}\n[[entry]]\nnamespace = \"my\"\nname = \"{name}\"\ndescription = \"Sums up the week.\"\ninput = \"input.json\"\nmanual = \"docs/manual.md\"\ncapabilities = [\"my.own\"]\n"
      ),
    )
    .unwrap();
    fs::write(dir.join("input.json"), r#"{ "type": "object" }"#).unwrap();
    fs::create_dir_all(dir.join("docs")).unwrap();
    fs::write(dir.join("docs").join("manual.md"), "Call it with nothing.").unwrap();
    fs::write(dir.join("main.py"), "print('{}')").unwrap();
    dir
  }

  // One asking of the runner for `owner`, every line it answered.
  fn ask(&self, owner: &str, asked: Value) -> Vec<Value> {
    let mut command = Command::new(crate::support::binary::path("node-link"));

    command.args(["runner", "--root"]).arg(self.root()).args(["--owner", "a1"]);
    command.env(node_link::runner::BWRAP_VARIABLE, &self.bwrap);
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());

    let mut child = Started(command.spawn().expect("the runner"));
    let mut input = child.0.stdin.take().unwrap();
    let mut line = asked;

    line["owner"] = json!(owner);
    input.write_all(format!("{line}\n").as_bytes()).unwrap();

    let mut answered = Vec::new();
    let mut lines = BufReader::new(child.0.stdout.take().unwrap());

    loop {
      let mut read = String::new();

      if lines.read_line(&mut read).unwrap_or(0) == 0 {
        break;
      }

      let value: Value = serde_json::from_str(&read).unwrap();
      let last = value["t"] != "said";

      answered.push(value);

      if last {
        break;
      }
    }

    drop(input);
    answered
  }

  fn last(&self, asked: Value) -> Value {
    self.ask("a1", asked).pop().unwrap()
  }

  fn versions_dir(&self, tool: &str) -> PathBuf {
    self.root().join("owners").join("a1").join("tools").join(tool)
  }

  fn standing(&self, tool: &str) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(self.versions_dir(tool))
      .into_iter()
      .flatten()
      .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
      .collect();

    found.sort();
    found
  }
}

fn listed(dir: &Path) -> Vec<String> {
  let mut found: Vec<String> = fs::read_dir(dir)
    .unwrap()
    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
    .collect();

  found.sort();
  found
}

// "an app's folder is placed whole as a new version, its .git left out, pointed at by nothing until it is pointed
// at, and its description answered; a point keeps three older versions and takes the rest away"
#[test]
fn a_place_copies_the_folder_into_a_version_and_a_point_keeps_three_older_ones() {
  let machine = Machine::new();

  machine.app("weekly", "weekly", "");

  let placed = machine.last(json!({ "place": { "tool": "weekly" } }));

  assert_eq!(placed["t"], "placed", "{placed}");

  let first = placed["version"].as_str().unwrap().to_owned();
  let files = placed["files"].as_object().unwrap();
  let mut paths: Vec<&String> = files.keys().collect();

  paths.sort();
  assert_eq!(paths, ["docs/manual.md", "input.json", "tool.toml"]);
  assert_eq!(files["docs/manual.md"], "Call it with nothing.");
  assert_eq!(
    listed(&machine.versions_dir("weekly").join(&first)),
    [".bmo-source.json", "docs", "input.json", "main.py", "tool.toml"]
  );

  let kept = machine.last(json!({ "versions": {} }));

  assert_eq!(kept["tools"]["weekly"], json!({ "current": null, "kept": [first] }));

  let pointed = machine.last(json!({ "point": { "tool": "weekly", "version": first } }));

  assert_eq!(pointed, json!({ "t": "pointed", "version": first, "kept": [] }));

  let pointer: Value = serde_json::from_str(
    &fs::read_to_string(machine.root().join("owners/a1/registry/weekly.json")).unwrap(),
  )
  .unwrap();

  assert_eq!(pointer["version"], first);
  assert_eq!(pointer["owner"], "a1");

  // Four more versions, each pointed at: the newest three older ones are kept.
  let mut versions = vec![first.clone()];

  for _ in 0..4 {
    std::thread::sleep(std::time::Duration::from_millis(3));

    let placed = machine.last(json!({ "place": { "tool": "weekly" } }));
    let version = placed["version"].as_str().unwrap().to_owned();

    machine.last(json!({ "point": { "tool": "weekly", "version": version } }));
    versions.push(version);
  }

  let kept = machine.last(json!({ "versions": {} }));

  assert_eq!(kept["tools"]["weekly"]["current"], versions[4]);
  assert_eq!(kept["tools"]["weekly"]["kept"], json!([versions[3], versions[2], versions[1]]));
  assert_eq!(machine.standing("weekly"), versions[1..].to_vec(), "the oldest is taken away");

  // A kept version is described and pointed at again.
  let described = machine.last(json!({ "describe": { "tool": "weekly", "version": versions[2] } }));

  assert_eq!(described["t"], "described");
  assert_eq!(described["files"]["input.json"], r#"{ "type": "object" }"#);

  let back = machine.last(json!({ "point": { "tool": "weekly", "version": versions[2] } }));

  assert_eq!(back["kept"], json!([versions[1]]), "{back}");
  assert_eq!(
    machine.standing("weekly"),
    [versions[1].clone(), versions[2].clone()],
    "newer ones are taken away"
  );
}

// "a build runs with the app's folder read at /src and the new version written at /out, the manifest and the files
// it names copied beside what it wrote; a build that fails is refused with what it said, and nothing is staged"
#[test]
fn a_build_writes_the_version_and_a_failed_one_is_refused_with_its_lines() {
  let machine = Machine::new();

  machine.app("built", "built", "build = 'echo building >&2 && cp main.py \"$BMO_TOOL_OUT/run.py\"'");

  let answered = machine.ask("a1", json!({ "place": { "tool": "built" } }));
  let placed = answered.last().unwrap();

  assert_eq!(placed["t"], "placed", "{answered:?}");
  assert_eq!(answered[0], json!({ "t": "said", "line": "building" }));
  assert_eq!(
    listed(&machine.versions_dir("built").join(placed["version"].as_str().unwrap())),
    [".bmo-source.json", "docs", "input.json", "run.py", "tool.toml"],
    "the source's own main.py stays out, kept as the version's source alone"
  );

  machine.app("broken", "broken", "build = 'echo it broke >&2; exit 3'");

  let refused = machine.last(json!({ "place": { "tool": "broken" } }));

  assert_eq!(refused["t"], "refused");
  assert!(refused["why"].as_str().unwrap().contains("build failed: it broke"), "{refused}");
  assert!(machine.standing("broken").is_empty(), "nothing is left staged");
}

// "a manifest not naming its folder, another account, a name that climbs and a version not placed are refused; a take
// leaves the source and the data standing"
#[test]
fn the_keeping_refuses_what_it_does_not_keep_and_a_take_leaves_source_and_data() {
  let machine = Machine::new();
  let source = machine.app("weekly", "monthly", "");
  let refused = |answered: Value| answered["why"].as_str().unwrap_or_default().to_owned();

  assert!(
    refused(machine.last(json!({ "place": { "tool": "weekly" } }))).contains("not its folder's name")
  );
  assert!(
    refused(machine.ask("b2", json!({ "versions": {} })).pop().unwrap())
      .contains("keeps no app of the account b2")
  );
  assert!(refused(machine.last(json!({ "place": { "tool": "../x" } }))).contains("is no tool's name"));
  assert!(
    refused(machine.last(json!({ "point": { "tool": "weekly", "version": "v9" } })))
      .contains("is not placed")
  );

  machine.app("weekly", "weekly", "");

  let placed = machine.last(json!({ "place": { "tool": "weekly" } }));

  machine.last(json!({ "point": { "tool": "weekly", "version": placed["version"] } }));

  let data = node_link::runner::data_dir(&machine.root(), "a1", "weekly");

  fs::create_dir_all(&data).unwrap();
  fs::write(data.join("notes.txt"), "kept").unwrap();

  assert_eq!(machine.last(json!({ "take": { "tool": "weekly" } })), json!({ "t": "taken" }));
  assert!(!machine.versions_dir("weekly").exists());
  assert!(!machine.root().join("owners/a1/registry/weekly.json").exists());
  assert!(source.join("tool.toml").exists(), "the source stands");
  assert_eq!(fs::read_to_string(data.join("notes.txt")).unwrap(), "kept", "the data stands");
  assert!(refused(machine.last(json!({ "take": { "tool": "weekly" } }))).contains("keeps no app weekly"));
}

// "a place's build sees the screen kit read alone at /kit and the runtime at /runtime/bun, first on its path, where
// the runner is told a kit, and neither where it is not"
#[test]
fn a_build_sees_the_screen_kit_and_the_runtime_where_a_kit_is_told() {
  let (source, out) = (Path::new("/apps/notes"), Path::new("/versions/staged"));
  let (kit, runtime) = (Path::new("/workplace/deploy/screen-kit"), Path::new("/workplace/bun/bin/bun"));
  let words = node_link::keeping::build_words(source, out, "bun /kit/build.mjs", Some((kit, runtime)));
  let bound = |from: &Path, at: &str| {
    words.windows(3).any(|three| three == ["--ro-bind", &from.to_string_lossy(), at])
  };

  assert!(bound(kit, "/kit") && bound(runtime, "/runtime/bun"), "{words:?}");
  assert!(
    words
      .windows(3)
      .any(|three| three[0] == "--setenv" && three[1] == "PATH" && three[2].starts_with("/runtime:")),
    "{words:?}"
  );

  let plain = node_link::keeping::build_words(source, out, "true", None);

  assert!(!plain.iter().any(|word| word == "/kit" || word.starts_with("/runtime")), "{plain:?}");
}

// "a version whose manifest names a screen is given the runner's index of its screen's files, its description carries
// the screen's words, layer and index, a file of its screen is answered in base64 and a path climbing out of it is
// refused; a screen holding no module is refused and nothing is left staged"
#[test]
fn a_screen_is_indexed_as_it_is_placed_and_its_files_are_read_alone() {
  let machine = Machine::new();
  let build = "build = 'cp main.py \"$BMO_TOOL_OUT/\" && mkdir -p \"$BMO_TOOL_OUT/screen/parts\" && printf module > \"$BMO_TOOL_OUT/screen/module.js\" && printf chunk > \"$BMO_TOOL_OUT/screen/parts/chunk.js\" && printf {} > \"$BMO_TOOL_OUT/screen/layer.json\" && printf {} > \"$BMO_TOOL_OUT/screen/words.json\" && printf forged > \"$BMO_TOOL_OUT/screen/files.json\"'\n[screen]\n";

  machine.app("notes", "notes", build);

  let placed = machine.last(json!({ "place": { "tool": "notes" } }));

  assert_eq!(placed["t"], "placed", "{placed}");

  let version = placed["version"].as_str().unwrap();
  let index: Value = serde_json::from_str(placed["files"]["screen/files.json"].as_str().unwrap()).unwrap();
  let sha = |text: &str| {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(text.as_bytes()))
  };

  assert_eq!(
    index,
    json!({ "layer.json": sha("{}"), "module.js": sha("module"), "parts/chunk.js": sha("chunk"), "words.json": sha("{}") })
  );
  assert_eq!(placed["files"]["screen/words.json"], "{}");
  assert!(placed["files"].get("screen/module.js").is_none(), "{placed}");

  let read =
    machine.last(json!({ "file": { "tool": "notes", "version": version, "path": "parts/chunk.js" } }));

  assert_eq!(read, json!({ "t": "file", "bytes": "Y2h1bms=" }));

  let climbing =
    machine.last(json!({ "file": { "tool": "notes", "version": version, "path": "../tool.toml" } }));

  assert_eq!(climbing["t"], "refused", "{climbing}");

  let described = machine.last(json!({ "describe": { "tool": "notes", "version": version } }));

  assert_eq!(described["files"]["screen/files.json"], placed["files"]["screen/files.json"]);

  machine.app("bare", "bare", "build = 'mkdir -p \"$BMO_TOOL_OUT/screen\" && printf {} > \"$BMO_TOOL_OUT/screen/words.json\"'\n[screen]\n");

  let refused = machine.last(json!({ "place": { "tool": "bare" } }));

  assert_eq!(refused["t"], "refused");
  assert!(refused["why"].as_str().unwrap().contains("no screen/module.js"), "{refused}");
  assert!(machine.standing("bare").is_empty(), "nothing is left staged");
}

// "a place of a folder that is a git repository of its own keeps what the repository keeps or would keep, so a test
// run's caches its ignore rules leave out stay out of the source"
#[test]
fn a_version_keeps_the_source_its_repository_keeps() {
  let machine = Machine::new();
  let dir = machine.app("cached", "cached", "");
  let git = |words: &[&str]| {
    let ran = std::process::Command::new("git").arg("-C").arg(&dir).args(words).output().unwrap();

    assert!(ran.status.success(), "git {words:?}: {}", String::from_utf8_lossy(&ran.stderr));
  };

  fs::remove_dir_all(dir.join(".git")).unwrap();
  git(&["init", "-q"]);
  fs::write(dir.join(".gitignore"), "__pycache__/\n").unwrap();
  git(&["add", "-A"]);
  git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "app"]);
  fs::create_dir_all(dir.join("__pycache__")).unwrap();
  fs::write(dir.join("__pycache__").join("main.cpython-312.pyc"), [0xff_u8, 0xfe, 0x00]).unwrap();
  fs::write(dir.join("notes.md"), "untracked, not ignored").unwrap();

  let placed = machine.last(json!({ "place": { "tool": "cached" } }));
  let version = placed["version"].as_str().unwrap().to_owned();
  let source = machine.last(json!({ "source": { "tool": "cached", "version": version } }));

  assert_eq!(source["t"], "source", "{source}");

  let mut paths: Vec<&String> = source["files"].as_object().unwrap().keys().collect();

  paths.sort();
  assert_eq!(
    paths,
    [".gitignore", "docs/manual.md", "input.json", "main.py", "notes.md", "tool.toml"]
  );
}

// "a place keeps the source it was built from, which a source answers, its .git left out; a receive writes a shared
// app's source into its folder where its hash is the one given, marked as a copy, replaces a copy of the same app, and
// never writes over the person's own app or another app's copy; an adoption takes the mark away"
#[test]
fn a_version_keeps_its_source_and_a_shared_one_is_received_checked_and_adopted() {
  let machine = Machine::new();

  machine.app("weekly", "weekly", "build = 'cp main.py \"$BMO_TOOL_OUT/run.py\"'");

  let placed = machine.last(json!({ "place": { "tool": "weekly" } }));
  let version = placed["version"].as_str().unwrap().to_owned();
  let source = machine.last(json!({ "source": { "tool": "weekly", "version": version } }));

  assert_eq!(source["t"], "source", "{source}");

  let files = source["files"].as_object().unwrap();
  let mut paths: Vec<&String> = files.keys().collect();

  paths.sort();
  assert_eq!(paths, ["docs/manual.md", "input.json", "main.py", "tool.toml"], "its .git is left out");
  assert!(
    machine.last(json!({ "source": { "tool": "weekly", "version": "v9" } }))["why"]
      .as_str()
      .unwrap()
      .contains("keeps no source")
  );

  // The shared source, as the catalogue would hand it, with the hash it records.
  let shared: std::collections::BTreeMap<String, String> =
    serde_json::from_value(source["files"].clone()).unwrap();
  let hash = node_link::keeping::source_hash(&shared);
  let receive = |tool: &str, hash: &str, app: &str| {
    machine.last(json!({ "receive": { "tool": tool, "files": shared, "hash": hash, "app": app } }))
  };

  assert!(
    receive("received", "0000", "17-weekly")["why"]
      .as_str()
      .unwrap()
      .contains("does not match the hash")
  );
  assert_eq!(receive("received", &hash, "17-weekly"), json!({ "t": "received" }));

  let folder = machine.dir.path().join("data").join(node_link::runner::MY_APPS).join("received");

  assert!(folder.join("main.py").exists());
  assert!(folder.join(node_link::keeping::RECEIVED_MARK).exists());

  // Received again, as an update, it replaces the copy; never the person's own app, nor another app's copy.
  assert_eq!(receive("received", &hash, "17-weekly"), json!({ "t": "received" }));
  assert!(
    receive("weekly", &hash, "17-weekly")["why"]
      .as_str()
      .unwrap()
      .contains("no copy of 17-weekly")
  );
  assert!(
    receive("received", &hash, "21-other")["why"]
      .as_str()
      .unwrap()
      .contains("no copy of 21-other")
  );

  // An empty folder of the name holds no app, and is written over.
  let empty = machine.dir.path().join("data").join(node_link::runner::MY_APPS).join("empty");

  std::fs::create_dir_all(&empty).unwrap();
  assert_eq!(receive("empty", &hash, "17-weekly"), json!({ "t": "received" }));
  assert!(empty.join("main.py").exists());

  // An adoption makes the copy the person's own; their own app is no copy to adopt.
  assert_eq!(machine.last(json!({ "adopt": { "tool": "received" } })), json!({ "t": "adopted" }));
  assert!(!folder.join(node_link::keeping::RECEIVED_MARK).exists());
  assert!(
    machine.last(json!({ "adopt": { "tool": "weekly" } }))["why"]
      .as_str()
      .unwrap()
      .contains("no received copy")
  );
}
