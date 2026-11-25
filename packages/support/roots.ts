import { existsSync, readFileSync, realpathSync, statSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Three directories, and once the services run out of a deployment of their own they are three
// different places. Everything that has to find one of them asks here, so that a path is
// decided once rather than four times.
//
// - The **code root** is the checkout this file was loaded out of. It is the code that is
//   running, and nothing but code is looked for under it.
// - The **source root** is the checkout people and job workers work in: where documents are
//   edited and committed, where git is run, where a job is launched, and what a warm session's
//   working directory is. BMO_SOURCE_DIR names it; without one it is the code root, which is
//   what a person running a script by hand out of a single checkout wants. It is a workplace and
//   nothing more: nothing the running service composes an agent out of is read from it, so a
//   checkout in any state — a document moved, a file half written — leaves the running bot as it
//   was. What the service reads ships with its code, under the code root (`assetRoot`).
// - The **data root** is where the runtime bytes live — the archive, the photos, the avatars,
//   the collected files, the locks and the env files. It sits outside every git tree on
//   purpose: bytes inside a checkout are untracked bytes, and an untracked byte is one `git
//   clean` away from gone.

/**
 * The checkout this code is running out of. It is worked out from this module's own URL, which
 * is the one thing every runtime that loads this file agrees on: the sidecars and the verbs run on
 * Bun, and the web console's server is built and run by its own framework, which does not answer
 * the shorthand Bun would. A file that reached for the shorthand would throw the moment the console
 * imported anything under here, and it does.
 */
export const codeRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");

/**
 * The tree the running code's own documents are read out of: the agent documents, the rulebooks and
 * the procedures, which ship with the code and go live with it.
 * It is the code root, so a service run out of the deploy tree composes out of the deploy tree and
 * a script run out of a checkout composes out of that checkout. BMO_ASSET_DIR moves it, and is
 * heard only inside the test home, where a test composes out of a fixture tree; everywhere else
 * nothing can point a running service at another tree's documents.
 */
export function assetRoot(): string {
  const named = process.env.BMO_ASSET_DIR;

  return named === undefined || named === "" || process.env[TEST_HOME_VARIABLE] === undefined ? codeRoot : resolve(named);
}

// The variable the test preload names the run's home under. A process that carries it is a test.
export const TEST_HOME_VARIABLE = "BMO_TEST_HOME";

// The variables that move a root, each read here alone. Every one of them is heard only where it is
// set and not empty, which is how the test preload takes a caller's away.
export const rootVariables = { source: "BMO_SOURCE_DIR", deploy: "BMO_DEPLOY_DIR", data: "BMO_DATA_DIR" } as const;

// The variable a resident program is started with to say which place it answers to. Its environment
// is fixed when it starts, so the run a turn belongs to is not in it: whatever the program runs
// reads that run out of the place (docs/agent-control.md, 「The keepalive ping goes into the same
// program as the turns」).
export const PLACE_VARIABLE = "BMO_AGENT_PLACE";

/** The variable a deployment's unit is started with, naming it; every process in the unit inherits it. */
export const deployUnitMarker = "BMO_DEPLOY_UNIT";

// The file in the test home every refused call is written to, which the preload reads as each test
// file ends: a refusal a caller swallowed still fails the file.
export const REFUSED_LOG = "refused";

/** The checkout that is edited and committed, and that every session stands in. */
export function sourceRoot(): string {
  const named = process.env[rootVariables.source];

  return named === undefined || named === "" ? codeRoot : resolve(named);
}

// Where a linked worktree's `.git` file says its git directory is, or nothing for a tree whose
// `.git` is the repository itself.
function linkedGitDir(tree: string): string | null {
  const marker = join(tree, ".git");

  try {
    if (!statSync(marker).isFile()) {
      return null;
    }

    const said = /^gitdir:\s*(.+)$/m.exec(readFileSync(marker, "utf8"));

    return said === null ? null : resolve(tree, said[1]!.trim());
  } catch {
    return null;
  }
}

/**
 * The home directory as the environment names it now. The runtime's own answer is read once, when
 * the process starts, and a process that has moved its home since — a test file, whose preload
 * moves it before its first line — must see the move, so `HOME` is asked first on every call.
 */
export function homeDir(): string {
  const named = process.env.HOME;

  return named === undefined || named === "" ? homedir() : named;
}

/** The deploy tree the service runs out of, where one has been set up: agency's own, unless one is named. */
export function deployRoot(): string {
  const named = process.env[rootVariables.deploy];

  // Agency's own installation directory under the home, a name fixed outside the code. It holds nothing
  // of another service's; the data root and the agents' directory are the shared installation's.
  return named === undefined || named === "" ? join(homeDir(), ".local", "share", "bmo-agency", "deploy") : resolve(named);
}

/**
 * The deploy tree's link, which everything standing beside the tree is found from: the deploy tree
 * itself, or, where `BMO_DEPLOY_DIR` names one of its releases — as a job carries the release it was
 * launched on — the link that release stands under.
 */
export function deployLink(): string {
  const deploy = deployRoot();
  const store = dirname(deploy);
  const suffix = "-releases";

  // A release is named by twelve hex digits under the link's name with the store's suffix.
  return /^[0-9a-f]{12}$/.test(basename(deploy)) && basename(store).endsWith(suffix) && basename(store).length > suffix.length
    ? join(dirname(store), basename(store).slice(0, -suffix.length))
    : deploy;
}

// A path as the disk resolves it, links and all; as it is written where nothing stands there.
function realPath(path: string): string {
  try {
    return realpathSync(path);
  } catch {
    return resolve(path);
  }
}

/**
 * Whether a working tree is one of the deployment's own: the release the deploy tree's link names or
 * any other beside it, or the build tree. Compared by real
 * path, since a process started through the link stands in the release it names.
 */
export function isDeploymentTree(tree: string): boolean {
  const deploy = deployLink();
  const at = realPath(tree);

  return at === realPath(deploy) || at === realPath(`${deploy}-expand`) || dirname(at) === realPath(`${deploy}-releases`);
}

/**
 * Where the agents' composed files are written — each runner's working directory, the system
 * prompts a run reads, the servers a line is handed. It sits outside the repository on purpose: a
 * directory inside it would inherit the project instruction files up the tree and the repository's
 * git status, and the runners are meant to see neither.
 */
export function agentsRoot(): string {
  // The installation's directory under the home, a name fixed outside the code.
  return process.env.BMO_AGENTS_DIR ?? join(homeDir(), ".local", "share", "bmo", "agents");
}

/**
 * The tree a script is run out of when what it starts must be the code the suite passed: the deploy
 * tree where it holds that script, and the code root on a machine with no deployment — a test, or a
 * checkout run on its own. A script named here and run out of the checkout would put whatever the
 * checkout holds this minute into service before any deployment had tried it.
 */
export function deployedTree(script: string): string {
  return existsSync(join(deployRoot(), script)) ? deployRoot() : codeRoot;
}

// Where the runtime data root stands under a home.
function dataRootUnder(home: string): string {
  // The installation's directory under the home, a name fixed outside the code.
  return join(home, ".local", "share", "bmo", "data");
}

/**
 * The home the account this process runs as was given, read from the user database by the process's
 * uid; null where it has no entry there. The runtime's own answer to the same question reads `HOME`
 * first, which a test moves, so it would name the test's home back.
 */
export function accountHome(passwd: string = "/etc/passwd", uid: number | undefined = process.getuid?.()): string | null {
  try {
    const entry = readFileSync(passwd, "utf8").split("\n").map((line) => line.split(":")).find((fields) => fields[2] === String(uid));

    return entry?.[5] !== undefined && entry[5] !== "" ? entry[5] : null;
  } catch {
    return null;
  }
}

// The account's entry does not move while a process runs, so it is read once.
let liveRoot: string | null | undefined;

/**
 * The machine's own data root, the one the running bot keeps its bytes in. It is read off the
 * account rather than off `HOME`, so it names the live root from inside a test and from anything a
 * test starts; null where the account has no home to read.
 */
export function liveDataRoot(): string | null {
  if (liveRoot === undefined) {
    const home = accountHome();

    liveRoot = home === null ? null : dataRootUnder(home);
  }

  return liveRoot;
}

/**
 * Why a process may not take a directory as its data root, or nothing. Only a test is ever refused,
 * and only the machine's own root and what lies under it: a test that reached it would empty the
 * live brain or overwrite a live token with its fixture, whoever handed it the path.
 */
export function testDataRootFault(root: string, live: string | null, env: Record<string, string | undefined> = process.env): string | null {
  if (env[TEST_HOME_VARIABLE] === undefined || live === null || !containsPath(live, root)) {
    return null;
  }

  return `a test reached for the live runtime data root: ${resolve(root)} is inside ${live}; a test names a data root under the temporary directory`;
}

/** Where the runtime bytes live. A test asking for the machine's own is refused by a throw. */
export function dataRoot(): string {
  const named = process.env[rootVariables.data];
  const root = named === undefined || named === "" ? ownDataRoot() : resolve(named);

  if (process.env[TEST_HOME_VARIABLE] !== undefined) {
    const fault = testDataRootFault(root, liveDataRoot());

    if (fault !== null) {
      throw new Error(fault);
    }
  }

  return root;
}

/** The bot's own data root, whatever `BMO_DATA_DIR` names for this process. */
export function ownDataRoot(): string {
  return dataRootUnder(homeDir());
}

/**
 * The data root as a copy — a script pointed with `BMO_DATA_DIR` at a root other than the bot's
 * own — with the name it is shown under, or null where it is the bot's own. The name is the
 * directory's own, or the one holding it where the directory is the usual `data`
 * (docs/agent-control.md, 「What goes where」).
 */
export function dataRootCopy(): { root: string; name: string } | null {
  const root = dataRoot();

  if (root === ownDataRoot()) {
    return null;
  }

  const own = basename(root);

  return { root, name: own === "data" ? basename(dirname(root)) : own };
}

/** A path under the runtime data root. */
export function dataPath(...parts: string[]): string {
  return join(dataRoot(), ...parts);
}

// The directory this machine's own services keep their sockets in: `bmo/` under the user's runtime
// directory, read off the account so a test that moves `HOME` still finds the services' own.
export function serviceSocketDir(): string {
  const runtime = process.env.XDG_RUNTIME_DIR;
  const uid = process.getuid?.();

  // The installation's directory under the runtime directory, a name fixed outside the code.
  return join(runtime !== undefined && runtime !== "" ? runtime : uid === undefined ? tmpdir() : `/run/user/${uid}`, "bmo");
}

/** The tool host's control socket, where a launcher issues and revokes session tokens (docs/RUNBOOK.md). */
export function mcpHostSocket(): string {
  const named = process.env.BMO_MCP_HOST_SOCKET;

  return named === undefined || named === "" ? join(serviceSocketDir(), "mcp-host.sock") : resolve(named);
}

/**
 * The loopback port the tool host serves sessions on, written into every line's MCP configuration:
 * the bot's environment file names it, and 3290 stands where it names none (docs/RUNBOOK.md).
 */
export function mcpHostPort(): number {
  const named = Number.parseInt(process.env.BMO_MCP_HOST_PORT ?? "", 10);

  return Number.isSafeInteger(named) && named > 0 ? named : 3290;
}

/**
 * Whether `path` is `root` or sits underneath it. The root counts as inside itself: what a
 * caller means by this question is "may I delete it", and deleting the root is the worst
 * answer of all.
 */
export function containsPath(root: string, path: string): boolean {
  const from = resolve(root);
  const to = resolve(path);

  if (from === to) {
    return true;
  }

  const step = relative(from, to);

  return step !== "" && !step.startsWith("..") && !isAbsolute(step);
}

/**
 * The nearest git working tree at or above a path, or nothing. A worktree carries a `.git`
 * file rather than a directory and both count, since `git clean` reaches into either.
 */
export function gitTreeAbove(path: string, present: (candidate: string) => boolean = existsSync): string | null {
  let at = resolve(path);

  for (;;) {
    if (present(join(at, ".git"))) {
      return at;
    }

    const up = dirname(at);

    if (up === at) {
      return null;
    }

    at = up;
  }
}

