import "@agency/test-support/setup";

import { afterAll, describe, expect, setDefaultTimeout, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import { installPlacer, spawnAgentProcess } from "@agency/agent/agent-process";
import { installProviderSettings, setProviderEnabled } from "@agency/agent/provider-order";
import { scratchArchive } from "@agency/test-support/store";
import { workspaceBinary } from "@agency/test-support/binaries";
import { scratchDir } from "@agency/test-support/scratch";
import queueWords from "@agency/policy/queue-words.toml";

import type { Archive } from "@agency/archive/archive";

// One run of a provider's program carried in its agent's host, by the TypeScript parity reference (`spawnAgentProcess`) and by
// the Rust side (`bmo-agency host-run`), each on a data root of its own with the real agent host serving it
// (docs/agent-control.md, 「The host」). What
// each came to must be the same: the outcome, what the program wrote, the execution row as it was left, and
// the lines the agent's view was given.

const binary = workspaceBinary("bmo-agency");
const hostBinary = workspaceBinary("agent-host");
const agent = "voice-polish";
const line = "gate";
const provider = "codex";
const previous = process.env.BMO_DATA_DIR;

interface Side {
  data: string;
  archive: Archive;
  archiveUrl: string;
  host: ReturnType<typeof Bun.spawn>;
}

const sides: Side[] = [];

// A data root of its own, its archive, and the agent host serving it.
async function stand(): Promise<Side> {
  const data = join(scratchDir("bmo-host-run-"), "data");

  mkdirSync(join(data, "executions"), { recursive: true });

  const archive = await scratchArchive();
  const archiveUrl = archive.path;
  const host = Bun.spawn([hostBinary, "serve", "--data-root", data, "--code-root", data, "--poll-ms", "10"], { stdout: "ignore", stderr: "ignore" });

  for (let look = 0; look < 400 && !existsSync(join(data, "executions", "agent-host", "claim")); look += 1) await Bun.sleep(25);

  const side = { data, archive, archiveUrl, host };

  sides.push(side);

  return side;
}

afterAll(async () => {
  for (const side of sides) {
    side.host.kill();
    await side.host.exited;
    rmSync(side.data, { recursive: true, force: true });
  }

  installProviderSettings(null);
  installPlacer(null);
  if (previous === undefined) delete process.env.BMO_DATA_DIR; else process.env.BMO_DATA_DIR = previous;
});

// The program: its launch marked in the folder it runs in, Codex's events for a thread it opened and what it
// said, a word on its error stream, and an exit of its own.
function program(exit: number, sleep = 0): string[] {
  const events = [
    JSON.stringify({ type: "thread.started", thread_id: "thread-1" }),
    JSON.stringify({ type: "item.completed", item: { type: "command_execution", command: "ls" } }),
    JSON.stringify({ type: "item.completed", item: { type: "agent_message", text: "all done\nsecond line" } }),
  ];

  return ["sh", "-c", `: > ${LAUNCHED}; ${sleep > 0 ? `sleep ${sleep}; ` : ""}printf '%s\\n' ${events.map((one) => `'${one}'`).join(" ")}; echo warn >&2; exit ${exit}`];
}

// The file a run's program puts down in its folder as it starts.
const LAUNCHED = "launched";

// How long a stop waits for the program to start before it is put down all the same, in tenths of a second.
const LAUNCH_WAIT_TENTHS = 600;

interface Asked {
  command: string[];
  // How long after the run's program has started its caller stops it, by putting a file down: a side that
  // takes long to start its program is stopped as late into the run as one that starts it at once.
  stopAfterMs?: number;
  // Named always, so neither side counts the run under the unit the test itself runs in.
  root: string;
}

// What a side's run came to, with what differs only by being made apart in the same shape.
function seen(side: Side, came: { exitCode?: number; cause?: string; stdout?: string; stderr?: string; thrown?: string; row: Record<string, unknown> | null }): unknown {
  const view = join(side.data, "executions", "agent-host", "views", agent);
  const row = came.row === null ? null : Object.fromEntries(Object.entries(came.row).map(([key, value]) => [
    key,
    ["id", "startedAt", "endedAt"].includes(key) ? typeof value : value,
  ]));

  return {
    ...came,
    row,
    // The resting line is the host's own, written whenever it next finds nothing running.
    view: existsSync(view) ? readFileSync(view, "utf8").split("\n").filter((one) => !one.startsWith("resting")).join("\n").replace(/\d\d:\d\d/g, "<clock>").replace(/bmo-execution-[0-9a-f-]+/g, "<id>") : "",
  };
}

async function bun(side: Side, asked: Asked): Promise<unknown> {
  process.env.BMO_DATA_DIR = side.data;
  installProviderSettings(side.archive);

  let timer: ReturnType<typeof setInterval> | null = null;
  const stopFile = join(side.data, "stop");

  try {
    putDownLater(stopFile, asked.stopAfterMs);

    const proc = await spawnAgentProcess({ args: asked.command, cwd: side.data, env: { PATH: process.env.PATH }, stdin: null }, line, provider, {
      agent, model: "gpt-5.6-terra", effort: "low", parent: "bmo-test", root: asked.root,
    });

    timer = asked.stopAfterMs === undefined ? null : setInterval(() => { if (existsSync(stopFile)) proc.kill(); }, 25);
    const [stdout, stderr] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text()]);
    const exitCode = await proc.exited;
    const rows = await side.archive.executions().executions();

    return seen(side, { exitCode, stdout, stderr, row: rows.at(-1) as unknown as Record<string, unknown> });
  } catch (error) {
    const rows = await side.archive.executions().executions();

    return seen(side, { thrown: `${(error as Error).name}: ${(error as Error).message}`, row: (rows.at(-1) ?? null) as unknown as Record<string, unknown> | null });
  } finally {
    if (timer !== null) clearInterval(timer);
  }
}

