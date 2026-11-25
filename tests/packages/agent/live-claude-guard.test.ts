import "@agency/test-support/setup";

import { installProviderSettings, setProviderEnabled } from "@agency/agent/provider-order";
import { guardClaudeLaunch, guardHerdrCall, guardPanePrompt } from "@agency/agent/live-claude-guard";
import { describe, expect, test } from "bun:test";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import { Archive } from "@agency/archive/archive";
import { archiveSettingPath } from "@agency/archive/archive-place";
import { herdrCallRefusal, launchRefusal, liveClaudeOpening, promptRefusal, recordLiveClaudeRun, socketRefusal, TEST_HOME_VARIABLE } from "@agency/agent/live-claude-guard";
import { accountHome, codeRoot, ownDataRoot } from "@agency/support/roots";
import { scratchDir } from "@agency/test-support/scratch";
import { UNIT } from "@agency/test-support/ids";
import queueWords from "@agency/policy/queue-words.toml";
import { scratchArchive, scratchDatabase } from "@agency/test-support/store";

// The guard every launch of the Claude Code CLI and every word put into a pane asks
// (docs/live-claude-guard.md). What a job may open is held here against a job's own markers, a
// room's, the bot's and a test's, and the job's opening is read from an archive of the test's own.

const root = codeRoot;
const unit = "bmo-run-331-stage-4-0-1";
const cli = ["claude", "-p", "--output-format", "json", "--resume", "00000000-0000-0000-0000-000000000000"];

// A job's shell, as its CLI starts it: both markers, the run's own name, an archive of the test's own.
function jobEnv(address: string): Record<string, string> {
  return { CLAUDECODE: "1", BMO_TASK_LOCKED: "1", BMO_RUN_NAME: unit, BMO_ARCHIVE_URL: address, PATH: "/usr/bin:/bin" };
}

const purpose = "measure what a /compact costs on a copy of the session";

// A store of the test's own, holding, when asked for, an opening this job's run made for itself.
async function archiveWith(opened: boolean): Promise<string> {
  const archive = await scratchArchive();

  if (opened) {
    await archive.sql.run("INSERT INTO live_claude_openings (run, purpose, opened_at) VALUES (?, ?, ?)", unit, purpose, 1);
  }

  return archive.path;
}

// The opening kept under this job's run, with what it let through.
async function openingOf(address: string): Promise<{ purpose: string; runs: { what: string }[] } | undefined> {
  const archive = new Archive(address);

  try {
    const opening = await archive.sql.get("SELECT id, purpose FROM live_claude_openings WHERE run = ? ORDER BY id DESC LIMIT 1", unit) as { id: number; purpose: string } | null;

    if (opening === null) return undefined;

    const runs = await archive.sql.all("SELECT what FROM live_claude_runs WHERE opening_id = ? ORDER BY id", opening.id) as { what: string }[];

    return { purpose: opening.purpose, runs };
  } finally {
    await archive.close();
  }
}

describe("a job opening a Claude Code session", () => {
  test("is refused on every line that carries a session, with the way to ask named", async () => {
    const env = jobEnv(await archiveWith(false));

    for (const line of ["worker", "chat", "search", "browse", "pane"]) {
      const reason = await launchRefusal(line, cli, { env });

      expect(reason).toContain("before it has said what for");
      expect(reason).toContain("bin/bmo-agency live-claude --purpose");
    }

    // Behind a lock, as a fold is started.
    expect(await launchRefusal("worker", ["flock", "--nonblock", "/tmp/x.lock", ...cli], { env })).not.toBeNull();
    expect(await promptRefusal(env)).not.toBeNull();
  });

  test("is not refused a gate run, which keeps nothing, nor a program that is not the CLI", async () => {
    const env = jobEnv(await archiveWith(false));

    expect(await launchRefusal("gate", cli, { env })).toBeNull();
    expect(await launchRefusal("worker", ["codex", "exec", "hi"], { env })).toBeNull();
  });

  test("opens once the job has said what for, for that job alone, with nobody asked", async () => {
    const env = jobEnv(await archiveWith(true));

    expect(await liveClaudeOpening(env)).not.toBeNull();
    expect(await launchRefusal("worker", cli, { env })).toBeNull();
    expect(await promptRefusal(env)).toBeNull();
    // Another job's run is not opened by it.
    expect(await launchRefusal("worker", cli, { env: { ...env, BMO_RUN_NAME: UNIT } })).not.toBeNull();
  });

  test("keeps every run it lets through against the opening, and nothing where there is none", async () => {
    const dbPath = await archiveWith(true);
    const env = jobEnv(dbPath);

    await recordLiveClaudeRun("worker line: claude -p", env);
    await recordLiveClaudeRun("words put into a pane's session", env);
    // A process that is not a job keeps nothing.
    await recordLiveClaudeRun("chat line", { CLAUDECODE: "1", BMO_ARCHIVE_URL: dbPath, BMO_RUN_NAME: unit });

    const opening = await openingOf(dbPath);

    expect(opening?.purpose).toBe(purpose);
    expect(opening?.runs.map((ran) => ran.what)).toEqual(["worker line: claude -p", "words put into a pane's session"]);
    // Nor does a job that opened nothing, on an archive of its own made after the first is read.
    await recordLiveClaudeRun("worker line", jobEnv(await archiveWith(false)));
  });

  // A caller asks before it spawns and the spawn asks again on the same command line. The guard keeps
  // runs only outside a test, so a script of its own asks as a job's process would.
  test("keeps one launch once however often it is asked about, and a second launch again", async () => {
    const dbPath = await archiveWith(true);
    const dir = scratchDir("bmo-live-claude-once-");
    const script = join(dir, "launch.ts");

    writeFileSync(
      script,
      [
        `import { guardClaudeLaunch } from ${JSON.stringify(join(root, "packages", "agent", "live-claude-guard.ts"))};`,
        `const command = ${JSON.stringify(cli)};`,
        `await guardClaudeLaunch("worker", command);`,
        `await guardClaudeLaunch("worker", command);`,
        `await guardClaudeLaunch("worker", [...command]);`,
      ].join("\n"),
    );

    const env: Record<string, string | undefined> = { ...process.env, ...jobEnv(dbPath), PATH: process.env.PATH };

    delete env[TEST_HOME_VARIABLE];

    const run = Bun.spawnSync([process.execPath, script], { cwd: dir, env: env as Record<string, string>, stdin: "ignore" });

    expect(run.stderr.toString()).toBe("");
    expect((await openingOf(dbPath))?.runs).toHaveLength(2);
  });

});

