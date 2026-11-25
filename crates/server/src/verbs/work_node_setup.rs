//! The workplace on a work node, stood up and read again (docs/work-nodes.md, 「The workplace on a node」):
//! each step checked before the next, and everything still missing said one thing per line, so a node
//! with anything missing takes no work. The steps are this module's; the node, the store and the
//! machine's own roots are handed in, so a test runs them against a shell of its own.

use futures_util::future::BoxFuture;

use policy::providers::claude_code::CLAUDE_CODE;

use crate::watches::links::{node_path, own_claude, path_head};

/// A word as a POSIX shell reads it back as itself.
pub fn shell_word(word: &str) -> String {
  let plain =
    !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c));

  if plain { word.to_owned() } else { format!("'{}'", word.replace('\'', r#"'"'"'"#)) }
}

/// What a command run on the node came to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellRun {
  pub code: i32,
  pub stdout: String,
  pub stderr: String,
}

/// The code of a command that came to no code of its own, its errors saying why: one that never ran, or whose end
/// was never seen. No program ends with it, where a launcher on Windows may end with any other negative code.
pub const NO_CODE: i32 = i32::MIN;

impl ShellRun {
  /// Why a command that failed failed, as the node said it: the code it ended with and what it wrote on its
  /// errors, or on its output where its errors hold nothing, since a launcher that fails before the command runs,
  /// as WSL's does, may write why there alone; a command that came to no code is its errors alone.
  pub fn said(&self) -> String {
    let words = [self.stderr.trim(), self.stdout.trim()]
      .into_iter()
      .find(|words| !words.is_empty())
      .unwrap_or("nothing was written on its output or its errors");

    if self.code == NO_CODE {
      words.to_owned()
    } else {
      format!("it ended with code {}: {words}", self.code)
    }
  }
}

/// The node's shell the setup runs its commands in.
pub trait SetupShell: Sync {
  fn run<'a>(&'a self, command: &'a str) -> BoxFuture<'a, ShellRun>;
}

/// One account's login as this machine keeps it — its own configuration directory, or the machine's for
/// a token account — whether this machine holds its token for the work nodes, and the command that
/// makes one.
#[derive(Clone, Debug)]
pub struct SetupLogin {
  pub account: String,
  /// Whether the path is the account's own configuration directory rather than the machine's.
  pub own_directory: bool,
  pub path: String,
  pub node_token: bool,
  pub token_command: String,
}

/// A checkout placed on the node, with the remote its base clones from.
#[derive(Clone, Debug)]
pub struct SetupCheckout {
  pub worker: String,
  pub cwd: String,
  pub remote: Option<String>,
}

/// What the setup is handed about where it works.
pub struct SetupPlace<'a> {
  pub node: String,
  /// The workplace's root as written, which may be under the node's home.
  pub root: String,
  /// This machine's deploy tree, whose commit is copied to the node.
  pub deploy: String,
  pub checkouts: Vec<SetupCheckout>,
  pub logins: Vec<SetupLogin>,
  /// A path of this machine as the node reads it, once its home and the workplace's root are known.
  pub to_node: &'a (dyn Fn(&str, &str, &str) -> String + Sync),
  /// This machine's herdr, which a node without one is given.
  pub herdr: Option<String>,
  /// This machine's Rust version, which the node's build takes.
  pub rust: Option<String>,
  /// This machine's bun version, which the node's must be: bun reads the repository's own configuration
  /// and lockfile, which another bun may not.
  pub bun: Option<String>,
  /// The Claude Code release this machine runs, which the workplace's own copy is laid at; none where this
  /// machine runs none, and the copy standing is left as it is.
  pub claude: Option<String>,
  /// The herdr session the node's agents stand in, apart from any session a person keeps there.
  pub session: String,
  /// Where the link's far end stands on the node as a service of the account's own, listening at the node's
  /// loopback, rather than being started by the link's own connection: a person's PC (docs/person-nodes.md).
  pub far_end: Option<FarEnd>,
}

/// The ports a far end standing as a service listens on at the node's loopback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FarEnd {
  pub mirror_port: u16,
  pub herdr_port: u16,
}

/// The user service the link's far end stands as on a node that keeps it standing itself.
pub const FAR_END_UNIT: &str = "bmo-node-link.service";

