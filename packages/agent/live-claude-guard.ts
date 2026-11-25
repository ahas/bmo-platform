import { openArchiveSql, storeAddress } from "@agency/archive/archive-place";
import queueWords from "@agency/policy/queue-words.toml";
import { configuredProviderEnabled } from "./model-settings";
import { jobWorkKey } from "./work-profile";
import { SelectedRouteUnavailableError } from "./route-error";
import { registeredRoad } from "./claude-road";

import { appendFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve, sep } from "node:path";

import { REFUSED_LOG, TEST_HOME_VARIABLE } from "@agency/support/roots";
import { runNameOf, turnEnvironment } from "./agent-place";

// Where a Claude Code session is opened, and who may open one (`docs/live-claude-guard.md`).
//
// A session opened afresh, or a session record copied and continued, writes its whole prefix and
// conversation to the cache again. The bot opens sessions as its own work and knows what they cost.
// Two kinds of process must not without more: a test, which is to reach nothing but the stand-ins in
// its own home, and a job, which may open one once it has said what for. Nobody is asked about a
// job's opening; it is kept with every run it let through, and the item's report is given the
// count. Every place that starts a Claude Code process or puts words into a pane's session asks here
// first.

type Env = Record<string, string | undefined>;

// The variable the test preload names the run's home under. A process that carries it is a test.
export { TEST_HOME_VARIABLE };

// The file in the test home every refusal is written to stands with the other names the test home
// reads (`roots.ts`).
export { REFUSED_LOG };

export class LiveClaudeRefused extends Error {
  constructor(reason: string) {
    super(reason);
    this.name = "LiveClaudeRefused";
  }
}

export function testHome(env: Env = process.env): string | null {
  const home = env[TEST_HOME_VARIABLE];

  return home === undefined || home === "" ? null : home;
}

// A process a job's own CLI started: it carries the Claude Code session marker the CLI plants in
// what its tools run, and the job runner's own marker, which reaches the job's pane with the rest of
// the runner's BMO_ variables. A room's session carries the first and not the second, and the bot's
// services carry neither.
export function insideJob(env: Env = process.env): boolean {
  return nonEmpty(env.CLAUDECODE) && nonEmpty(turnEnvironment(env)[JOB_RUNNER_MARKER]);
}

// The job runner's own marker, set by the runner once it holds its checkout's lock.
export const JOB_RUNNER_MARKER = "BMO_TASK_LOCKED";

function nonEmpty(value: string | undefined): boolean {
  return value !== undefined && value !== "";
}

function within(path: string, dir: string): boolean {
  const target = resolve(path);
  const root = resolve(dir);

  return target === root || target.startsWith(`${root}${sep}`);
}

// The lines whose runs carry a session or write a prefix a later run reads. A gate run is one
// judgement on a small fixed prompt with nothing kept, and a job's own tools — asking the owner,
// sending a message — may need one.
const sessionLines: ReadonlySet<string> = new Set(["worker", "chat", "search", "browse", "pane"]);

// Why starting this command is refused here, or nothing.
export async function launchRefusal(
  line: string,
  command: readonly string[],
  options: { env?: Env; commandEnv?: Env } = {},
): Promise<string | null> {
  const env = options.env ?? process.env;
  const program = programOf(command);

  if (program === null) {
    return null;
  }

  const home = testHome(env);

  if (home !== null) {
    const path = (options.commandEnv ?? env).PATH ?? "";
    const found = program.includes("/") ? program : Bun.which(program, { PATH: path });

    if (found === null || within(found, home) || within(found, tmpdir())) {
      return null;
    }

    return `a test was about to start ${found}, the machine's own ${basename(program)}; a test starts only the stand-in in its own home or a fake of its own`;
  }

  if (basename(program) !== "claude" || !sessionLines.has(line)) {
    return null;
  }

  return await jobRefusal(env, `start a Claude Code session on the ${line} line`);
}