describe("a job driving herdr through the bot's own client", () => {
  const layout = {
    tab_id: "t",
    root: { type: "split", first: { type: "pane", command: ["zsh"] }, second: { type: "pane", command: ["flock", "/tmp/x.lock", ...cli] } },
  };

  test("is refused a pane started on the CLI and every word or key put into a pane", async () => {
    const env = jobEnv(await archiveWith(false));

    expect(await herdrCallRefusal("layout.apply", layout, env)).toContain("in a herdr pane");
    expect(await herdrCallRefusal("layout.apply", { root: { type: "pane", command: ["zsh", "-c", "claude --resume x"] } }, env)).not.toBeNull();
    expect(await herdrCallRefusal("agent.prompt", { target: "p", text: "/compact" }, env)).toContain("put words into a pane");
    expect(await herdrCallRefusal("pane.send_keys", { pane_id: "p", keys: ["Enter"] }, env)).not.toBeNull();
  });

  test("is let through every other call, and every call once the job has said what for", async () => {
    const env = jobEnv(await archiveWith(false));

    expect(await herdrCallRefusal("workspace.close", { workspace_id: "w" }, env)).toBeNull();
    expect(await herdrCallRefusal("layout.apply", { root: { type: "pane", command: ["bun", "scripts/dev-pane.ts"] } }, env)).toBeNull();

    const approved = jobEnv(await archiveWith(true));

    expect(await herdrCallRefusal("layout.apply", layout, approved)).toBeNull();
    expect(await herdrCallRefusal("agent.prompt", { target: "p", text: "/compact" }, approved)).toBeNull();
  });

  // The #318 job's own technique: a script of its own importing the pane opener and the herdr client
  // straight, past the turn helpers. The script runs as a job's process would, against a socket of its
  // own that counts what connects to it.
  test("meets the guard in the client itself, before anything reaches the socket", async () => {
    const dir = scratchDir("bmo-live-claude-herdr-");
    const socket = join(dir, "herdr.sock");
    const script = join(dir, "drive.ts");
    const where = JSON.stringify({ socket });

    writeFileSync(
      script,
      [
        `import { openPane } from ${JSON.stringify(join(root, "packages", "agent", "herdr-run.ts"))};`,
        `import { herdrCall } from ${JSON.stringify(join(root, "packages", "agent", "herdr.ts"))};`,
        "let connections = 0;",
        `const server = Bun.listen({ unix: ${JSON.stringify(socket)}, socket: { open(s) { connections += 1; s.end(); }, data() {} } });`,
        "const outcome = async (call) => { try { await call(); return 'returned'; } catch (error) { return error.name; } };",
        `const opened = await outcome(() => openPane({ name: "x", title: "x", cwd: "/tmp", command: ${JSON.stringify(cli)}, env: {} }, ${where}));`,
        `const prompted = await outcome(() => herdrCall(${JSON.stringify("agent.prompt")}, { target: "p", text: "/compact" }, ${where}));`,
        "server.stop(true);",
        "console.log(JSON.stringify({ opened, prompted, connections }));",
      ].join("\n"),
    );

    const env: Record<string, string | undefined> = { ...process.env, ...jobEnv(await archiveWith(false)), PATH: process.env.PATH };

    delete env[TEST_HOME_VARIABLE];

    const run = Bun.spawnSync([process.execPath, script], { cwd: dir, env: env as Record<string, string>, stdin: "ignore" });

    expect(JSON.parse(run.stdout.toString() || "null")).toEqual({ opened: "LiveClaudeRefused", prompted: "LiveClaudeRefused", connections: 0 });
  });

  test("a test is held to its socket rule rather than to this one", async () => {
    const env = { ...jobEnv(await archiveWith(false)), [TEST_HOME_VARIABLE]: "/tmp/x" };

    expect(await herdrCallRefusal("agent.prompt", { target: "p", text: "/compact" }, env)).toBeNull();
  });
});

