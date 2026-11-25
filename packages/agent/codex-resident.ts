import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

import declaredCodex from "@agency/policy/providers/codex.toml";
import declaredTurn from "@agency/policy/resident-turn.toml";

import { processBirth } from "./agent-host";
import { agentPlace, beginPlaceRun, endPlaceRun, residentEnvironment, runVariables } from "./agent-place";
import { codexResidentHome, codexSessionsRoot, codexUserHome, prepareCodexHome } from "./codex-local";
import { beginExecution, executionCancelled, executionRoot } from "./execution-identity";
import { herdrCall, RUN_NAME_VARIABLE } from "./herdr";
import { guardPanePrompt } from "./live-claude-guard";
import { PaneTransportError } from "./pane-process";
import { providerSettingsArchive } from "./provider-order";
import { adoptSession, interruptProgram, programAlive, readRunning, residentCommand, standResident, type ResidentProgram } from "./resident-pane";
import { herdrSilent, publishEnvelope, runHeader, screenOf, statusOf, type PaneTurnEnd } from "./worker-pane";

import type { ResidentSurface, ResidentTurnRequest, ResidentTurnResult } from "./harness";

// One turn of Codex's interactive program standing in its agent's pane (docs/agent-control.md, 「One
// standing pane per agent」). The program is stood up on the command the turn needs, the turn goes in
// as a person would type it, and it is read back out of the thread's rollout until the task it started
// has completed or been aborted (「The interactive surface a provider declares」).

// How often the program is looked at, the pace every resident turn keeps. Nothing about the turn is
// bounded by a clock: it waits for the program to come up and for its task to end, however long each
// takes.
const pollMs: number = declaredTurn.poll_ms;
// The folder trust question, which herdr reads as idle (measured): only the screen says it stands.
const trustQuestion = declaredCodex.resident.trust_question as string;

// The rollout of one thread, wherever the day it began filed it.
function rolloutOf(root: string, thread: string): string | null {
  if (!/^[a-zA-Z0-9-]+$/.test(thread)) return null;

  try {
    const found = [...new Bun.Glob(`**/rollout-*${thread}.jsonl`).scanSync({ cwd: root, absolute: true })];

    return found.length === 1 ? found[0]! : null;
  } catch {
    return null;
  }
}

// Every rollout written in the newest few days, which is where the rollout of a thread opened now is.
function recentRollouts(root: string, days: number = 2): string[] {
  const entries = (path: string): string[] => {
    try {
      return readdirSync(path).sort().reverse();
    } catch {
      return [];
    }
  };
  const found: string[] = [];
  let seen = 0;

  for (const year of entries(root)) {
    for (const month of entries(join(root, year))) {
      for (const day of entries(join(root, year, month))) {
        if (seen >= days) return found;

        seen += 1;
        found.push(...entries(join(root, year, month, day)).filter((name) => name.startsWith("rollout-")).map((name) => join(root, year, month, day, name)));
      }
    }
  }

  return found;
}

function rolloutLines(path: string): unknown[] {
  let text: string;

  try {
    text = readFileSync(path, "utf8");
  } catch {
    return [];
  }

  return text.split("\n").flatMap((line) => {
    if (line.trim() === "") return [];

    try {
      return [JSON.parse(line) as unknown];
    } catch {
      return [];
    }
  });
}

// The rollout of the thread a program opened for itself: one that was not there before the prompt,
// begun in the directory the program stands in.
function openedRollout(root: string, before: ReadonlySet<string>, cwd: string, since: number): string | null {
  for (const path of recentRollouts(root)) {
    if (before.has(path)) continue;

    try {
      if (statSync(path).mtimeMs < since - 1_000) continue;
    } catch {
      continue;
    }

    const meta = rolloutLines(path).find((line) => (line as { type?: unknown }).type === "session_meta") as { payload?: { cwd?: unknown } } | undefined;

    if (meta?.payload?.cwd === cwd) return path;
  }

  return null;
}