// The caller's stop, put down beside the program's launch mark by a process of its own once the program has
// started, since the Rust side's run holds this one's loop; the wait for the start is bounded.
function putDownLater(file: string, afterMs: number | undefined): void {
  if (afterMs === undefined) return;

  const launched = join(dirname(file), LAUNCHED);
  const waited = `n=0; while [ ! -e '${launched}' ] && [ $n -lt ${LAUNCH_WAIT_TENTHS} ]; do sleep 0.1; n=$((n + 1)); done`;

  Bun.spawn(["sh", "-c", `${waited}; sleep ${afterMs / 1000}; : > '${file}'`], { stdin: "ignore", stdout: "ignore", stderr: "ignore" });
}

async function rust(side: Side, asked: Asked): Promise<unknown> {
  const request = join(side.data, "host-run.json");
  const stopFile = join(side.data, "stop");

  putDownLater(stopFile, asked.stopAfterMs);
  writeFileSync(request, JSON.stringify({
    agent, line, provider,
    command: { args: asked.command, cwd: side.data, env: { PATH: process.env.PATH }, stdin: null },
    model: "gpt-5.6-terra", effort: "low", parent: "bmo-test", root: asked.root,
    ...(asked.stopAfterMs === undefined ? {} : { stopFile }),
  }));

  const ran = Bun.spawnSync([binary, "host-run", request], {
    env: { ...process.env, BMO_DATA_DIR: side.data, BMO_ARCHIVE_URL: side.archiveUrl },
  });
  const came = JSON.parse(ran.stdout.toString()) as Record<string, unknown>;

  if (came.thrown !== undefined) {
    const rows = await side.archive.executions().executions();

    return seen(side, { thrown: came.thrown as string, row: (rows.at(-1) ?? null) as unknown as Record<string, unknown> | null });
  }

  // How it ended is the row's state, which the bun side reads there alone.
  const { cause: _cause, ...rest } = came;

  return seen(side, rest as never);
}

async function twins(prepare: (side: Side) => Promise<void>, asked: Asked): Promise<{ bun: unknown; rust: unknown }> {
  const one = await stand();
  const other = await stand();

  await prepare(one);
  await prepare(other);

  return { bun: await bun(one, asked), rust: await rust(other, asked) };
}

// A twin starts `bmo-agency` and stands a store of its own on each side, several times in one
// scenario: a second or two alone, many times that on a machine carrying other runs.
setDefaultTimeout(60_000);

describe("one run carried in its agent's host, by either side", () => {
  test("a run its caller stops: cancelled, with what it said up to then", async () => {
    const seenBy = await twins(async () => undefined, { command: program(0, 30), stopAfterMs: 500, root: "bmo-root-test" });

    expect(seenBy.rust).toEqual(seenBy.bun);
    expect(seenBy.bun).toMatchObject({ row: { state: "cancelled" } });
  }, 60_000);

  test("a run whose provider is switched off is refused before it is placed", async () => {
    const seenBy = await twins(async (side) => {
      await setProviderEnabled(side.archive, provider, false);
    }, { command: program(0), root: "bmo-root-test" });

    expect(seenBy.rust).toEqual(seenBy.bun);
    expect(seenBy.bun).toMatchObject({ thrown: `Error: codex: ${queueWords.pool.off}`, row: { state: "launch-error" } });
  }, 60_000);

  test("a run a standing cancellation covers ends as a stop", async () => {
    const seenBy = await twins(async (side) => {
      await side.archive.settings().setSetting("execution_cancel:bmo-test-root", "1");
    }, { command: program(0), root: "bmo-test-root" });

    expect(seenBy.rust).toEqual(seenBy.bun);
    expect(seenBy.bun).toMatchObject({ thrown: "ExecutionStopped: execution cancelled", row: { state: "cancelled" } });
  }, 60_000);
});