/// What each step may do beyond reading: install a missing tool, copy the code, build, clone.
pub trait SetupHands: Sync {
  fn install(&self) -> bool;
  /// Whether the code is copied and built, or only found there: a check reads, a setup and a sync write.
  fn copy_code(&self) -> bool;
  /// A file or a tree of this machine, the tree's path ending in a slash, written at a path of the node.
  fn copy<'a>(&'a self, from: &'a str, to: &'a str) -> BoxFuture<'a, (bool, String)>;
  /// The node saved as a machine of this herdr, or nothing done where herdr does not reach it.
  fn add_machine(&self) -> BoxFuture<'_, (bool, String)>;
  fn say(&self, line: &str);
}

/// What a setup came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupOutcome {
  pub home: Option<String>,
  pub root: Option<String>,
  pub missing: Vec<String>,
  pub herdr_socket: Option<String>,
  /// Whether this run laid the deploy tree's copy over the node's, so the node now holds the deployed commit.
  pub copied: bool,
  /// On a person's PC, why its personal tools cannot run confined, where they cannot: bubblewrap not on the PC, or
  /// one that makes no namespace there (docs/person-nodes.md, 「Personal tools on a PC」). None where they can, and on
  /// every other node.
  pub confinement: Option<String>,
  /// The release the workplace's own Claude Code reports, where it stands and runs (docs/work-nodes.md, 「The
  /// workplace on a node」); none where it does not.
  pub claude: Option<String>,
}

/// The crates a node builds its own binaries from, each by its folder in the repository, which it keeps in
/// the workspace `crates/node-link/node-build.toml` makes (docs/work-nodes.md, 「The workplace on a node」).
pub const NODE_CRATES: [&str; 4] =
  ["crates/agent-host", "crates/store", "crates/pane-head", "crates/node-link"];

/// What the deploy tree's copy on a node leaves out: what is built or installed there, and what no job on
/// a node reads.
pub const COPY_LEFT_OUT: [&str; 10] = [
  "/.git",
  "node_modules/",
  "/target/",
  "/bin/",
  "/android/",
  "/client/.output/",
  "/client/.nuxt/",
  "/data/",
  "/.env*",
  "*.log",
];

/// Where the deploy tree's Bun workspace stands, which a node installs from its own lockfile.
pub const BUN_WORKSPACE: &str = "packages";

// The binaries a node builds for itself.
const NODE_BINARIES: [&str; 3] = ["pane-head", "agent-host", "node-link"];

// The search path every command of the setup runs with.
fn with_path(root: &str, command: &str) -> String {
  format!(
    r#"export PATH={}:"$HOME/.bun/bin:$HOME/.local/bin:$HOME/.cargo/bin:$PATH"; {command}"#,
    shell_word(&join(root, "bun/bin"))
  )
}

// An installer fetched into a file and run from it with nothing on its standing input: run as `curl … |
// bash`, a program it starts inherits the pipe the script came through and may wait on it.
fn installer(url: &str, args: &str) -> String {
  let name: String = url.chars().map(|one| if one.is_ascii_alphanumeric() { one } else { '-' }).collect();
  let script = format!("/tmp/bmo-install-{name}.sh");

  format!(
    "curl -fsSL {} -o {} && bash {} {args} </dev/null; status=$?; rm -f {}; exit $status",
    shell_word(url),
    shell_word(&script),
    shell_word(&script),
    shell_word(&script)
  )
}