describe("processes that are not a job", () => {
  test("a room's session, which carries the session marker alone, opens what it opens", async () => {
    expect(await launchRefusal("chat", cli, { env: { CLAUDECODE: "1", PATH: "/usr/bin" } })).toBeNull();
  });

  test("the bot's own services carry neither marker and are never asked", async () => {
    expect(await launchRefusal("worker", cli, { env: { BMO_TASK_LOCKED: "1", PATH: "/usr/bin" } })).toBeNull();
    expect(await promptRefusal({ PATH: "/usr/bin" })).toBeNull();
  });
});

describe("a test starting a program or reaching herdr", () => {
  const home = scratchDir("bmo-live-claude-home-");
  const fake = join(scratchDir("bmo-live-claude-fake-"), "bin");
  const machine = { PATH: "/nonexistent-bmo-bin" };

  Bun.spawnSync(["mkdir", "-p", fake, join(home, "bin")]);
  writeFileSync(join(fake, "claude"), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  writeFileSync(join(home, "bin", "claude"), "#!/bin/sh\nexit 97\n", { mode: 0o755 });

  const env = { [TEST_HOME_VARIABLE]: home, PATH: `${join(home, "bin")}:/usr/bin` };

  test("starts the stand-in in its home or a fake of its own, and nothing else", async () => {
    expect(await launchRefusal("gate", ["claude", "-p"], { env })).toBeNull();
    expect(await launchRefusal("worker", ["claude", "-p"], { env, commandEnv: { PATH: `${fake}:/usr/bin` } })).toBeNull();
    expect(await launchRefusal("worker", ["/home/someone/.local/bin/claude", "-p"], { env })).toContain("the machine's own claude");
    // A test whose PATH has lost the stand-in reaches whatever `claude` the machine has, and is refused.
    expect(await launchRefusal("gate", ["claude", "-p"], { env: { ...env, ...machine }, commandEnv: { PATH: `/usr/local/bin:/usr/bin:${join(accountHome() ?? "/", ".local", "bin")}` } }) ?? "no claude on this PATH").not.toBeNull();
  });

  test("reaches a herdr under its home or the temporary directory, and never the machine's", () => {
    expect(socketRefusal(join(home, ".config", "herdr", "herdr.sock"), env)).toBeNull();
    expect(socketRefusal("/tmp/bmo-herdr-standin-x/herdr/herdr.sock", env)).toBeNull();
    expect(socketRefusal("/home/someone/.config/herdr/herdr.sock", env)).toContain("the herdr at /home/someone/.config/herdr/herdr.sock");
  });
});

test("Claude-off launch and pane delivery checks read the current archive", async () => {
 const archive = await scratchArchive();
 installProviderSettings(archive);
 try {
  await setProviderEnabled(archive, "claude-code", false);
  expect(() => guardClaudeLaunch("chat", ["claude", "-p"])).toThrow(queueWords.pool.off);
  expect(() => guardClaudeLaunch("worker", ["claude", "--resume", "held"])).toThrow(queueWords.pool.off);
  expect(() => guardPanePrompt()).toThrow(queueWords.pool.off);
  // A program pointed at an own LLM service's endpoint is that road's, which Claude being off leaves open.
  const road = { ANTHROPIC_BASE_URL: "http://127.0.0.1:18030" };
  expect(() => guardHerdrCall("pane.create", { command: ["claude", "--model", "local-0-1"], env: road })).not.toThrow();
  expect(() => guardHerdrCall("pane.create", { command: ["claude", "--model", "claude-opus-5-5"], env: {} })).toThrow(queueWords.pool.off);
  expect(() => guardClaudeLaunch("worker", ["claude", "--model", "local-0-1"], road)).not.toThrow(queueWords.pool.off);
  expect(() => guardPanePrompt({ claude: false })).not.toThrow();
 } finally { installProviderSettings(null); }
});
