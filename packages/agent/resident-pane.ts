import { chmodSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { identityAlive, type ProcessIdentity } from "./agent-host";
import { MCP_TOKEN_VARIABLE, ToolHostDown, revokeToolToken, toolTokenFor } from "./tool-host";
import { readStatus } from "./agent-place";
import { closePane, standingWorkspace, type AgentHome } from "./herdr-run";
import { aliveOnNode, checkoutShadow, placeNode, signalOnNode } from "./work-node";

import { herdrCall } from "./herdr";
import declaredTurn from "@agency/policy/resident-turn.toml";

import type { HarnessCommand, ResidentInterrupt } from "./harness";
import { programFile, readProgram, type ResidentCommand, type ResidentProgram } from "./resident-program";
import type { HerdrAnswer } from "./herdr";

// The resident program an agent's pane holds (docs/agent-control.md, 「One standing pane per agent」).
//
// The pane runs a loop and the loop runs `pane-head`, which reads the agent's place and finds either a
// program to start there or nothing, and follows the agent's view on nothing. What the program is, is
// the launcher's to say, because only the line knows what the agent's command is — the
// model, the prompt file, the session — so the launcher writes it into the place and the head starts
// what it finds. The two talk through two files, and neither reaches into the other's process:
//
// - the program: the command to start, the command to start it again on once it has been started,
//   and the identity the launcher compares before every turn;
// - the running program: which identity is standing, its process, the head holding it and how many
//   times that identity has been started.

export type { ResidentCommand, ResidentProgram };
export { readProgram };

export interface RunningProgram {
  identity: string;
  pid: number;
  birth: string | null;
  loop: ProcessIdentity;
  starts: number;
  startedAt: number;
  // The pane the program draws on, which is where a turn's words go.
  paneId?: string | null;
  // When the program was started lately, whatever its identity, which is what bounds how soon it is
  // started again.
  recent?: number[];
  // Set while the head waits out that bound before starting it: the place is held until then.
  pendingUntil?: number;
  // Set once the program started in a loop: the agent host starts it no more until the place changes.
  restingSince?: number;
}

// How far back the starts of a place are remembered.
const recentWindowMs = 5 * 60_000;
// Starts within the remembered window that make a program one in a loop.
const loopingStarts = 6;
// How often one place may be rebuilt.
const rebuildEveryMs = 60_000;

// The starts still remembered at `now`.
function recentStarts(running: RunningProgram | null, now: number = Date.now()): number[] {
  return (running?.recent ?? []).filter((at) => now - at < recentWindowMs);
}

// Whether the place's program keeps falling over and being started again, or rests after doing so.
export function startLooping(running: RunningProgram | null, now: number = Date.now()): boolean {
  return running?.restingSince !== undefined || recentStarts(running, now).length >= loopingStarts;
}

const runningFile = (place: string) => join(place, "running.json");

function readJson<T>(path: string): T | null {
  try {
    return JSON.parse(readFileSync(path, "utf8")) as T;
  } catch {
    return null;
  }
}

// Written whole and private: the command carries the login a program goes out on.
function writePrivate(path: string, value: unknown): void {
  mkdirSync(join(path, ".."), { recursive: true, mode: 0o700 });
  const temporary = `${path}.${process.pid}`;

  writeFileSync(temporary, `${JSON.stringify(value)}\n`, { mode: 0o600 });
  chmodSync(temporary, 0o600);
  renameSync(temporary, path);
}

// A program carrying its session token in its own environment, and its restart with it, so the
// pane's loop starts it again on the same record. The token is
// no part of the program's identity, which is decided before it is laid on.
export function carryToken(program: ResidentProgram, token: string | null): ResidentProgram {
  if (token === null) return program;

  const carry = (command: ResidentCommand): ResidentCommand => ({ ...command, env: { ...command.env, [MCP_TOKEN_VARIABLE]: token } });

  return { ...program, command: carry(program.command), restart: program.restart === null ? null : carry(program.restart) };
}

/**
 * The checkout a program's tool session record names: its own, or for a program on a work node the
 * shadow its link keeps of that checkout here (docs/work-nodes.md, 「The link」).
 */
export function recordCwd(place: string, cwd: string): string {
  const node = placeNode(place);

  return node === null ? cwd : checkoutShadow(node, cwd);
}

/** The session token a written program carries, or null. */
export function programToken(program: ResidentProgram | null): string | null {
  return program?.command.env[MCP_TOKEN_VARIABLE] ?? null;
}

// A token the place's program no longer carries, taken back from the tool host.
function revokeReplaced(before: ResidentProgram | null, after: ResidentProgram | null): void {
  const token = programToken(before);

  if (token !== null && token !== programToken(after)) void revokeToolToken(token);
}

// A command as it is handed to a pane: only the variables that hold a value.
export function residentCommand(command: HarnessCommand): ResidentCommand {
  const env: Record<string, string> = {};

  for (const [name, value] of Object.entries(command.env)) {
    if (value !== undefined) env[name] = value;
  }

  return { args: [...command.args], cwd: command.cwd, env };
}

export function readRunning(place: string): RunningProgram | null {
  return readJson<RunningProgram>(runningFile(place));
}

/**
 * Whether a process a place names is alive, asked of the machine it runs on (docs/work-nodes.md, 「The
 * bot's side of a remote agent」): this one's kernel for a place here, and what the link last told for a
 * place on a node. A link that has told nothing lately says nothing either way, and the process is
 * taken as standing rather than stood up again over a program that may be working.
 */
export function placeAlive(place: string): (identity: ProcessIdentity) => boolean {
  if (placeNode(place) === null) return identityAlive;

  return (identity) => aliveOnNode(place, { pid: identity.pid, birth: identity.birth || null }) ?? true;
}

/** A place's program ended on the machine it runs on: signalled here, or through the node's link. */
export function placeEnd(place: string): (pid: number) => void {
  const node = placeNode(place);

  if (node === null) return (pid) => process.kill(pid, "SIGTERM");

  return (pid) => {
    const running = readRunning(place);
    const birth = running?.pid === pid ? running.birth : running?.loop.pid === pid ? running.loop.birth : null;

    void signalOnNode(node, { pid, birth: birth ?? null });
  };
}

// The program standing in the place, or null where none is: its process alive and still the one
// the record names.
export function programAlive(place: string, alive: (identity: ProcessIdentity) => boolean = placeAlive(place)): RunningProgram | null {
  const running = readRunning(place);

  return running !== null && alive({ pid: running.pid, birth: running.birth } as ProcessIdentity) ? running : null;
}

// A program written into a place in place of whatever it holds, and the one standing ended where it
// is another, which the loop answers by starting the new one in the same pane. The new one is written
// first, so the loop finds it rather than nothing. Answers whether a standing program was ended.
export function replaceProgram(place: string, program: ResidentProgram, options: { alive?: (identity: ProcessIdentity) => boolean; end?: (pid: number) => void } = {}): boolean {
  const before = readProgram(place);
  // The same program written again because it now carries a token the one standing lacks — one
  // written before its line stood on the tool host — and started again so it reaches the host.
  const tokened = programToken(program) !== null && programToken(before) === null;

  if (before?.identity === program.identity && !tokened) return false;

  writePrivate(programFile(place), program);
  revokeReplaced(before, program);

  const running = programAlive(place, options.alive);

  if (running === null || (running.identity === program.identity && !tokened)) return false;

  (options.end ?? placeEnd(place))(running.pid);

  return true;
}

// Whether the program standing in the place has drawn itself since its loop started it: a status-line
// call written at or after that moment, which Claude Code makes once its prompt footer is up. Until then
// the pane can still read at rest from the program before it, and words put in reach no input box
// (docs/agent-control.md, 「The order for putting in one turn」).
export function programDrawn(place: string): boolean {
  const running = readRunning(place);

  return running !== null && readStatus(place).some((record) => record.at >= running.startedAt);
}

export function recordStarted(place: string, running: RunningProgram): void {
  writePrivate(runningFile(place), running);
}

// A program the head could not start at all — its executable not on the PATH it was handed, or not
// runnable. It is written into the place so the turn waiting on that program reads why as soon as it
// happens, rather than waiting out its wait and a rebuild that would fail the same way (「Standing up
// again」 in docs/agent-control.md).
export interface StartFailure {
  identity: string;
  at: number;
  reason: string;
}

const startFailedFile = (place: string) => join(place, "start-failed.json");

export function readStartFailure(place: string): StartFailure | null {
  return readJson<StartFailure>(startFailedFile(place));
}

// A program that opened a session of its own choosing is, from then on, the program on that
// session: the place is told so, the program standing is relabelled rather than started again,
// and a program that falls over later comes back resuming it. Only the standing program that
// `from` names is relabelled; one that is already another has nothing to do with this.
export function adoptSession(place: string, from: string, program: ResidentProgram): void {
  const running = readRunning(place);

  // The program standing is the same process under a new name, and goes on with the token it was
  // started with: the record carries it on rather than taking it back.
  writePrivate(programFile(place), carryToken(program, programToken(readProgram(place))));

  if (running !== null && running.identity === from) recordStarted(place, { ...running, identity: program.identity });
}

// The place torn down so it stands fresh: the program ended, its pane closed — the head and its loop
// go with it — and its start history cleared, so the pane the next standing pass or turn makes starts
// it at once, on the command that resumes its session. At most once a minute per place; answers
// whether it was done.
async function rebuildPlace(options: {
  place: string;
  home: AgentHome;
  paneId: string | null;
  alive: (identity: ProcessIdentity) => boolean;
  end: (pid: number) => void;
  close: (paneId: string) => Promise<unknown>;
}): Promise<boolean> {
  const stamp = join(options.place, "rebuilt.json");

  if (Date.now() - (readJson<{ at: number }>(stamp)?.at ?? 0) < rebuildEveryMs) return false;

  writePrivate(stamp, { at: Date.now() });

  const running = readRunning(options.place);
  const up = programAlive(options.place, options.alive);

  if (up !== null) options.end(up.pid);
  if (running !== null) recordStarted(options.place, { ...running, pid: 0, birth: null, recent: [], pendingUntil: undefined, restingSince: undefined });

  const pane = running?.paneId ?? options.paneId;

  if (pane !== null && pane !== undefined) await options.close(pane);

  console.error(`${options.home.agent}: its resident program would not stand (${running === null ? "never started" : `${recentStarts(running).length} starts in five minutes`}); the place was rebuilt`);

  return true;
}

// The launcher's side, before every turn: the program the turn needs, standing. A place already
// running that program is left as it is. A different program is written and the one standing is
// ended, which the loop answers by starting the new one in the same pane; a pane that is not there is
// stood up. Then the turn waits for the program to be up, for its start failure to be written, or for
// `check` to throw for a turn cancelled meanwhile, however long that takes. A program in a start loop
// has its place rebuilt and is waited on once more; one still in a loop after that is not up.
export async function standResident(options: {
  place: string;
  home: AgentHome;
  program: ResidentProgram;
  // The line the program runs, whose session token it is handed; none where it is handed no tools.
  line?: string;
  socket?: string;
  pollMs?: number;
  // Asked on every look while the program is waited for; a turn cancelled meanwhile throws out of it.
  check?: () => void | Promise<unknown>;
  alive?: (identity: ProcessIdentity) => boolean;
  end?: (pid: number) => void;
  close?: (paneId: string) => Promise<unknown>;
}): Promise<HerdrAnswer<{ paneId: string; restarted: boolean }>> {
  const alive = options.alive ?? placeAlive(options.place);
  const end = options.end ?? placeEnd(options.place);
  const close = options.close ?? ((paneId: string) => closePane(paneId, { socket: options.socket }));
  let program = options.program;

  // A program about to be written is handed its session token first, and a tool host that does not answer
  // refuses the turn before anything is written. A place already holding this program keeps the token it
  // stands on.
  const standing = readProgram(options.place);

  if (options.line !== undefined && (standing?.identity !== program.identity || programToken(standing) === null)) {
    try {
      program = carryToken(program, await toolTokenFor(options.line, { ...program.command, cwd: recordCwd(options.place, program.command.cwd), agent: options.home.agent }));
    } catch (error) {
      if (error instanceof ToolHostDown) return { ok: false, error: { code: "tool-host-down", message: error.message } };
      throw error;
    }
  }

  const restarted = replaceProgram(options.place, program, { alive, end });
  const rebuild = (paneId: string | null) => rebuildPlace({ place: options.place, home: options.home, paneId, alive, end, close });
  // A start failure written from now on, for this program, is the answer; one written before is a
  // head that may since have been stood again on another environment.
  const askedAt = Date.now();
  let rebuilt = startLooping(readRunning(options.place)) && await rebuild(null);

  for (;;) {
    const pane = await standingWorkspace(options.home, { socket: options.socket });

    if (!pane.ok) return pane;

    for (;;) {
      const up = programAlive(options.place, alive);

      if (up !== null && up.identity === options.program.identity) {
        return { ok: true, result: { paneId: up.paneId ?? pane.result.headPaneId, restarted } };
      }

      const failed = readStartFailure(options.place);

      if (failed !== null && failed.identity === options.program.identity && failed.at >= askedAt) {
        return { ok: false, error: { code: "program_not_started", message: `the resident program of ${options.home.agent} could not be started: ${failed.reason}` } };
      }

      // A program in a start loop is one whose own starts keep coming, never those of the program it was
      // written in place of.
      const running = readRunning(options.place);

      if (running?.identity === options.program.identity && startLooping(running)) break;

      await options.check?.();
      await Bun.sleep(options.pollMs ?? 250);
    }

    if (rebuilt || !(await rebuild(pane.result.headPaneId))) {
      return { ok: false, error: { code: "program_not_up", message: `the resident program of ${options.home.agent} keeps dying as it starts${rebuilt ? " even after its place was rebuilt" : ""}` } };
    }

    rebuilt = true;
  }
}

// A working turn of the place's program cut short by the surface's own interrupt: keys the program
// reads as an interruption, which leave its session usable, or — where no interrupt has been
// measured — the program ended, which its loop answers by starting it again on the same session. An
// ended program is waited on until its process is gone: until then the place still names it
// and reads it alive, and whatever reads the place next would take it for the one standing. Either way
// the pane stays (「Stopping, falling back and standing up again」).
export async function interruptProgram(
  place: string,
  paneId: string,
  interrupt: ResidentInterrupt,
  options: { socket?: string; alive?: (identity: ProcessIdentity) => boolean; end?: (pid: number) => void } = {},
): Promise<void> {
  if ("keys" in interrupt) {
    await herdrCall("pane.send_keys", { pane_id: paneId, keys: [...interrupt.keys] }, { socket: options.socket });

    return;
  }

  const alive = options.alive ?? placeAlive(place);
  const running = programAlive(place, alive);

  if (running === null) return;

  (options.end ?? placeEnd(place))(running.pid);

  const ended = { pid: running.pid, birth: running.birth } as ProcessIdentity;
  while (alive(ended)) await Bun.sleep(declaredTurn.end_poll_ms);
}