// Why putting words into a pane's session is refused here, or nothing.
export async function promptRefusal(env: Env = process.env): Promise<string | null> {
  return testHome(env) !== null ? null : await jobRefusal(env, "put words into a pane's Claude Code session");
}

// Why a test may not reach this herdr socket, or nothing. Outside a test every socket is the caller's.
export function socketRefusal(socket: string, env: Env = process.env): string | null {
  const home = testHome(env);

  if (home === null || within(socket, home) || within(socket, tmpdir())) {
    return null;
  }

  return `a test was about to reach the herdr at ${socket}; a test reaches only a stand-in under its own home or the temporary directory`;
}

// Why a call to herdr is refused in a job, or nothing: a call that puts words or keys into a pane, and
// a call that starts a pane on the Claude Code CLI. This tree's herdr client asks this before it
// connects, so a script that drives herdr through that client meets the same guard as the launch
// code does. A test follows the socket rule instead.
export async function herdrCallRefusal(method: string, params: unknown, env: Env = process.env): Promise<string | null> {
  if (testHome(env) !== null) {
    return null;
  }

  if (/(^|\.)(prompt|send)/.test(method)) {
    return await promptRefusal(env);
  }

  return commandsIn(params).some(startsCli) ? await jobRefusal(env, "start a Claude Code session in a herdr pane") : null;
}

// Every command a herdr call's parameters carry, however deep in a layout it stands — or, asked for
// a login's alone, every one but those standing beside an environment that makes the program the
// registered models' road (agency's gateway or a registration's own home).
function commandsIn(params: unknown, loginsOnly = false): string[][] {
  if (Array.isArray(params)) {
    return params.flatMap((one) => commandsIn(one, loginsOnly));
  }

  if (params === null || typeof params !== "object") {
    return [];
  }

  const env = (params as { env?: unknown }).env;

  if (loginsOnly && typeof env === "object" && env !== null && registeredRoad(env as Env)) {
    return [];
  }

  return Object.entries(params).flatMap(([key, value]) => {
    if (key === "command" && Array.isArray(value)) {
      return [value.map(String)];
    }

    if (key === "command" && typeof value === "string") {
      return [[value]];
    }

    return commandsIn(value, loginsOnly);
  });
}