// One tool a workplace needs: how it is installed into the account where it is missing — none where it
// is the machine's to have — and how its version is read where it must be this machine's.
struct Tool {
  name: &'static str,
  install: Option<String>,
  version: Option<(&'static str, String)>,
}

fn tools(bun: Option<&str>, root: &str) -> Vec<Tool> {
  // The runtime of this machine's version, installed into the workplace with a home of its own, so neither
  // the account's runtime nor its shell files are touched.
  let own = join(root, "bun");
  let bun_install = format!(
    "mkdir -p {home} && export BUN_INSTALL={own} HOME={home} && {}",
    installer("https://bun.sh/install", &bun.map(|bun| format!("bun-v{bun}")).unwrap_or_default()),
    own = shell_word(&own),
    home = shell_word(&join(&own, "home")),
  );

  vec![
    Tool {
      name: "git",
      install: None,
      version: None,
    },
    Tool {
      name: "rsync",
      install: None,
      version: None,
    },
    Tool {
      name: "curl",
      install: None,
      version: None,
    },
    Tool {
      name: "bun",
      install: Some(bun_install),
      version: bun.map(|bun| ("bun --version", bun.to_owned())),
    },
    Tool {
      name: "cargo",
      install: Some(installer("https://sh.rustup.rs", "-y --profile minimal")),
      version: None,
    },
  ]
}

/// The program that lays the workplace's own Claude Code on the node, run by the workplace's runtime: the release
/// named, fetched from the release downloads for the node's processor and C library, checked against the checksum
/// its manifest names, and renamed over the path, so a program running the one before keeps it. The manifest is read
/// by the runtime's JSON reader.
const LAY_CLAUDE: &str = r#"
const fs = require("node:fs");
const { BMO_CLAUDE_DOWNLOADS: base, BMO_CLAUDE_RELEASE: release, BMO_CLAUDE_TO: to } = process.env;
const arch = { x64: "x64", arm64: "arm64" }[process.arch];
if (!arch) throw new Error(`Claude Code has no build for the processor ${process.arch}`);
const musl = ["/lib/libc.musl-x86_64.so.1", "/lib/libc.musl-aarch64.so.1"].some((one) => fs.existsSync(one));
const platform = `linux-${arch}${musl ? "-musl" : ""}`;
const asked = await fetch(`${base}/${release}/manifest.json`);
if (!asked.ok) throw new Error(`the manifest of Claude Code ${release} could not be fetched: ${asked.status}`);
const checksum = (await asked.json()).platforms?.[platform]?.checksum;
if (!checksum) throw new Error(`Claude Code ${release} names no build for ${platform}`);
const got = await fetch(`${base}/${release}/${platform}/claude`);
if (!got.ok) throw new Error(`Claude Code ${release} for ${platform} could not be fetched: ${got.status}`);
const bytes = new Uint8Array(await got.arrayBuffer());
if (new Bun.CryptoHasher("sha256").update(bytes).digest("hex") !== checksum) throw new Error(`Claude Code ${release} for ${platform} does not match its checksum`);
fs.mkdirSync(require("node:path").dirname(to), { recursive: true });
fs.writeFileSync(`${to}.new`, bytes, { mode: 0o755 });
fs.renameSync(`${to}.new`, to);
"#;

/// The command that lays the workplace's own Claude Code at `to`, at `release` (docs/work-nodes.md, 「The workplace
/// on a node」).
pub fn lay_claude(to: &str, release: &str) -> String {
  format!(
    "BMO_CLAUDE_DOWNLOADS={} BMO_CLAUDE_RELEASE={} BMO_CLAUDE_TO={} bun -e {}",
    shell_word(&CLAUDE_CODE.workplace_copy.downloads),
    shell_word(release),
    shell_word(to),
    shell_word(LAY_CLAUDE.trim())
  )
}

/// The release a Claude Code reports: the first word of its version line, `2.1.0` of `2.1.0 (Claude Code)`.
pub fn claude_release_of(said: &str) -> Option<String> {
  said.split_whitespace().next().map(str::to_owned)
}

// The release the workplace's own Claude Code at `path` reports, or none where it does not stand or does not run.
async fn claude_release(shell: &dyn SetupShell, root: &str, path: &str) -> Option<String> {
  let path = shell_word(path);
  let ran = shell.run(&with_path(root, &format!("test -x {path} && {path} --version"))).await;

  (ran.code == 0).then(|| claude_release_of(&ran.stdout).unwrap_or_default())
}

/// The herdr session's socket as the node's herdr lists it, or none where it lists none.
pub fn session_socket(listing: &str, session: &str) -> Option<String> {
  listing.lines().find_map(|line| {
    let words: Vec<&str> = line.split_whitespace().collect();

    (words.first() == Some(&session) && words.len() >= 4).then(|| words[words.len() - 1].to_owned())
  })
}

fn last_lines(text: &str, count: usize) -> String {
  let lines: Vec<&str> = text.trim().split('\n').collect();

  lines[lines.len().saturating_sub(count)..].join(" ")
}

fn join(base: &str, part: &str) -> String {
  std::path::Path::new(base).join(part).to_string_lossy().into_owned()
}

// A command whose connection failed is a step that failed, said with the rest, never the end of the
// setup.
async fn found(shell: &dyn SetupShell, root: &str, tool: &str) -> bool {
  looked(shell, root, tool).await.is_ok_and(|standing| standing)
}

/// Whether a tool stands on the node's search path, or why it could not be looked for (docs/work-nodes.md, 「The
/// workplace on a node」): `command -v` ends with 0 for a tool it finds and 1 for one it does not, so any other end
/// — a step that came to no code, a launcher's own code, a shell that could not run — is a look the node did not
/// answer, never a tool missing.
pub async fn looked(shell: &dyn SetupShell, root: &str, tool: &str) -> Result<bool, String> {
  let ran = shell
    .run(&with_path(root, &format!("command -v {} >/dev/null", shell_word(tool))))
    .await;

  match ran.code {
    0 => Ok(true),
    1 => Ok(false),
    _ => Err(ran.said()),
  }
}

// A word of a unit file's command line as systemd reads it back as itself.
fn unit_word(word: &str) -> String {
  format!(
    "\"{}\"",
    word.replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%").replace('$', "$$")
  )
}

/// The unit file of a far end standing as a service: the link's serve program on the workplace, listening at
/// the node's loopback, started again whenever it ends; stopping it stops it alone, so the herdr session and
/// the agent host it started stay.
pub fn far_end_unit(home: &str, root: &str, session: &str, socket: &str, ports: &FarEnd) -> String {
  let command = [
    join(root, "deploy/bin/node-link"),
    "serve".into(),
    "--root".into(),
    root.into(),
    "--herdr-session".into(),
    session.into(),
    "--herdr-socket".into(),
    socket.into(),
    "--listen".into(),
    format!("127.0.0.1:{}", ports.mirror_port),
    "--herdr-listen".into(),
    format!("127.0.0.1:{}", ports.herdr_port),
  ]
  .iter()
  .map(|word| unit_word(word))
  .collect::<Vec<_>>()
  .join(" ");
  let mut path = path_head(home, root);

  path.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(str::to_owned));