export async function residentCodexTurn(options: ResidentTurnRequest & {
  // The surface this turn runs on, handed in by that surface's own `turn`.
  surface: ResidentSurface;
  pollMs?: number;
  // The two Codex homes, where they are not the machine's own.
  codexHome?: string;
  userHome?: string;
}): Promise<ResidentTurnResult> {
  const archive = providerSettingsArchive();

  if (!archive) throw new Error("execution archive unavailable");

  const where = { socket: options.socket };
  const surface = options.surface;
  const run = options.run;
  const place = agentPlace(options.home.agent);
  const codexHome = options.codexHome ?? codexResidentHome();
  const root = codexSessionsRoot(codexHome);
  const every = options.pollMs ?? pollMs;

  prepareCodexHome(run.cwd, codexHome, options.userHome ?? codexUserHome());

  const command = residentCommand(await surface.command(run));
  // The program stands across many runs, so it is started without this run's own variables and with
  // the place its processes read them from; the run puts them there before its prompt.
  const standing = { ...command, env: residentEnvironment({ ...command.env, CODEX_HOME: codexHome }, place) };
  // The run's own variables, off the run itself: the command the program stands on carries none.
  const given = Object.fromEntries(Object.entries(run.env).filter((entry): entry is [string, string] => entry[1] !== undefined));
  const own = { ...runVariables(given), [RUN_NAME_VARIABLE]: options.name };
  const program: ResidentProgram = {
    identity: surface.identity({ ...standing, stdin: null }),
    command: standing,
    restart: run.resumeSessionId === null ? null : standing,
  };
  const execution = await beginExecution(archive, "codex", run.line, {
    root: options.root ?? executionRoot(options.name), parent: options.parent ?? options.name,
    model: run.model, effort: run.effort || null, sessionId: run.resumeSessionId, panePath: true,
  });
  const owner = { pid: process.pid, birth: processBirth(process.pid) };
  const cancelled = () => executionCancelled(archive.settings(), [options.name, execution.id, execution.root, execution.parent]);
  let ending: PaneTurnEnd | "launch-error" = "launch-error";

  publishEnvelope(execution.id, { owner });
  await archive.executions().saveExecution(execution);

  try {
    // A turn stopped while its program comes up ends there, before anything goes in.
    const stoppedBefore = (): ResidentTurnResult => {
      ending = "cancelled";

      return { error: "the turn was stopped before it went in" };
    };
    const stopping = new Error("stopped before the prompt");
    const stood = await standResident({
      place, home: options.home, program, line: run.line, socket: options.socket,
      check: async () => {
        if (await cancelled()) throw stopping;
      },
    }).catch((error: unknown) => {
      if (error === stopping) return null;
      throw error;
    });

    if (stood === null) return stoppedBefore();

    if (!stood.ok) return { error: `codex's program did not stand: ${stood.error.code}: ${stood.error.message}` };

    const pane = stood.result.paneId;

    execution.paneId = pane;

    // Ready once herdr reads its input box — idle, or done after a turn that ended, failed or was
    // interrupted — and the screen is not the trust question herdr reads the same way. It is waited for
    // until then, until it stands on a question, or until its pane is gone.
    for (;;) {
      if (await cancelled()) return stoppedBefore();

      const now = await statusOf(pane, where);

      if (now === null) throw new PaneTransportError("codex's pane went away before its turn");

      // Herdr answering none of the looks says nothing of the program, and never that a person is
      // asked something: the wait goes on until herdr answers.
      if (now === herdrSilent) {
        await Bun.sleep(every);
        continue;
      }

      if (now === "idle" || now === "done") {
        const screen = await screenOf(pane, where);

        if (!screen.replace(/\s+/g, " ").includes(trustQuestion)) break;

        ending = "cancelled";

        return { error: `codex's program is waiting on a person: ${screen}` };
      }

      if (now === "blocked") {
        ending = "cancelled";

        return { error: `codex's program is waiting on a person: ${await screenOf(pane, where)}` };
      }

      await Bun.sleep(every);
    }

    const started = readRunning(place);
    let path = run.resumeSessionId === null ? null : rolloutOf(root, run.resumeSessionId);
    let from = path === null ? 0 : rolloutLines(path).length;
    const before = new Set(path === null ? recentRollouts(root) : []);
    const promptedAt = Date.now();
    const setResident = () => {
      execution.resident = { place, record: path ?? "", from, pid: started?.pid ?? null, birth: started?.birth ?? null };
    };

    if (await cancelled()) {
      ending = "cancelled";

      return { error: "the turn was stopped before it went in" };
    }

    setResident();
    beginPlaceRun(place, options.name, options.title, promptedAt, own);

    await guardPanePrompt({ claude: false });
    const sent = await herdrCall("agent.prompt", { target: pane, text: `${runHeader(options.name, options.title, promptedAt)}\n\n${run.prompt}` }, where);

    if (!sent.ok) {
      ending = "cancelled";

      return { error: `codex's program would not take the prompt: ${sent.error.code}: ${sent.error.message}` };
    }

    execution.state = "running";
    await archive.executions().saveExecution(execution);

    let reading = surface.readTurn({ lines: [] }, run);

    for (;;) {
      if (await cancelled()) {
        await interruptProgram(place, pane, surface.interrupt, where);
        ending = "cancelled";

        return { error: "the turn was stopped" };
      }

      const now = await statusOf(pane, where);

      if (now === null) throw new PaneTransportError("codex's pane went away during the turn");

      // A herdr that does not answer is waited out while the rollout is read on, until it answers; a
      // silence ends nothing.
      const alive = programAlive(place);

      if (alive === null || (started !== null && alive.pid !== started.pid)) throw new PaneTransportError("codex's program went away during the turn");

      if (path === null) {
        path = openedRollout(root, before, run.cwd, promptedAt);
        from = 0;

        if (path !== null) {
          setResident();
          await archive.executions().saveExecution(execution);
        }
      }

      if (path !== null) {
        reading = surface.readTurn({ lines: rolloutLines(path).slice(from) }, run);

        if (reading.state !== "working") break;
      }

      await Bun.sleep(every);
    }

    ending = reading.state === "interrupted" ? "cancelled" : "exited";

    // A thread the program opened for itself is its thread from now on: the program standing is
    // relabelled as the one on it, so the next turn resuming it finds it rather than restarting it.
    const thread = reading.reply.sessionId;

    if (run.resumeSessionId === null && thread !== null && thread !== undefined) {
      const resumed = residentCommand({ ...await surface.command({ ...run, resumeSessionId: thread }), env: standing.env });

      adoptSession(place, program.identity, { identity: surface.identity({ ...resumed, stdin: null }), command: resumed, restart: resumed });
    }

    if (reading.state === "interrupted") return { error: "the turn was interrupted" };
    if (reading.error !== null) return { error: reading.error };
    if (reading.reply.text === "") return { error: "codex wrote no answer" };

    return { reply: reading.reply };
  } catch (error) {
    ending = ending === "launch-error" && execution.paneId !== null ? "cancelled" : ending;
    throw error;
  } finally {
    endPlaceRun(place);
    execution.state = ending;
    execution.endedAt = Date.now();
    await archive.executions().saveExecution(execution);
    publishEnvelope(execution.id, { owner, cause: ending });
  }
}
