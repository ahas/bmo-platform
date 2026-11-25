import "@agency/test-support/setup";

import { mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { afterAll, describe, expect, test } from "bun:test";

import { agentPlace, readRun, runNameOf, turnEnvironment } from "@agency/agent/agent-place";
import { Archive } from "@agency/archive/archive";
import { addClaudeAccount, SEED_ACCOUNT_ID } from "@agency/agent/claude-account";
import { executionCancelKey } from "@agency/agent/execution-identity";
import { answerRecord, standInHerdr } from "@agency/test-support/herdr-standin";
import { AGENT_BLOCKED } from "@agency/agent/herdr";
import { PANE_HEAD, stopInstruction, type AgentHome } from "@agency/agent/herdr-run";
import { installProviderSettings } from "@agency/agent/provider-order";
import { readProgram, readRunning } from "@agency/agent/resident-pane";
import { codeRoot } from "@agency/support/roots";
import { scratchDir } from "@agency/test-support/scratch";
import { toolHostStandIn } from "@agency/test-support/tool-host-stand-in";
import { headCommand } from "@agency/agent/units";
import { runTurnInPane } from "@agency/agent/worker-pane";

import type { ConfirmAnswer, PaneConfirmation } from "@agency/agent/pane-confirm";
import type { RunLog } from "@agency/agent/runlog";
import { scratchArchive, scratchDatabase } from "@agency/test-support/store";

type PaneTurnConfirm = (asked: PaneConfirmation, answeredElsewhere: AbortSignal) => Promise<ConfirmAnswer>;

// A standing worker's turn goes into the program standing in its pane rather than a pane of its own
// (docs/agent-control.md, 「One standing pane per agent」): the program is started for the first turn
// and found again by the next, replaced only when the command it needs is another, never closed, and
// told to stop before it is cut short. The stand-in herdr plays the pane's loop.

const root = scratchDir("bmo-resident-turn-");
const realDataDir = process.env.BMO_DATA_DIR;
const sessionId = "5b1d5a1c-0000-4000-8000-000000000001";

process.env.BMO_DATA_DIR = root;

// A chat round asks the tool host for its program's token before it writes the program.
const toolHost = toolHostStandIn(root);

afterAll(() => {
  toolHost.stop();
  if (realDataDir === undefined) delete process.env.BMO_DATA_DIR;
  else process.env.BMO_DATA_DIR = realDataDir;

  rmSync(root, { recursive: true, force: true });
});

const home = (agent: string): AgentHome => ({ agent, label: agent, head: { command: headCommand(agent, codeRoot), cwd: root, env: {} } });

const turn = (
  agent: string,
  socket: string,
  options: { model?: string; resume?: boolean; name?: string; log?: RunLog; confirm?: PaneTurnConfirm; env?: Record<string, string>; prompt?: string } = {},
) =>
  runTurnInPane({
    name: options.name ?? "bmo-job-1",
    title: "#1 work",
    home: home(agent),
    cwd: root,
    command: ["claude", "--model", options.model ?? "claude-opus-5", options.resume === true ? "--resume" : "--session-id", sessionId],
    env: { PATH: "/usr/bin:/bin", BMO_RUN_NAME: options.name ?? "bmo-job-1", BMO_TASK_LOCKED: "1", ...options.env },
    prompt: options.prompt ?? "the work",
    sessionId,
    from: 0,
    pollMs: 5,
    log: options.log,
    confirm: options.confirm,
    socket,
  });

describe("a standing worker's turns", () => {
  test("go into one program, found again by the next turn and replaced when its command changes", async () => {
    const archive = await scratchArchive();
    const herdr = standInHerdr("resident-turns", { answer: async () => [answerRecord({ text: "done", input: 5 })], cost: 0.02 });
    const agent = "resident-worker";
    const place = agentPlace(agent);

    installProviderSettings(archive);

    try {
      const first = await turn(agent, herdr.socket);

      expect(first.outcome).toBe("answered");
      expect(first.outcome === "answered" ? first.reply.text : "").toBe("done");

      const program = readProgram(place);

      // The program is the turn's own command; started again it resumes the session it opened.
      expect(program?.command.args).toContain("--session-id");
      expect(program?.restart?.args).toEqual(["claude", "--model", "claude-opus-5", "--resume", sessionId]);
      // Its environment is the pane's without the run's own variables, which the place holds instead.
      expect(program?.command.env.BMO_AGENT_PLACE).toBe(place);
      expect(program?.command.env.BMO_RUN_NAME).toBeUndefined();
      expect(readRun(place)).toMatchObject({ name: "bmo-job-1", env: { BMO_RUN_NAME: "bmo-job-1", BMO_TASK_LOCKED: "1" } });
      expect(readRun(place)?.endedAt).not.toBeNull();
      expect(runNameOf({ BMO_AGENT_PLACE: place })).toBe("bmo-job-1");
      expect(turnEnvironment({ BMO_AGENT_PLACE: place }).BMO_TASK_LOCKED).toBe("1");

      // Never a pane of the turn's own, and never a pane closed: the head is the one pane.
      expect(herdr.asked.filter((one) => ((one.params.root ?? {}) as { command?: string[] }).command?.some((part) => part.endsWith(PANE_HEAD)) === true)).toHaveLength(1);
      expect(herdr.asked.some((one) => ((one.params.root ?? {}) as { command?: string[] }).command?.includes("--session-id") === true)).toBe(false);
      expect(herdr.asked.some((one) => one.method === "pane.close")).toBe(false);
      // The prompt's first line is typed and names the run, and says whose words the paste is; the job's
      // own words are the paste (「The order for putting in one turn」).
      expect(String(herdr.asked.find((one) => one.method === "pane.send_text")?.params.text)).toMatch(/^\[bmo-job-1 · #1 work · \d\d:\d\d\] The text pasted after this line .*on the owner's behalf\. $/);
      expect(String(herdr.asked.find((one) => one.method === "agent.prompt")?.params.text)).toBe("the work");

      const pid = readRunning(place)?.pid;
      const second = await turn(agent, herdr.socket, { resume: true, name: "bmo-job-2" });

      // The same program: nothing restarted, and the turn's money is the running total's difference.
      expect(readRunning(place)?.pid).toBe(pid);
      expect(second.outcome === "answered" ? second.reply.spend[0]?.cost : null).toBeCloseTo(0.02, 10);

      const third = await turn(agent, herdr.socket, { resume: true, model: "claude-sonnet-5", name: "bmo-job-3" });

      expect(third.outcome).toBe("answered");
      expect(readRunning(place)?.pid).not.toBe(pid);
      expect(readProgram(place)?.restart?.args).toContain("claude-sonnet-5");
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);

  test("told to stop, the turn is asked to end its work and waited on until it comes to rest, with nothing cut behind the words", async () => {
    const archive = await scratchArchive();
    const herdr = standInHerdr("resident-stop", {
      // Told its work, it is stopped from outside while it keeps working, and it works on a while
      // after the stop words before it comes to rest.
      answer: async ({ text }) => {
        if (text.endsWith("the work")) await archive.settings().setSetting(executionCancelKey("bmo-job-stop"), "1");
        return [];
      },
      after: (text) => (text.endsWith(stopInstruction) ? [...Array<string>(40).fill("working"), "done"] : ["working"]),
    });

    installProviderSettings(archive);

    try {
      const stopped = await turn("stopping-worker", herdr.socket, { name: "bmo-job-stop" });
      const prompts = herdr.asked.filter((one) => one.method === "agent.prompt").map((one) => String(one.params.text));

      expect(stopped.outcome).toBe("stopped");
      expect(prompts.at(-1)).toBe(stopInstruction);
      expect(herdr.asked.some((one) => one.method === "pane.send_keys")).toBe(false);
      expect(herdr.asked.some((one) => one.method === "pane.close")).toBe(false);
      expect(readRun(agentPlace("stopping-worker"))?.endedAt).not.toBeNull();
      // The row says it was a turn of a resident program, and closes as a stop once the program came
      // to rest on its own.
      const row = (await archive.executions().executions()).find((one) => one.parent === "bmo-job-stop");

      expect(row).toMatchObject({ state: "cancelled", resident: { place: agentPlace("stopping-worker"), from: 0 } });
      expect(row?.endedAt).not.toBeNull();
      expect(row?.resident?.pid).toBe(readRunning(agentPlace("stopping-worker"))?.pid ?? -1);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);

  // A notice queued twice under one task id is kept once and taken off once, so the count still
  // says a word is queued when none is (docs/agent-control.md, 「Words that arrive while a turn is working」).
  test("a round whose count still expects a word is over once the program rests on two looks with the turn's end its last line", async () => {
    const archive = await scratchArchive();
    const notice = { type: "queue-operation", operation: "enqueue", content: "<task-notification>\n<task-id>t1</task-id>\n</task-notification>" };
    const herdr = standInHerdr("resident-quiet", {
      answer: async () => [
        answerRecord({ text: "done" }),
        notice,
        notice,
        { type: "queue-operation", operation: "dequeue" },
        { type: "system", subtype: "turn_duration" },
        { type: "user", message: { role: "user", content: notice.content } },
        answerRecord({ text: "noted" }),
      ],
    });
    const notes: string[] = [];
    const log = { write: (line: { text?: string }) => notes.push(line.text ?? ""), feed: () => {} } as unknown as RunLog;

    installProviderSettings(archive);

    try {
      const ended = await turn("quiet-worker", herdr.socket, { name: "bmo-job-quiet", log });

      expect(ended.outcome).toBe("answered");
      expect(ended.outcome === "answered" ? ended.reply.text : "").toBe("done\n\nnoted");
      expect(notes.some((note) => note.includes("1 waiting, 2 turns owed, 2 turns ended"))).toBe(true);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);

  // A chat round has nothing of its own to wind down: no stop words go in, and the surface's
  // interrupt cuts it short at once.
  test("a stopped chat round is interrupted at once, with no stop words, and its program stays", async () => {
    const archive = await scratchArchive();
    const herdr = standInHerdr("resident-chat-stop", {
      answer: async ({ text }) => {
        if (text.endsWith("the work")) await archive.settings().setSetting(executionCancelKey("bmo-chat-stop"), "1");
        return [];
      },
      after: ["working"],
    });
    const place = agentPlace("stopping-chat");

    installProviderSettings(archive);

    try {
      const stopped = await runTurnInPane({
        line: "chat", name: "bmo-chat-stop", title: "a talk", home: home("stopping-chat"), cwd: root,
        command: ["claude", "--model", "claude-opus-5", "--session-id", sessionId], env: { PATH: "/usr/bin:/bin" },
        prompt: "the work", sessionId, from: 0, pollMs: 5, socket: herdr.socket,
      });
      const prompts = herdr.asked.filter((one) => one.method === "agent.prompt").map((one) => String(one.params.text));
      const pid = readRunning(place)?.pid;

      expect(stopped.outcome).toBe("stopped");
      expect(prompts).not.toContain(stopInstruction);
      expect(herdr.asked.filter((one) => one.method === "pane.send_keys").map((one) => one.params.keys)).toEqual([["escape"]]);
      expect(herdr.asked.some((one) => one.method === "pane.close")).toBe(false);
      expect((await archive.executions().executions()).find((one) => one.parent === "bmo-chat-stop")).toMatchObject({ line: "chat", state: "cancelled" });
      expect(readRunning(place)?.pid).toBe(pid);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);

  // A surface whose interrupt was never measured ends its program instead, and the loop starts it
  // again on the same session in the same pane.
  test("a stopped chat round on a surface with no measured interrupt ends its program, and the loop starts it again", async () => {
    const archive = await scratchArchive();
    const herdr = standInHerdr("resident-end-stop", {
      answer: async ({ text }) => {
        if (text.endsWith("the work")) await archive.settings().setSetting(executionCancelKey("bmo-job-end"), "1");
        return [];
      },
      after: ["working"],
    });
    const place = agentPlace("ending-worker");

    installProviderSettings(archive);

    try {
      const first = await runTurnInPane({
        line: "chat", name: "bmo-job-end", title: "a talk", home: home("ending-worker"), cwd: root,
        command: ["claude", "--model", "claude-opus-5", "--session-id", sessionId], env: { PATH: "/usr/bin:/bin" },
        prompt: "the work", sessionId, from: 0, pollMs: 5,
        interrupt: { endProgram: true }, socket: herdr.socket,
      });
      // The program the turn went into, as its row recorded it.
      const before = (await archive.executions().executions()).find((one) => one.parent === "bmo-job-end")?.resident;

      expect(first.outcome).toBe("stopped");
      expect(before?.pid).toBeNumber();
      expect(herdr.asked.some((one) => one.method === "pane.send_keys")).toBe(false);
      expect(herdr.asked.some((one) => one.method === "pane.close")).toBe(false);

      for (let look = 0; look < 200 && readRunning(place)?.pid === before?.pid; look += 1) await Bun.sleep(10);

      expect(readRunning(place)?.pid).not.toBe(before?.pid);
      expect(readRunning(place)?.starts).toBe(2);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);
});

// A prompt the pane accepted can stand pasted in the input box and never be sent, and a program at
// rest with words in its box reads exactly like one that has finished, so unless the turn looks, it
// would read the words sitting there for the next turn as a turn come back silent.
describe("a prompt that went in", () => {
  const boxedSession = "5b1d5a1c-0000-4000-8000-000000000002";
  const boxedTurn = (agent: string, socket: string) =>
    runTurnInPane({
      name: "bmo-job-boxed", title: "#3 work", home: home(agent), cwd: root,
      command: ["claude", "--model", "claude-opus-5", "--session-id", boxedSession], env: { PATH: "/usr/bin:/bin" },
      prompt: "the work", sessionId: boxedSession, from: 0, pollMs: 5, submitMs: 200, socket,
    });

  test("left unsent in the input box is sent with one enter, and the turn goes on", async () => {
    const archive = await scratchArchive();
    const herdr = standInHerdr("resident-enter", { answer: async () => [answerRecord({ text: "done", input: 5 })], submits: "on-enter" });

    installProviderSettings(archive);

    try {
      const turned = await boxedTurn("entering-worker", herdr.socket);
      const keys = herdr.asked.filter((one) => one.method === "pane.send_keys").map((one) => one.params.keys);

      expect(turned.outcome).toBe("answered");
      expect(turned.outcome === "answered" ? turned.reply.text : "").toBe("done");
      expect(keys).toEqual([["enter"]]);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);

  // herdr goes on reading a pane at rest from the program before it while the one started for the turn
  // is still coming up, and words put in then reach no input box at all.
  test("into a program started for the turn waits for it to come up, and is not lost to the one before it", async () => {
    const archive = await scratchArchive();
    const herdr = standInHerdr("resident-coming-up", { answer: async () => [answerRecord({ text: "done", input: 5 })], comesUpMs: 400 });
    const place = agentPlace("coming-up-worker");
    const upTurn = (model: string, name: string) =>
      runTurnInPane({
        name, title: "#4 work", home: home("coming-up-worker"), cwd: root,
        command: ["claude", "--model", model, "--resume", sessionId], env: { PATH: "/usr/bin:/bin" },
        prompt: "the work", sessionId, from: 0, pollMs: 5, submitMs: 200, socket: herdr.socket,
      });

    installProviderSettings(archive);

    try {
      const first = await upTurn("claude-opus-5", "bmo-job-first");
      const pid = readRunning(place)?.pid;
      // Another command: the program is ended and started again, and the pane still reads `done`.
      const second = await upTurn("claude-sonnet-5", "bmo-job-second");

      expect(readRunning(place)?.pid).not.toBe(pid);
      expect([first.outcome, second.outcome]).toEqual(["answered", "answered"]);
      expect(herdr.asked.filter((one) => one.method === "pane.send_keys")).toEqual([]);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);

  test("never sent has enter pressed again at the submit pace until the turn is stopped, and the box is emptied", async () => {
    const archive = await scratchArchive();
    const herdr = standInHerdr("resident-never", { answer: async () => [answerRecord({ text: "done", input: 5 })], submits: "never" });
    const place = agentPlace("unsent-worker");

    installProviderSettings(archive);

    try {
      const turning = boxedTurn("unsent-worker", herdr.socket);

      // No clock ends the wait: enter is pressed again and again until somebody stops the turn.
      for (let look = 0; look < 400 && herdr.asked.filter((one) => one.method === "pane.send_keys").length < 3; look += 1) await Bun.sleep(10);
      await archive.settings().setSetting(executionCancelKey("bmo-job-boxed"), "1");

      const turned = await turning;
      const before = (await archive.executions().executions()).find((one) => one.parent === "bmo-job-boxed")?.resident;

      expect(turned.outcome).toBe("stopped");
      expect(herdr.asked.filter((one) => one.method === "pane.send_keys").length).toBeGreaterThanOrEqual(3);
      expect(herdr.asked.filter((one) => one.method === "pane.send_keys").every((one) => JSON.stringify(one.params.keys) === JSON.stringify(["enter"]))).toBe(true);
      expect(herdr.asked.filter((one) => one.method === "agent.prompt")).toHaveLength(1);

      for (let look = 0; look < 200 && readRunning(place)?.pid === before?.pid; look += 1) await Bun.sleep(10);

      expect(readRunning(place)?.pid).not.toBe(before?.pid);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);
});

// A program left as it stands hands the same wedge to the next turn, and the one after it, until the
// room has gone quiet with nothing in the record saying why. The turn clears it instead: the program
// is ended, its loop stands it again on the same session with an empty box, and the turn goes in once
// more (`docs/agent-control.md`, 「When a program will not come to rest」).
describe("a program that will not come to rest", () => {
  // Wedged until it is reset: herdr reads the pane as standing on a question nobody is to answer for
  // as long as the program standing is the first one, and at rest once the loop has stood another.
  const wedgedUntilReset = (agent: string) => (): string => ((readRunning(agentPlace(agent))?.starts ?? 0) > 1 ? "idle" : "blocked");

  test("is stood again and the turn goes in once more, and the answer comes out", async () => {
    const archive = await scratchArchive();
    const agent = "wedged-worker";
    const herdr = standInHerdr("resident-wedged", { answer: async () => [answerRecord({ text: "done", input: 5 })], before: wedgedUntilReset(agent) });
    const place = agentPlace(agent);

    installProviderSettings(archive);

    try {
      const turned = await turn(agent, herdr.socket);

      expect(turned.outcome).toBe("answered");
      expect(turned.outcome === "answered" ? turned.reply.text : "").toBe("done");
      // Stood a second time, which is the reset, and told only once — the retry is the same turn.
      expect(readRunning(place)?.starts).toBe(2);
      expect(herdr.asked.filter((one) => one.method === "agent.prompt")).toHaveLength(1);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);

  test("is given one reset and one retry, never a loop, and the second failure is the turn's end", async () => {
    const archive = await scratchArchive();
    const agent = "stuck-worker";
    const herdr = standInHerdr("resident-stuck", { answer: async () => [answerRecord({ text: "done", input: 5 })], before: () => "blocked" });
    const place = agentPlace(agent);

    installProviderSettings(archive);

    try {
      const turned = await turn(agent, herdr.socket);

      expect(turned.outcome).toBe("blocked");
      // Reset once and no more, and nothing was ever put into it.
      expect(readRunning(place)?.starts).toBe(2);
      expect(herdr.asked.filter((one) => one.method === "agent.prompt")).toEqual([]);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 60_000);

  // A question is a person's to answer and the program stays up holding it. Only a turn that can put
  // one to somebody has such a question, and that is the job turn: a pane blocked in front of a round
  // that was given no way to ask is a question nobody is coming to answer.
  test("is left standing where the pane holds a question this turn can put to somebody", async () => {
    const archive = await scratchArchive();
    const agent = "asking-worker";
    const herdr = standInHerdr("resident-asking", { answer: async () => [answerRecord({ text: "done", input: 5 })], before: () => "blocked" });
    const place = agentPlace(agent);

    installProviderSettings(archive);

    try {
      const turned = await turn(agent, herdr.socket, { confirm: async () => ({ leave: true }) });

      expect(turned.outcome).toBe("blocked");
      // Stood once and never ended: whoever is to answer finds the same question where it stands.
      expect(readRunning(place)?.starts).toBe(1);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);

  // A work-order stage's prompt runs to hundreds of kilobytes, past what a socket takes in one write.
  // Written short, herdr waits for the line's end and gives up, and the turn read that as a program
  // that would not take its words.
  test("takes a prompt longer than the socket holds at once whole, and the turn is answered", async () => {
    const archive = await scratchArchive();
    const agent = "long-prompt-worker";
    const herdr = standInHerdr("resident-long", { answer: async () => [answerRecord({ text: "done", input: 5 })] });
    const prompt = "€".repeat(150_000);

    installProviderSettings(archive);

    try {
      const turned = await turn(agent, herdr.socket, { prompt });
      const told = herdr.asked.filter((one) => one.method === "agent.prompt").map((one) => one.params.text);

      expect(turned.outcome).toBe("answered");
      expect(told).toEqual([prompt]);
      expect(readRunning(agentPlace(agent))?.starts).toBe(1);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);

  // Nothing on the screen asks anybody anything, so an item held for an answer would stay held. The
  // screen is the one a work order's stage stood at four times over, recorded as it was: the program at
  // rest with the typed lead in its box and nothing pasted after it.
  test("refusing its prompt twice ends the turn unsubmitted, never as waiting on a person, and empties the box", async () => {
    const archive = await scratchArchive();
    const agent = "refusing-worker";
    const refused = { code: "closed", message: "herdr closed before answering agent.prompt" };
    const screen = await Bun.file(join(import.meta.dir, "fixtures", "panes", "prompt-refused.txt")).text();
    const herdr = standInHerdr("resident-refusing", {
      answer: async () => [answerRecord({ text: "done", input: 5 })],
      refuses: (method) => (method === "agent.prompt" ? refused : null),
      screen,
    });
    const place = agentPlace(agent);
    const notes: string[] = [];
    const log = { write: (line: { text?: string }) => notes.push(line.text ?? ""), feed: () => {} } as unknown as RunLog;

    installProviderSettings(archive);

    try {
      const turned = await turn(agent, herdr.socket, { log, confirm: async () => ({ leave: true }) });

      expect(turned.outcome).toBe("unsubmitted");
      expect(turned.outcome === "unsubmitted" ? turned.screen : "").toBe(screen);
      expect(herdr.asked.filter((one) => one.method === "agent.prompt")).toHaveLength(2);
      // What the call answered is on the log, where the cause is read.
      expect(notes.filter((one) => one.includes("closed: herdr closed before answering"))).toHaveLength(2);

      // Stood, reset once, and started again on the way out so the lead typed into it goes.
      for (let look = 0; look < 200 && (readRunning(place)?.starts ?? 0) < 3; look += 1) await Bun.sleep(10);

      expect(readRunning(place)?.starts).toBe(3);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);

  test("a prompt refused as blocked is a question left standing where the turn can put it to somebody", async () => {
    const archive = await scratchArchive();
    const agent = "prompt-asking-worker";
    const herdr = standInHerdr("resident-prompt-asking", {
      answer: async () => [answerRecord({ text: "done", input: 5 })],
      refuses: (method) => (method === "agent.prompt" ? { code: AGENT_BLOCKED, message: "requires interactive input" } : null),
    });

    installProviderSettings(archive);

    try {
      const turned = await turn(agent, herdr.socket, { confirm: async () => ({ leave: true }) });

      expect(turned.outcome).toBe("blocked");
      expect(readRunning(agentPlace(agent))?.starts).toBe(1);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);
});

// A resident turn's account windows are the login's that made the reading after it, and the reading
// says whose it is: the place is the agent's, and every login that ever stands in it writes there.
describe("a resident turn's account windows", () => {
  // Windows that reset ahead of the run whenever it runs: a window already past is dropped as stale.
  const fiveHourResets = Math.floor(Date.now() / 1000) + 3 * 3600;
  const sevenDayResets = fiveHourResets + 5 * 86_400;
  const limits = {
    five_hour: { used_percentage: 36, resets_at: fiveHourResets },
    seven_day: { used_percentage: 79, resets_at: sevenDayResets },
  };
  const second = join(root, "login-second");

  mkdirSync(second, { recursive: true });

  const archiveWithSecond = async (): Promise<Archive> => {
    const archive = await scratchArchive();

    await addClaudeAccount(archive, { id: "second", name: "Number 2", credential: { kind: "directory", path: second } });

    return archive;
  };

  test("go to the login the reading names, not the one the pane was launched on", async () => {
    const archive = await archiveWithSecond();
    // Launched on the machine's own login, while the program that made the reading ran on the second.
    const herdr = standInHerdr("resident-login-named", { answer: async () => [answerRecord({ text: "done", input: 5 })], limits, login: { configDir: second, token: null } });

    installProviderSettings(archive);

    try {
      const turned = await turn("login-named-worker", herdr.socket);

      expect(turned.outcome).toBe("answered");
      expect(turned.outcome === "answered" ? turned.reply.account : null).toBe("second");
      expect(turned.outcome === "answered" ? turned.reply.windows : []).toEqual([
        { name: "five_hour", utilization: 0.36, resetsAt: fiveHourResets },
        { name: "seven_day", utilization: 0.79, resetsAt: sevenDayResets },
      ]);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);

  test("are written under nobody where the reading names no registered login", async () => {
    const archive = await archiveWithSecond();
    const herdr = standInHerdr("resident-login-unknown", { answer: async () => [answerRecord({ text: "done", input: 5 })], limits, login: { configDir: join(root, "nobody"), token: null } });

    installProviderSettings(archive);

    try {
      const turned = await turn("login-unknown-worker", herdr.socket);
      const reply = turned.outcome === "answered" ? turned.reply : null;

      expect(reply?.windows).toEqual([]);
      expect(reply?.sessionUsage).toBeNull();
      // Its spend still goes to the login the pane was launched on, which its environment names.
      expect(reply?.account).toBe(SEED_ACCOUNT_ID);
    } finally {
      installProviderSettings(null);
      herdr.stop();
    }
  }, 30_000);
});
