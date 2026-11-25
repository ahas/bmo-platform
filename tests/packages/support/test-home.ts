import { afterAll } from "bun:test";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

import { deployUnitMarker, PLACE_VARIABLE, REFUSED_LOG, rootVariables, TEST_HOME_VARIABLE } from "@agency/support/roots";

// The home every test file runs in, set up before the file's first line: each test file imports
// `./setup` first, and `tests/packages/bunfig.toml` preloads it for a run started in that folder.
//
// The cache siren reads what the Claude Code CLI writes under its own directory as real spending,
// and a stand-in's record there looks exactly like a real one. So no test writes there, starts the
// machine's `claude`, or reaches the machine's herdr — not because each test takes care, but because
// the run is moved somewhere those do not exist before any test can try.

const realHome = homedir();
const homePrefix = "agency-test-home-";

// A file killed before its end leaves its home behind; an hour on, it belongs to no run still going.
for (const name of readdirSync(tmpdir())) {
  if (name.startsWith(homePrefix)) {
    try {
      if (statSync(join(tmpdir(), name)).mtimeMs < Date.now() - 60 * 60_000) {
        rmSync(join(tmpdir(), name), { recursive: true, force: true });
      }
    } catch {
      // Taken away by another file's sweep in the same moment.
    }
  }
}

const home = mkdtempSync(join(tmpdir(), homePrefix));
const bin = join(home, "bin");

mkdirSync(bin, { recursive: true });
mkdirSync(join(home, ".claude"), { recursive: true });
mkdirSync(join(home, ".config"), { recursive: true });

// A stand-in under each name that must never reach the real program. It writes down that it was
// called and with what, and ends as a failure; the file it ran in fails when it ends.
for (const name of ["claude", "herdr"]) {
  const path = join(bin, name);

  writeFileSync(
    path,
    [
      "#!/bin/sh",
      `printf '%s\\n' "a test started the stand-in ${name}: $*" >> "${join(home, REFUSED_LOG)}"`,
      `echo "a test started ${name}; tests reach only fakes of their own" >&2`,
      "exit 97",
      "",
    ].join("\n"),
  );
  chmodSync(path, 0o755);
}

// What the tests still read from the real home, named where it is so that moving the home does not
// move it: git's global configuration, so a test that commits in a temporary repository keeps its
// author; the browser builds; and the user cache, where a browser keeps what makes a launch take a
// second rather than several. The data root and the agents' directory are not among them — they follow
// the home, so a test that reaches for the runtime data reaches a directory of its own.
mkdirSync(join(home, "agents"), { recursive: true });
process.env.BMO_AGENTS_DIR = join(home, "agents");

if (process.env.XDG_CACHE_HOME === undefined) {
  process.env.XDG_CACHE_HOME = join(realHome, ".cache");
}

const gitConfig = join(realHome, ".gitconfig");

if (process.env.GIT_CONFIG_GLOBAL === undefined && existsSync(gitConfig)) {
  process.env.GIT_CONFIG_GLOBAL = gitConfig;
}

const browsers = join(realHome, ".cache", "ms-playwright");

if (process.env.PLAYWRIGHT_BROWSERS_PATH === undefined && existsSync(browsers)) {
  process.env.PLAYWRIGHT_BROWSERS_PATH = browsers;
}

process.env.HOME = home;
process.env.CLAUDE_CONFIG_DIR = join(home, ".claude");
process.env.XDG_CONFIG_HOME = join(home, ".config");
process.env[TEST_HOME_VARIABLE] = home;
process.env.PATH = `${bin}:${process.env.PATH ?? ""}`;

// A test run from a job's shell or a room's session carries that session's markers, that pane's
// herdr and the agent place whose run the code under test would take for its own, and the checkout's .env hands it the running server's control token. None of it is the test's:
// a script a test launches would otherwise tell the live server about what exists only in a test's
// store.
for (const name of Object.keys(process.env)) {
  if (name === "CLAUDECODE" || name === "CLAUDE_PID" || name === PLACE_VARIABLE || name.startsWith("CLAUDE_CODE_") || name.startsWith("HERDR_")) {
    delete process.env[name];
  }
}

// A caller's roots are not the test's either: a deployment's unit hands the suite the running server's
// data root, source root and deploy tree, and a test that kept them would read its logins and take
// its hosts' runs for its own. Set empty, each reads as unset and follows the home, and the checkout's
// .env cannot hand one back. The deploy verb takes its own two as given even when empty, so those go.
for (const name of [...Object.values(rootVariables), deployUnitMarker]) {
  process.env[name] = "";
}

delete process.env.BMO_BACKUP_DIR;
delete process.env.BMO_UNIT;