  // An assignment's value is taken as written but for its specifiers.
  let path = path.join(":").replace('%', "%%");

  format!(
    "[Unit]\nDescription=The far end of agency's link to this machine\n\n[Service]\nExecStart={command}\nEnvironment=\"PATH={path}\"\nRestart=always\nRestartSec=5\nKillMode=process\n\n[Install]\nWantedBy=default.target\n"
  )
}

// The configuration file a login keeps on the node: its own directory's, read as the node reads it, or the
// home's for a login on the machine's own directory.
fn login_file(place: &SetupPlace<'_>, login: &SetupLogin, home: &str, root: &str) -> String {
  if login.own_directory {
    join(&(place.to_node)(&login.path, home, root), ".claude.json")
  } else {
    join(home, ".claude.json")
  }
}

/// Every step of the setup in order. A step that cannot be met is said and the rest still run where they
/// can, so one pass names everything missing; a node whose login fails names that alone.
pub async fn set_up_workplace(
  shell: &dyn SetupShell,
  place: &SetupPlace<'_>,
  hands: &dyn SetupHands,
) -> SetupOutcome {
  let mut missing = Vec::new();
  let mut copied = false;
  let home_run = shell.run(r#"printf '%s\n' "$HOME""#).await;
  let home = home_run.stdout.trim().to_owned();

  if home_run.code != 0 || !home.starts_with('/') {
    let said = home_run.stderr.trim().split('\n').next_back().filter(|line| !line.is_empty());

    return SetupOutcome {
      home: None,
      root: None,
      missing: vec![format!("login: {}", said.unwrap_or("the node did not answer"))],
      herdr_socket: None,
      copied: false,
      confinement: None,
      claude: None,
    };
  }

  let root = node_path(&place.root, Some(&home));
  let deploy = join(&root, "deploy");
  let data = join(&root, "data");
  let node = &place.node;

  hands.say(&format!("{node}: home {home}, workplace {root}"));

  // The tools, each installed into the account where it is missing and the setup may install.
  for tool in tools(place.bun.as_deref(), &root) {
    let standing = match looked(shell, &root, tool.name).await {
      Ok(standing) => standing,
      Err(why) => {
        missing.push(format!("{}: could not be looked for on the node: {why}", tool.name));
        continue;
      }
    };
    let version = match (&tool.version, standing) {
      (Some((ask, _)), true) => Some(shell.run(&with_path(&root, ask)).await.stdout.trim().to_owned()),
      _ => None,
    };
    let wanted = tool.version.as_ref().map(|(_, want)| want.clone());

    if standing && (wanted.is_none() || version == wanted) {
      continue;
    }

    if standing && !hands.install() {
      missing.push(format!(
        "{}: {} on the node, where this machine runs {}; run the setup with --install",
        tool.name,
        version.unwrap_or_default(),
        wanted.unwrap_or_default()
      ));
      continue;
    }

    if hands.install()
      && let Some(install) = &tool.install
    {
      hands.say(&format!("{node}: installing {}", tool.name));

      let installed = shell.run(&with_path(&root, install)).await;
      let now = match &tool.version {
        Some((ask, _)) => Some(shell.run(&with_path(&root, ask)).await.stdout.trim().to_owned()),
        None => None,
      };

      if installed.code == 0 && found(shell, &root, tool.name).await && (wanted.is_none() || now == wanted)
      {
        continue;
      }
    }

    missing.push(match &tool.install {
      None => format!("{}: not on the node, and the machine's to install", tool.name),
      Some(_) if hands.install() => format!("{}: not installed (its installer failed)", tool.name),
      Some(_) => format!("{}: not installed; run the setup with --install", tool.name),
    });
  }

  // A person's PC runs its owner's personal tools confined by bubblewrap, which is the machine's to have: installed
  // with --install where the PC's own package manager may be asked without a password, and tried once it stands.
  let confinement = match place.far_end {
    Some(_) => confinement(shell, &root, hands).await,
    None => None,
  };

  // Bun, and every Claude Code release built on it, needs a processor with AVX: on one without, as a
  // virtual machine on a generic processor type is, it crashes on its first line.
  if shell.run("grep -q -w avx /proc/cpuinfo").await.code != 0 {
    missing.push("cpu: the node's processor offers no AVX, which bun and Claude Code need; a virtual machine takes it with a processor type that passes the host's through".into());
  } else if found(shell, &root, "bun").await
    && !shell.run(&with_path(&root, "bun -e 'console.log(6 * 7)'")).await.stdout.contains("42")
  {
    missing.push("bun: installed on the node, but it does not run there".into());
  }

  // The Rust toolchain, brought to this machine's version through rustup.
  if let Some(rust) = &place.rust {
    // The toolchain is named for the build alone, so the account's own default stays as it was.
    let rustc = shell
      .run(&with_path(&root, &format!("RUSTUP_TOOLCHAIN={} rustc --version", shell_word(rust))))
      .await
      .stdout
      .trim()
      .to_owned();

    if !rustc.starts_with(&format!("rustc {rust} ")) {
      let pinned = if hands.install() {
        let bring = format!(
          "if command -v rustup >/dev/null; then rustup toolchain install {} --profile minimal; else {}; fi",
          shell_word(rust),
          installer(
            "https://sh.rustup.rs",
            &format!("-y --profile minimal --default-toolchain {}", shell_word(rust))
          )
        );

        Some(shell.run(&with_path(&root, &bring)).await)
      } else {
        None
      };

      if pinned.is_none_or(|pinned| pinned.code != 0) {
        missing.push(format!(
          "rust: {} on the node, where this machine builds with {rust}{}",
          if rustc.is_empty() { "none" } else { &rustc },
          if hands.install() {
            " (rustup could not bring it)"
          } else {
            "; run the setup with --install"
          }
        ));
      }
    }
  }

  let folders = [
    deploy.clone(),
    join(&data, "run"),
    join(&data, "agent-places"),
    join(&data, "env"),
    join(&root, "benches"),
  ];

  shell
    .run(&format!(
      "mkdir -p {}",
      folders.iter().map(|folder| shell_word(folder)).collect::<Vec<_>>().join(" ")
    ))
    .await;

  // The code: the deploy tree's commit, installed there, and the node's own binaries built from it.
  if !hands.copy_code() {
    let standing = NODE_BINARIES
      .iter()
      .map(|binary| format!("test -x {}", shell_word(&join(&deploy, &format!("bin/{binary}")))))
      .collect::<Vec<_>>()
      .join(" && ");

    if shell.run(&standing).await.code != 0 {
      missing
        .push("code: the deploy tree's copy or its binaries are not on the node; run the setup".into());
    }
  } else {
    hands.say(&format!("{node}: copying the code"));

    let (laid, said) = hands.copy(&format!("{}/", place.deploy), &format!("{deploy}/")).await;

    copied = laid;

    if !laid {
      missing.push(format!("code: the deploy tree could not be copied: {said}"));
    } else {
      let install = shell
        .run(&with_path(
          &root,
          &format!(
            "cd {} && bun install --frozen-lockfile && cd {} && bun install --frozen-lockfile",
            shell_word(&join(&deploy, BUN_WORKSPACE)),
            shell_word(&join(&deploy, node_link::runner::SCREEN_KIT)),
          ),
        ))
        .await;

      if install.code != 0 {
        missing.push(format!("code: bun install failed: {}", last_lines(&install.stderr, 3)));
      }

      hands.say(&format!("{node}: building pane-head, agent-host and node-link there"));

      // In a workspace of their own beside the code, from the crates they are made of and the
      // repository's lockfile: the whole workspace names a git dependency a node may not reach.
      let build = join(&root, "build");
      let target = join(&root, "cargo-target");
      let toolchain = place
        .rust
        .as_ref()
        .map(|rust| format!("RUSTUP_TOOLCHAIN={} ", shell_word(rust)))
        .unwrap_or_default();
      // Each crate at the path it holds in the repository, so the paths between them read as they do there.
      let parent = |one: &str| one.rsplit_once('/').map_or("", |(above, _)| above).to_owned();
      let copies = NODE_CRATES
        .iter()
        .map(|one| {
          format!(
            "mkdir -p {above} && cp -r {} {above}/",
            shell_word(&join(&deploy, one)),
            above = shell_word(&join(&build, &parent(one)))
          )
        })
        .collect::<Vec<_>>()
        .join(" && ");
      let steps = [
        format!("rm -rf {}", shell_word(&build)),
        format!("mkdir -p {} {}", shell_word(&build), shell_word(&join(&deploy, "bin"))),
        copies,
        format!(
          "cp {} {}",
          shell_word(&join(&deploy, "crates/node-link/node-build.toml")),
          shell_word(&join(&build, "Cargo.toml"))
        ),
        format!(
          "cp {} {}",
          shell_word(&join(&deploy, "Cargo.lock")),
          shell_word(&join(&build, "Cargo.lock"))
        ),
        format!("cd {}", shell_word(&build)),
        format!("{toolchain}CARGO_TARGET_DIR={} cargo build --release -q", shell_word(&target)),
        // Each binary copied beside the one in place and renamed over it, since a binary running there — the
        // far end, the agent host — refuses to be written over but not to be replaced.
        NODE_BINARIES
          .iter()
          .map(|one| {
            let into = join(&deploy, &format!("bin/{one}"));
            let beside = join(&deploy, &format!("bin/.{one}.new"));

            format!(
              "cp {} {beside} && mv -f {beside} {into}",
              shell_word(&join(&target, &format!("release/{one}"))),
              beside = shell_word(&beside),
              into = shell_word(&into),
            )
          })
          .collect::<Vec<_>>()
          .join(" && "),
        // The agent host standing on the old binary ends, and the link's far end starts the new one.
        format!(
          r#"(held=$(grep -o '"pid":[0-9]*' {} 2>/dev/null | cut -d: -f2); if [ -n "$held" ]; then kill "$held" 2>/dev/null; fi; true)"#,
          shell_word(&join(&data, "executions/agent-host/claim"))
        ),
      ];
      let built = shell.run(&with_path(&root, &steps.join(" && "))).await;

      if built.code != 0 {
        missing.push(format!("binaries: the build failed: {}", last_lines(&built.stderr, 3)));
      }
    }
  }

  // The workplace's own Claude Code, laid with every copy at this machine's release under a name no `claude` of the
  // account's takes, and read standing either way.
  let own_claude = own_claude(&root);
  let mut claude = claude_release(shell, &root, &own_claude).await;

  if hands.copy_code()
    && let Some(release) = &place.claude
    && claude.as_ref() != Some(release)
  {
    hands.say(&format!("{node}: laying Claude Code {release} at {own_claude}"));

    let laid = shell.run(&with_path(&root, &lay_claude(&own_claude, release))).await;

    claude = claude_release(shell, &root, &own_claude).await;

    // A release that could not be laid where an earlier one still runs is said, and the earlier one serves on.
    match (laid.code, &claude) {
      (0, Some(_)) => {}
      (_, Some(standing)) => hands.say(&format!(
        "{node}: Claude Code {release} could not be laid, and {standing} serves on: {}",
        last_lines(&laid.said(), 3)
      )),
      (_, None) => missing.push(format!(
        "claude: Claude Code {release} could not be laid at {own_claude}: {}",
        last_lines(&laid.said(), 3)
      )),
    }
  } else if claude.is_none() {
    missing.push(format!(
      "claude: the workplace holds no Claude Code of its own at {own_claude}; run the setup"
    ));
  }

  // herdr, and the session the agents stand in: this machine's herdr given to a node with none, and
  // herdr's own machine setup saving the node here, so herdr's sidebar shows it.
  if hands.copy_code()
    && let Some(herdr) = &place.herdr
    && !found(shell, &root, "herdr").await
  {
    shell.run(r#"mkdir -p "$HOME/.local/bin""#).await;

    let (given, said) = hands.copy(herdr, &join(&home, ".local/bin/herdr")).await;

    if !given {
      hands.say(&format!("{node}: herdr could not be copied: {said}"));
    }
  }

  if hands.copy_code() {
    let (added, said) = hands.add_machine().await;

    if !added && !said.to_lowercase().contains("already") {
      hands.say(&format!("{node}: herdr machine add said: {said}"));
    }
  }

  let mut herdr_socket = None;

  if found(shell, &root, "herdr").await {
    let listing = shell.run(&with_path(&root, "herdr session list </dev/null")).await;

    herdr_socket = session_socket(&listing.stdout, &place.session);

    if herdr_socket.is_none() {
      shell
        .run(&with_path(
          &root,
          &format!(
            "setsid -f herdr --session {} server </dev/null >{} 2>&1; sleep 2",
            shell_word(&place.session),
            shell_word(&join(&data, "herdr-server.log"))
          ),
        ))
        .await;
      herdr_socket = session_socket(
        &shell.run(&with_path(&root, "herdr session list </dev/null")).await.stdout,
        &place.session,
      );
    }

    if herdr_socket.is_none() {
      missing.push(format!("herdr: the session {} does not stand on the node", place.session));
    }
  } else {
    missing.push("herdr: not installed; add the node to herdr with herdr machine add".into());
  }

  // The far end standing as a service of the account's own where the node keeps it: written and started again
  // after a copy, so it runs the binary just built; read as running otherwise.
  if let (Some(ports), Some(socket)) = (&place.far_end, &herdr_socket) {
    let unit_file = join(&home, &format!(".config/systemd/user/{FAR_END_UNIT}"));

    if hands.copy_code() {
      hands.say(&format!("{node}: standing the link's far end as {FAR_END_UNIT}"));

      let unit = far_end_unit(&home, &root, &place.session, socket, ports);
      let stood = shell
        .run(&format!(
          "mkdir -p {dir} && printf %s {unit} >{file} && systemctl --user daemon-reload && systemctl --user enable -q {FAR_END_UNIT} && systemctl --user restart {FAR_END_UNIT} && sleep 2 && systemctl --user is-active -q {FAR_END_UNIT}",
          dir = shell_word(&join(&home, ".config/systemd/user")),
          unit = shell_word(&unit),
          file = shell_word(&unit_file),
        ))
        .await;

      if stood.code != 0 {
        missing.push(format!("far end: {FAR_END_UNIT} does not run: {}", last_lines(&stood.stderr, 2)));
      }
    } else if shell.run(&format!("systemctl --user is-active -q {FAR_END_UNIT}")).await.code != 0 {
      missing.push(format!("far end: {FAR_END_UNIT} does not run on the node; run the setup"));
    }
  }

  // Every login a run on the node goes out on, its first-run screens marked as passed and the workplace's data
  // root trusted, where a supervisor's own folder and a run's folder stand: none of them stops a program at its
  // start (docs/work-nodes.md, 「The workplace on a node」).
  if hands.copy_code() {
    for login in &place.logins {
      let file = login_file(place, login, &home, &root);
      let dir = std::path::Path::new(&file)
        .parent()
        .map(|dir| dir.to_string_lossy().into_owned())
        .unwrap_or_default();
      let marked = shell
        .run(&with_path(
          &root,
          &format!(
            "mkdir -p {} && {} trust --onboarded {} {}",
            shell_word(&dir),
            shell_word(&join(&deploy, "bin/agent-host")),
            shell_word(&file),
            shell_word(&data)
          ),
        ))
        .await;

      if marked.code != 0 {
        missing.push(format!(
          "login {}: its configuration on the node could not be marked: {}",
          login.account,
          last_lines(&format!("{}{}", marked.stdout, marked.stderr), 2)
        ));
      }
    }
  }

  // The checkouts placed there, cloned where they are not, set up, and trusted under every login.
  for checkout in &place.checkouts {
    let cwd = node_path(&checkout.cwd, Some(&home));
    let standing = shell.run(&format!("test -d {}", shell_word(&join(&cwd, ".git")))).await;

    if standing.code != 0 && !hands.copy_code() {
      missing.push(format!("{}: no checkout at {cwd}; run the setup", checkout.worker));
      continue;
    }

    if standing.code != 0 {
      let Some(remote) = &checkout.remote else {
        missing.push(format!(
          "{}: no checkout at {cwd}, and its base names no remote to clone",
          checkout.worker
        ));
        continue;
      };

      hands.say(&format!("{node}: cloning {} into {cwd}", checkout.worker));

      let cloned = shell
        .run(&with_path(&root, &format!("git clone -q {} {}", shell_word(remote), shell_word(&cwd))))
        .await;

      if cloned.code != 0 {
        let said = cloned.stderr.trim().split('\n').next_back().unwrap_or_default().to_owned();

        missing.push(format!("{}: could not clone {remote}: {said}", checkout.worker));
        continue;
      }
    }

    if hands.copy_code() {
      let setup = shell
        .run(&with_path(&root, &format!(
          r#"cd {} && if grep -q '"worktree:setup"' package.json 2>/dev/null; then bun run worktree:setup; fi"#,
          shell_word(&cwd)
        )))
        .await;

      if setup.code != 0 {
        missing.push(format!("{}: its setup failed: {}", checkout.worker, last_lines(&setup.stderr, 2)));
      }
    }

    for login in &place.logins {
      let file = login_file(place, login, &home, &root);

      shell
        .run(&with_path(
          &root,
          &format!(
            "{} trust {} {}",
            shell_word(&join(&deploy, "bin/agent-host")),
            shell_word(&file),
            shell_word(&cwd)
          ),
        ))
        .await;
    }

    let push = shell
      .run(&with_path(
        &root,
        &format!("cd {} && git push --dry-run -q origin HEAD:refs/heads/bmo-node-check", shell_word(&cwd)),
      ))
      .await;

    if push.code != 0 {
      let said = push.stderr.trim().split('\n').next_back().unwrap_or_default().to_owned();

      missing.push(format!("{}: the node cannot push to its remote: {said}", checkout.worker));
    }
  }

  // Each login a run on the node goes out on, never a credential file copied: the account's token for
  // the work nodes, which this machine hands every run, or else a login made on the node in the
  // account's directory there.
  for login in &place.logins {
    if login.node_token {
      continue;
    }

    let path = (place.to_node)(&login.path, &home, &root);

    if login.own_directory
      && shell
        .run(&format!("test -s {}", shell_word(&join(&path, ".credentials.json"))))
        .await
        .code
        == 0
    {
      continue;
    }

    missing.push(format!(
      "login {}: no token for the work nodes; on this machine, run {}",
      login.account, login.token_command
    ));
  }

  SetupOutcome {
    home: Some(home),
    root: Some(root),
    missing,
    herdr_socket,
    copied,
    confinement,
    claude,
  }
}

/// The command bubblewrap is tried with: every namespace its own, as a personal tool's run takes them, running
/// nothing but `true` over the machine's own filesystem read alone.
pub const CONFINEMENT_TRY: &str = "bwrap --ro-bind / / --unshare-all --die-with-parent true";

/// Why a person's PC cannot run its owner's personal tools confined, or none where it can: bubblewrap looked for,
/// installed through the package manager where the setup may install and the PC lets it without a password, and
/// tried (docs/person-nodes.md, 「Personal tools on a PC」).
pub async fn confinement(shell: &dyn SetupShell, root: &str, hands: &dyn SetupHands) -> Option<String> {
  let mut standing = match looked(shell, root, "bwrap").await {
    Ok(standing) => standing,
    Err(why) => return Some(format!("bubblewrap could not be looked for on the PC: {why}")),
  };

  if !standing && hands.install() {
    hands.say("installing bubblewrap");

    let installed = shell
      .run("sudo -n apt-get install -y -q bubblewrap </dev/null || sudo -n dnf install -y -q bubblewrap </dev/null")
      .await;

    standing = installed.code == 0 && found(shell, root, "bwrap").await;
  }

  if !standing {
    return Some(
      "bubblewrap (bwrap) is not on the PC, and its personal tools run confined or not at all: install the bubblewrap package in its Linux shell (sudo apt install bubblewrap), or run the setup with --install where sudo asks no password".to_owned(),
    );
  }

  let tried = shell.run(&with_path(root, CONFINEMENT_TRY)).await;

  (tried.code != 0).then(|| {
    format!(
      "bubblewrap makes no namespace on the PC, so its personal tools cannot run: {}",
      last_lines(&format!("{}{}", tried.stdout, tried.stderr), 2)
    )
  })
}
