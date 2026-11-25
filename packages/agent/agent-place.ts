import { appendFileSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";

import { agentVerbs } from "./verbs";
import { dataRoot, deployedTree, PLACE_VARIABLE } from "@agency/support/roots";

import type { ProgramLogin } from "./program-login";

// An agent's place: what agency reads of its resident program while that program stays up
// (docs/agent-control.md, 「What a resident turn reads」). The program itself writes into it through
// the status-line command and the hooks its settings name, and the bot writes one thing into it — the
// run it is putting in — so the line at the foot of the program's screen can say what is going on.
//
// Three files. The status readings, one line per call the program made, each with the moment it was
// made. The hook calls, the same way. The run: the one the bot put in last, or a person's turn, with
// when it started and when it ended. Beside them, while the bot is handing its run a word, a mark
// saying so (`handPlaceRun`).

export function agentPlace(agent: string, root: string = dataRoot()): string {
  const directory = /^[a-zA-Z0-9_-]{1,64}$/.test(agent) ? agent : `h-${createHash("sha256").update(agent).digest("hex").slice(0, 16)}`;

  return join(root, "agent-places", directory);
}

const statusFile = (place: string) => join(place, "status.jsonl");
const hooksFile = (place: string) => join(place, "hooks.jsonl");
const runFile = (place: string) => join(place, "run.json");
const handedFile = (place: string) => join(place, "handed");

export const settingsFileName = "settings.json";

function statusScriptRoot(): string {
  return deployedTree(agentVerbs().statusLine[0]);
}

// The run standing in the place. A person's turn has no name of the bot's.
export interface PlaceRun {
  name: string | null;
  title: string | null;
  person: boolean;
  startedAt: number;
  endedAt: number | null;
  // The run's own variables, which a program standing across many runs cannot carry in its own
  // environment (`turnVariables` below).
  env?: Record<string, string>;
}

export interface StatusRecord {
  at: number;
  payload: unknown;
  // The login of the program that made the call, as its own environment named it. Every login that
  // ever stands in the place writes into the same file, so this is what says whose reading a line
  // is; a line written before records carried it names nobody.
  login?: ProgramLogin;
}

// A `held` call is a stop the hook answered with a block: the program went on in the same turn. A
// `failure` is the end a failed request gave a turn in place of its stop.
export type HookKind = "stop" | "failure" | "submit" | "start" | "held";

// Whether a hook call of this kind ends a turn: a stop, or the failure a failed request ended it with.
export function endsTurn(kind: HookKind): boolean {
  return kind === "stop" || kind === "failure";
}

export interface HookRecord {
  at: number;
  kind: HookKind;
  payload: unknown;
}

function readLines<T>(path: string): T[] {
  if (!existsSync(path)) return [];

  return readFileSync(path, "utf8")
    .split("\n")
    .filter((line) => line.trim() !== "")
    .flatMap((line) => {
      try {
        return [JSON.parse(line) as T];
      } catch {
        return [];
      }
    });
}

export function readRun(place: string): PlaceRun | null {
  try {
    return JSON.parse(readFileSync(runFile(place), "utf8")) as PlaceRun;
  } catch {
    return null;
  }
}

function writeRun(place: string, run: PlaceRun): void {
  mkdirSync(place, { recursive: true });
  const temporary = `${runFile(place)}.${process.pid}`;

  writeFileSync(temporary, `${JSON.stringify(run)}\n`);
  renameSync(temporary, runFile(place));
}

// The bot names the run it is about to put in, before it puts it in, so the prompt the program takes
// is already the bot's by the time its hook asks whose it is.
export function beginPlaceRun(place: string, name: string, title: string, at: number = Date.now(), env: Record<string, string> = {}): void {
  writeRun(place, { name, title, person: false, startedAt: at, endedAt: null, env });
}

// The bot closes its run once it has read the turn's end — the only close an interrupted turn gets,
// since an Escape fires no hook.
export function endPlaceRun(place: string, at: number = Date.now()): void {
  const run = readRun(place);

  if (run !== null && run.endedAt === null) writeRun(place, { ...run, endedAt: at });
}

// The bot marks its run as handed a word just before typing it into the program, and takes the mark
// back where it does not type it after all (docs/agent-control.md, 「Words that arrive while a turn is
// working」). The word can land after the turn's end has closed the run, and its submit is the bot's
// all the same. The next submit takes the mark away; a person's turn is never marked. The mark is a
// file of its own, so the bot never writes the run while a hook may be writing its end.
export function handPlaceRun(place: string, handing: boolean = true): void {
  if (!handing) {
    rmSync(handedFile(place), { force: true });
    return;
  }

  const run = readRun(place);

  if (run === null || run.person) return;

  writeFileSync(handedFile(place), `${Date.now()}\n`);
}

function handedPending(place: string): boolean {
  return existsSync(handedFile(place));
}

// One call of the program's, as its hook or status-line command receives it. A prompt submitted while
// no run of the bot's is going, and no word of the bot's is still to arrive, is a person's turn; a
// submit the bot marked for opens its run again where a turn's end had closed it, and takes the mark
// away. A turn's end, a stop or a failure, closes whatever run is going, except a
// bot's run whose round still owes a turn — a word handed to the program after its last tool call opens
// a turn of its own, and the round is not over until that turn is (`keepsRun`). What the program handed
// over is kept whole beside the moment it arrived: this writer stands in for the hook command in tests, and
// the records a deployment keeps are that command's, which reads them for secrets (docs/agent-control.md,
// 「A record holds no secret」).
export function recordCall(
  place: string,
  kind: HookKind | "status",
  payload: unknown,
  at: number = Date.now(),
  options: { keepsRun?: boolean; login?: ProgramLogin } = {},
): void {
  mkdirSync(place, { recursive: true });

  if (kind === "status") {
    const record: StatusRecord = options.login === undefined ? { at, payload } : { at, payload, login: options.login };

    appendFileSync(statusFile(place), `${JSON.stringify(record)}\n`);
    return;
  }

  appendFileSync(hooksFile(place), `${JSON.stringify({ at, kind, payload } satisfies HookRecord)}\n`);
  const run = readRun(place);

  if (kind === "submit" && personSubmits(place)) {
    writeRun(place, { name: null, title: null, person: true, startedAt: at, endedAt: null });
  } else if (kind === "submit" && run !== null && !run.person && run.endedAt !== null) {
    writeRun(place, { ...run, endedAt: null });
  } else if (endsTurn(kind) && run !== null && run.endedAt === null && !(options.keepsRun === true && !run.person)) {
    writeRun(place, { ...run, endedAt: at });
  }

  if (kind === "submit") handPlaceRun(place, false);
}

// The variable a resident program names its place in stands with the other environment names
// (`roots.ts`).
export { PLACE_VARIABLE };

// The variables that belong to one run rather than to the program: the run's name, the job runner's
// marker, whether the run is a middle step of a work order, and every variable under the turn prefix,
// which is where a launch names what else its run is about. A resident program is started without
// them, and what it runs finds them in its place under the run that is going.
export const turnVariables = ["BMO_RUN_NAME", "BMO_TASK_LOCKED", "BMO_WORK_ORDER_STEP"] as const;
export const turnPrefix = "BMO_TURN_";

export function isTurnVariable(name: string): boolean {
  return (turnVariables as readonly string[]).includes(name) || name.startsWith(turnPrefix);
}

// Every variable of the run's own taken out of an environment.
export function withoutTurnVariables<T extends string | undefined>(env: Record<string, T>): Record<string, T> {
  return Object.fromEntries(Object.entries(env).filter(([name]) => !isTurnVariable(name)));
}

type Env = Record<string, string | undefined>;

// A process's environment with its run's own variables in it. A process a resident program started
// takes them from the run its place says is going — none for a person's turn — and every other
// process has them in its own environment already.
export function turnEnvironment(env: Env = process.env): Env {
  const place = env[PLACE_VARIABLE];

  if (place === undefined || place === "") return env;

  const run = readRun(place);
  const own: Env = withoutTurnVariables(env);

  return run === null || run.person ? own : { ...own, ...(run.env ?? {}) };
}

// A resident program's own environment: the pane's, without the variables of the run that happened
// to start it, and with the place it answers to.
export function residentEnvironment(env: Record<string, string>, place: string): Record<string, string> {
  return { ...withoutTurnVariables(env), [PLACE_VARIABLE]: place };
}

// The run's own variables out of a pane's environment, for the place to hold while the run goes on.
export function runVariables(env: Record<string, string>): Record<string, string> {
  return Object.fromEntries(Object.entries(env).filter(([name]) => isTurnVariable(name)));
}

// The designated agent a process is working for, as the run that started it names it under the turn
// prefix: whose memories, notes, reminders and watches its writes are (docs/agent-ownership.md). Null
// where no agent's run is going, which a writer refuses rather than filing under a default.
export function runAgentOf(env: Env = process.env): string | null {
  const named = turnEnvironment(env).BMO_TURN_AGENT?.trim();

  return named === undefined || named === "" ? null : named;
}

// The name of the run a process is working for.
export function runNameOf(env: Env = process.env): string | null {
  const named = turnEnvironment(env).BMO_RUN_NAME;

  return named === undefined || named === "" ? null : named;
}

export function readStatus(place: string): StatusRecord[] {
  return readLines<StatusRecord>(statusFile(place));
}

export function readHooks(place: string): HookRecord[] {
  return readLines<HookRecord>(hooksFile(place));
}

// The two status readings either side of a turn: the last one made before it went in, and the first
// one stamped after it ended, with the login of the program that made that one. A reading stamped in
// the millisecond the turn ended may have been made before the end — the call as the program came
// up, which the launcher waits for before putting the prompt in, or a call inside the turn — so it
// is never the one after. The program makes the one after about a second after the end, so the
// reader waits for it a bounded time and goes without where it does not come. Named a session, only that session's readings count: the place is the agent's, not
// the session's, and a line another session wrote into it says nothing about this turn.
export interface StatusAround {
  before: unknown;
  after: unknown;
  login: ProgramLogin | null;
}

function sessionOf(record: StatusRecord): unknown {
  return (record.payload as { session_id?: unknown } | null)?.session_id;
}

export function statusAround(records: readonly StatusRecord[], startedAt: number, endedAt: number, sessionId: string | null = null): StatusAround {
  const own = sessionId === null ? records : records.filter((record) => sessionOf(record) === sessionId);
  const after = own.find((record) => record.at > endedAt);

  return {
    before: own.findLast((record) => record.at <= startedAt)?.payload ?? null,
    after: after?.payload ?? null,
    login: after?.login ?? null,
  };
}

// The status-line readings around a turn, the one after its end waited for until it comes or the program
// that would make it is no longer alive (`standing`), whichever is first.
export async function statusAfterTurn(
  place: string,
  startedAt: number,
  endedAt: number,
  options: { standing: () => boolean; pollMs?: number; sessionId?: string | null },
): Promise<StatusAround> {
  for (;;) {
    const around = statusAround(readStatus(place), startedAt, endedAt, options.sessionId ?? null);

    if (around.after !== null || !options.standing()) return around;

    await Bun.sleep(options.pollMs ?? 200);
  }
}

// The settings a resident program is started with: the status-line command and the three hooks, each
// the deployment's status command told which place to write into, merged into whatever settings the
// line already hands its program so a hook the line had keeps standing beside these. Written into the
// place and named back.
//
// The command is the deployed tree's wherever one is set up, like a head's, so a program started from
// a job's checkout and one started by the bot are handed the same settings. A line that asks for it has
// its stop hook hold a turn's end while the turn's background work runs (`holdFlag`), while the
// turn's final answer is a tool call written as text (`textToolCallHoldFlag`), or while the turn's
// final answer uses a word the glossary bans (`wordingHoldFlag`). A line that composes its
// typed lines has its submit hook hand a person's line that composition (`typedFlag`), given the
// time the composing takes.
export function writePlaceSettings(
  place: string,
  base: Record<string, unknown> = {},
  root: string = statusScriptRoot(),
  policy: { holdForTasks?: boolean; holdForTextToolCall?: boolean; holdForWording?: boolean; composeTyped?: boolean } = {},
): string {
  const [binary, ...verb] = agentVerbs().statusLine;
  const call = (kind: string, ...flags: string[]) => [join(root, binary), ...verb, kind, "--place", place, ...flags].map((word) => `'${word.replaceAll("'", "'\\''")}'`).join(" ");
  const submit = policy.composeTyped === true
    ? { type: "command", command: call("submit", typedFlag), timeout: typedHookSeconds }
    : { type: "command", command: call("submit") };
  const own: Record<string, unknown[]> = {
    Stop: [{ hooks: [{ type: "command", command: call("stop", ...(policy.holdForTasks === true ? [holdFlag] : []), ...(policy.holdForTextToolCall === true ? [textToolCallHoldFlag] : []), ...(policy.holdForWording === true ? [wordingHoldFlag] : [])) }] }],
    // A turn a failed request ended fires this in place of the stop, and the program reads nothing it answers.
    StopFailure: [{ hooks: [{ type: "command", command: call("failure") }] }],
    UserPromptSubmit: [{ hooks: [submit] }],
    SessionStart: [{ hooks: [{ type: "command", command: call("start") }] }],
  };
  const baseHooks = (typeof base.hooks === "object" && base.hooks !== null ? base.hooks : {}) as Record<string, unknown[]>;
  const hooks: Record<string, unknown[]> = { ...baseHooks };

  for (const [event, entries] of Object.entries(own)) {
    hooks[event] = [...(hooks[event] ?? []), ...entries];
  }

  const settings = { ...base, statusLine: { type: "command", command: call("status") }, hooks };
  const path = join(place, settingsFileName);

  mkdirSync(place, { recursive: true });
  writeFileSync(path, `${JSON.stringify(settings, null, 2)}\n`);

  return path;
}

// The word the stop hook is started with where its line asks for the hold.
export const holdFlag = "--hold-tasks";

// The word the stop hook is started with where its line holds a turn's final answer that is a tool call
// written as text instead of a call made.
export const textToolCallHoldFlag = "--hold-text-tool-call";

// The word the stop hook is started with where its line holds a turn's final answer to the glossary.
export const wordingHoldFlag = "--hold-wording";

// The word the submit hook is started with where its line composes a person's typed line, and how
// long the program gives that hook: the program kills a hook that outlasts its limit, so the hook is
// given the longest the program takes, a day, and the composing it waits on is never cut short.
export const typedFlag = "--compose-typed";
export const typedHookSeconds = 86_400;

// Whether a submit opens a person's turn: no run of the bot's is going in the place, and the bot has
// handed it no word still to arrive. Asked before the submit is recorded, since recording it opens
// that turn.
export function personSubmits(place: string): boolean {
  const run = readRun(place);

  return run === null || (run.endedAt !== null && !handedPending(place));
}