// The store a test opens is the scratch one it names, never the one an env file names; set empty, the
// address reads as unset and nothing a test starts reaches the live store.
process.env.BMO_ARCHIVE_URL = "";

// Nor is the user's service manager: a runtime directory holding no socket, and no bus address, so a
// `systemctl --user` or `systemd-run --user` under test reaches nothing whether or not the caller's
// line could. A launcher's test names a runtime directory of its own beside its own systemd-run.
const runtimeDir = join(home, "runtime");

mkdirSync(runtimeDir, { recursive: true, mode: 0o700 });
process.env.XDG_RUNTIME_DIR = runtimeDir;
delete process.env.DBUS_SESSION_BUS_ADDRESS;

// The token and the address are set empty rather than removed, which reads as no token and no port
// at all: a bun process a test starts in the checkout reads the .env there for every variable that is
// not set, and a removed one would come straight back from it. The token file is cleared too, since a
// caller's environment can name the running server's token that way just as well.
process.env.BMO_CONTROL_TOKEN = "";
process.env.AGENCY_CONTROL_TOKEN = "";
process.env.AGENCY_CONTROL_TOKEN_FILE = "";
process.env.AGENCY_CONTROL_PORT = "";

// The services agency calls are named by no address, for the same reason: a process a test starts
// reaches the live ones of none of them, and answers each one's default (docs/api.md, 「Calls agency
// makes」). A test of one passes its own address.
for (const name of ["AGENCY_NODE_URL", "AGENCY_BRAIN_URL"]) {
  process.env[name] = "";
}

// The deployment's own roots are not the test's either: they name where the running services keep the
// brain and the persona's values, and a server a test starts would read them instead of the home. Set
// empty rather than removed, as with the token, so the checkout's .env cannot hand one back.
for (const name of ["AGENCY_BRAIN_ROOT", "AGENCY_PERSONA_VALUES"]) {
  process.env[name] = "";
}

// The machine's turn for heavy commands is held by the run this file stands in, so a run, a close or a
// testbed command a test starts takes its turn in this home instead and never waits on its own parent,
// and the holder's mark the run handed down names nothing here.
process.env.BMO_TESTBED_LOCK = join(home, "testbed.lock");
delete process.env.BMO_TESTBED_HELD;

// The machine's other shared records stay the machine's too: a test reads an empty machine rather
// than the real one, so no launch it makes is held for the real machine's load; writes down the herdr
// calls its stand-ins leave unanswered in its own home; and takes its Chrome places there.
process.env.BMO_PROC_ROOT = join(home, "proc");
process.env.BMO_HERDR_SILENCES = join(home, "herdr-silences.jsonl");
process.env.BMO_CHROME_PLACES_DIR = home;

// The runtime starts a program on the environment the process began with unless it is handed one,
// and looks a program up on the PATH it began with: the moves above would reach nobody a test or the
// code under it starts without naming an environment. So within a test file, starting a program and
// looking one up both default to the environment as it stands now.
type Spawn = typeof Bun.spawn;
type SpawnSync = typeof Bun.spawnSync;
type Which = typeof Bun.which;

const spawn = Bun.spawn;
const spawnSync = Bun.spawnSync;
const which = Bun.which;

function withEnv<T>(first: unknown, options: unknown, run: (first: unknown, options: unknown) => T): T {
  if (Array.isArray(first)) {
    const given = (options ?? {}) as { env?: unknown };

    return run(first, { ...given, env: given.env ?? process.env });
  }

  const given = first as { env?: unknown };

  return run({ ...given, env: given.env ?? process.env }, undefined);
}

const runtime = Bun as unknown as Record<"spawn" | "spawnSync" | "which", unknown>;

runtime.spawn = ((first: unknown, options?: unknown) =>
  withEnv(first, options, (cmd, opts) => (spawn as unknown as (a: unknown, b: unknown) => unknown)(cmd, opts))) as unknown as Spawn;
runtime.spawnSync = ((first: unknown, options?: unknown) =>
  withEnv(first, options, (cmd, opts) => (spawnSync as unknown as (a: unknown, b: unknown) => unknown)(cmd, opts))) as unknown as SpawnSync;
runtime.which = ((command: string, options?: { PATH?: string; cwd?: string }) =>
  which(command, { ...options, PATH: options?.PATH ?? process.env.PATH ?? "" })) as Which;

afterAll(() => {
  const log = join(home, REFUSED_LOG);
  const refused = existsSync(log) ? readFileSync(log, "utf8").trim() : "";

  rmSync(home, { recursive: true, force: true });

  if (refused !== "") {
    throw new Error(`this test file reached what tests must never reach:\n${refused}`);
  }
});