// A command that starts the CLI: its name as a word of the argv, or as a word of a shell string in it.
function startsCli(command: readonly string[]): boolean {
  return command.some((part) => basename(part) === "claude" || /(^|[\s;&|(/])claude(\s|$)/.test(part));
}

// The program a command line starts, looking through whatever a launch puts in front of it — a lock,
// a timeout, an assignment — to the Claude Code or herdr executable it runs.
function programOf(command: readonly string[]): string | null {
  return command.find((part) => basename(part) === "claude" || basename(part) === "herdr") ?? null;
}

async function jobRefusal(env: Env, what: string): Promise<string | null> {
  if (!insideJob(env) || (await liveClaudeOpening(env)) !== null) {
    return null;
  }

  return [
    `refused: this process runs inside a job's Claude Code session, and a job does not ${what} before it has said what for — it writes a whole prefix to the cache again.`,
    `To open it for this job: ~/.local/share/bmo-agency/deploy/bin/bmo-agency live-claude --purpose '<what you measure and why>'. Nobody is asked; every run it lets through is recorded and the count goes into the item's report (docs/live-claude-guard.md).`,
  ].join(" ");
}

// The store a job's process reads its opening from: agency's own, wherever this process is pointed.
export async function liveClaudeArchivePlace(env: Env = process.env): Promise<string | null> {
  return storeAddress(env);
}

// The opening this job made for itself, the latest of them, or nothing: the one kept under the run's
// own name. A store that cannot be read has none.
export async function liveClaudeOpening(env: Env = process.env): Promise<number | null> {
  const run = runNameOf(env) ?? undefined;
  const place = await liveClaudeArchivePlace(env);

  if (!nonEmpty(run) || place === null) {
    return null;
  }

  try {
    const { sql, close } = openArchiveSql(place, { readonly: true });

    try {
      const row = await sql.get("SELECT id FROM live_claude_openings WHERE run = ? ORDER BY id DESC LIMIT 1", run as string) as { id: number } | null;

      return row?.id ?? null;
    } finally {
      await close();
    }
  } catch {
    return null;
  }
}

// A run the opening let through, kept against it: which line or which command, and when. Outside a
// job, or where the job made no opening, there is nothing to keep it against. A record that cannot
// be written stops nothing — the run was let through already, and the count is a record, not a
// wall.
export async function recordLiveClaudeRun(what: string, env: Env = process.env): Promise<void> {
  if (!insideJob(env)) {
    return;
  }

  const opening = await liveClaudeOpening(env);

  if (opening === null) {
    return;
  }

  try {
    const place = await liveClaudeArchivePlace(env);

    if (place === null) return;

    const { sql, close } = openArchiveSql(place);

    try {
      await sql.run("INSERT INTO live_claude_runs (opening_id, what, ran_at) VALUES (?, ?, ?)", opening, what, Date.now());
    } finally {
      await close();
    }
  } catch {
    // Kept where it can be; see above.
  }
}

// A refusal written where the test preload will read it, when this is a test.
export function recordRefusal(reason: string, env: Env = process.env): void {
  const home = testHome(env);

  if (home === null) {
    return;
  }

  try {
    appendFileSync(join(home, REFUSED_LOG), `${reason}\n`);
  } catch {
    // A home already taken away has nobody left to tell.
  }
}

function refuse(reason: string): never {
  recordRefusal(reason);

  throw new LiveClaudeRefused(reason);
}

// The command lines already kept as runs. A caller asks before it spawns and the spawn asks again on
// the same command line, which is one launch; every launch builds a command line of its own.
const recordedLaunches = new WeakSet<readonly string[]>();

// The calls every launch, every pane prompt and every call to herdr makes. Each throws where the answer
// is no.
export async function guardClaudeLaunch(line: string, command: readonly string[], commandEnv?: Env): Promise<void> {
  // A program on the registered models' road is that road's, which Claude Code being off does not
  // close; an endpoint account's program is the Claude Code road's and is closed with it.
  if (programOf(command) !== null && !registeredRoad(commandEnv ?? {}) && !await configuredProviderEnabled("claude-code")) {
    throw new SelectedRouteUnavailableError(line, "claude-code", queueWords.pool.off);
  }
  const reason = await launchRefusal(line, command, { commandEnv });

  if (reason !== null) {
    refuse(reason);
  }

  const program = programOf(command);

  if (testHome() === null && program !== null && basename(program) === "claude" && sessionLines.has(line) && !recordedLaunches.has(command)) {
    recordedLaunches.add(command);
    await recordLiveClaudeRun(`${line} line: ${command.join(" ").slice(0, 200)}`);
  }
}

export async function guardHerdrCall(method: string, params: unknown): Promise<void> {
  // A program on the registered models' road is that road's, which Claude Code being off does not close.
  if (commandsIn(params, true).some(startsCli) && !await configuredProviderEnabled("claude-code")) {
    throw new SelectedRouteUnavailableError(jobWorkKey(), "claude-code", queueWords.pool.off);
  }
  const reason = await herdrCallRefusal(method, params);

  if (reason !== null) {
    refuse(reason);
  }
}

// A pane holding another provider's program is still a pane a job does not put words into, but no
// Claude session is written by it: Claude being off does not refuse it, and nothing is recorded as
// a Claude run.
export async function guardPanePrompt(options: { claude?: boolean } = {}): Promise<void> {
  const claude = options.claude ?? true;

  if (claude && !await configuredProviderEnabled("claude-code")) throw new SelectedRouteUnavailableError(jobWorkKey(), "claude-code", queueWords.pool.off);
  const reason = await promptRefusal();

  if (reason !== null) {
    refuse(reason);
  }

  if (claude && testHome() === null) {
    await recordLiveClaudeRun("words put into a pane's session");
  }
}

