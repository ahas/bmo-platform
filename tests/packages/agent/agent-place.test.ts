import "@agency/test-support/setup";

import { readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { afterAll, describe, expect, test } from "bun:test";

import { beginPlaceRun, endPlaceRun, holdFlag, readHooks, readRun, readStatus, recordCall, residentEnvironment, runVariables, statusAfterTurn, statusAround, turnEnvironment, typedFlag, typedHookSeconds, wordingHoldFlag, writePlaceSettings, type HookRecord, type StatusRecord } from "@agency/agent/agent-place";
import { endpointPlaceSettings, readClaudeResidentTurn } from "@agency/agent/claude-code";
import { codeRoot } from "@agency/support/roots";
import { scratchDir } from "@agency/test-support/scratch";

import type { HarnessRun } from "@agency/agent/harness";

// An agent's place, read over what a live Claude Code program really handed its status line and its
// hooks: one session that ran a four-tool turn with a word pushed after its last tool call, a long
// prompt and a turn cut short by Escape, then ended with /exit; and a second program on a fresh
// session. The transcript beside them is the first session's own, cut down to what a reader reads.

interface Recorded {
  status: StatusRecord[];
  hooks: HookRecord[];
  transcript: Record<string, unknown>[];
}

const recorded = JSON.parse(readFileSync(join(import.meta.dir, "fixtures", "resident-claude.json"), "utf8")) as Recorded;
const firstSession = "6096288f-1bcf-42d2-a1f5-4963bc592176";
const hooks = (kind: string) => recorded.hooks.filter((hook) => hook.kind === kind).map((hook) => hook.at);
const [promptA, pushX, promptLong, promptCut] = hooks("submit") as [number, number, number, number];
const [endA, endX] = hooks("stop") as [number, number];
const turnLine = (text: string) => recorded.transcript.findIndex((line) => JSON.stringify((line as { message?: unknown }).message ?? "").includes(text));

const root = scratchDir("bmo-agent-place-");

afterAll(() => rmSync(root, { recursive: true, force: true }));

const run: HarnessRun = {
  line: "chat",
  model: "claude-sonnet-5",
  effort: "low",
  systemPromptPath: null,
  toolsShown: null,
  toolsAllowed: null,
  toolsDenied: null,
  mcpConfigPath: null,
  prompt: "",
  promptDelivery: "channel",
  resumeSessionId: null,
  keepSession: true,
  hostCommands: true,
  thinking: true,
  answerShape: "unread",
  cwd: root,
  env: {},
};

describe("the money a turn cost", () => {
  test("is the running total's difference across the turn, the word pushed after its last tool call included", () => {
    const lines = recorded.transcript.slice(turnLine("RUN-A"), turnLine("WORD-X") + 4);
    const around = statusAround(recorded.status, promptA, endX);
    const reading = readClaudeResidentTurn({ lines, statusBefore: around.before, statusAfter: around.after }, run);

    expect(pushX).toBeGreaterThan(promptA);
    expect(reading.state).toBe("ended");
    expect(reading.reply.text).toEndWith("PINEAPPLE");
    expect(reading.money).toBeCloseTo(0.0372666, 10);
  });

  test("reads the turn as going until the pushed word's own turn has ended", () => {
    const lines = recorded.transcript.slice(turnLine("RUN-A"), turnLine("WORD-X"));
    const reading = readClaudeResidentTurn({ lines }, run);

    expect(reading.state).toBe("working");
  });
});

describe("a new session", () => {
  test("starts its total again from zero, and a turn read across the two has no money", () => {
    const second = recorded.status.filter((record) => (record.payload as { session_id?: string }).session_id !== firstSession);
    const firstLast = recorded.status.findLast((record) => (record.payload as { session_id?: string }).session_id === firstSession);

    expect((second[0]?.payload as { cost: { total_cost_usd: number } }).cost.total_cost_usd).toBe(0);

    const reading = readClaudeResidentTurn({ lines: [], statusBefore: firstLast?.payload, statusAfter: second[1]?.payload }, run);

    expect(reading.money).toBeNull();
  });
});

describe("the account windows", () => {
  test("are absent from the reading made at startup and arrive with the first request", () => {
    const [startup, firstRequest] = recorded.status as [StatusRecord, StatusRecord];

    expect(readClaudeResidentTurn({ lines: [], statusAfter: startup.payload }, run).reply.sessionUsage).toBeNull();

    const usage = readClaudeResidentTurn({ lines: [], statusAfter: firstRequest.payload }, run).reply.sessionUsage;

    expect(usage?.utilization).toBeGreaterThanOrEqual(0);
    expect(usage?.utilization).toBeLessThanOrEqual(1);
    expect(usage?.weeklyUtilization).not.toBeNull();
  });

  test("carry the context the status line counts", () => {
    const reading = readClaudeResidentTurn({ lines: [], statusAfter: recorded.status[1]?.payload }, run);

    expect(reading.reply.contextTokens).toBe(2 + 3869);
  });
});

describe("a turn cut short by Escape", () => {
  test("fires no Stop hook and reads as interrupted", () => {
    expect(hooks("stop").some((at) => at > promptCut)).toBe(false);

    const lines = recorded.transcript.slice(turnLine("Run one Bash command"), turnLine("interrupted by user") + 1);

    expect(readClaudeResidentTurn({ lines }, run).state).toBe("interrupted");
  });

  test("leaves the bot's run open until the bot closes it", () => {
    const place = join(root, "cut");

    beginPlaceRun(place, "bmo-chat-9", "room test", promptCut - 5);
    recordCall(place, "submit", {}, promptCut);
    expect(readRun(place)?.endedAt).toBeNull();
    expect(readRun(place)?.person).toBe(false);

    endPlaceRun(place, promptCut + 6000);
    expect(readRun(place)?.endedAt).toBe(promptCut + 6000);
  });
});

describe("a run's own variables", () => {
  test("are the fixed three and every one under the turn prefix, and a resident program stands without them", () => {
    const pane = { PATH: "/usr/bin", BMO_DATA_DIR: "/data", BMO_RUN_NAME: "chat:1", BMO_TURN_CHAT: "-1", BMO_TURN_VIA: "console", BMO_WORK_ORDER_STEP: "1" };
    const place = join(root, "variables");

    expect(runVariables(pane)).toEqual({ BMO_RUN_NAME: "chat:1", BMO_TURN_CHAT: "-1", BMO_TURN_VIA: "console", BMO_WORK_ORDER_STEP: "1" });
    expect(residentEnvironment(pane, place)).toEqual({ PATH: "/usr/bin", BMO_DATA_DIR: "/data", BMO_AGENT_PLACE: place });
  });

  test("are read from the run the place says is going, each round its own, and none for a person's turn", () => {
    const place = join(root, "rounds");
    // A process the program started: the program's environment, still carrying what the run that
    // started it had, which is not what the round going now is about.
    const started = { BMO_AGENT_PLACE: place, BMO_TURN_CHAT: "-1", PATH: "/usr/bin" };

    beginPlaceRun(place, "chat:-1:1:1", "a talk", 1_000, { BMO_RUN_NAME: "chat:-1:1:1", BMO_TURN_CHAT: "-1" });
    expect(turnEnvironment(started).BMO_TURN_CHAT).toBe("-1");
    endPlaceRun(place, 2_000);

    beginPlaceRun(place, "chat:42:3:2", "a talk", 3_000, { BMO_RUN_NAME: "chat:42:3:2", BMO_TURN_CHAT: "42", BMO_TURN_VIA: "console" });
    expect(turnEnvironment(started)).toMatchObject({ BMO_RUN_NAME: "chat:42:3:2", BMO_TURN_CHAT: "42", BMO_TURN_VIA: "console" });
    endPlaceRun(place, 4_000);

    // A touch names no room, and the room of the round before it is not left standing for it.
    beginPlaceRun(place, "chat:42:5:3", "a talk", 5_000, { BMO_RUN_NAME: "chat:42:5:3" });
    expect("BMO_TURN_CHAT" in turnEnvironment(started)).toBe(false);
    expect("BMO_TURN_VIA" in turnEnvironment(started)).toBe(false);
    endPlaceRun(place, 6_000);

    recordCall(place, "submit", {}, 7_000);
    expect(Object.keys(turnEnvironment(started)).filter((name) => name.startsWith("BMO_TURN_") || name === "BMO_RUN_NAME")).toEqual([]);
  });
});

describe("the place a program writes into", () => {
  test("replays the recorded calls into the run the status line names", () => {
    const place = join(root, "replay");

    beginPlaceRun(place, "bmo-chat-1", "room test", promptA - 10);

    for (const call of [...recorded.status.map((record) => ({ at: record.at, kind: "status" as const, payload: record.payload })), ...recorded.hooks]
      .filter((call) => call.at <= endX)
      .sort((a, b) => a.at - b.at)) {
      recordCall(place, call.kind, call.payload, call.at);
    }

    expect(readStatus(place).length).toBe(recorded.status.filter((record) => record.at <= endX).length);
    expect(readHooks(place).map((hook) => hook.kind)).toEqual(["start", "submit", "submit", "stop", "stop"]);
    expect(readRun(place)).toMatchObject({ name: "bmo-chat-1", person: false, endedAt: endA });
  });

  test("takes a prompt submitted while no run of the bot's is going as a person's turn", () => {
    const place = join(root, "person");

    recordCall(place, "submit", {}, promptLong);
    expect(readRun(place)).toMatchObject({ person: true, startedAt: promptLong, endedAt: null });

    recordCall(place, "stop", {}, promptLong + 900);
    expect(readRun(place)).toMatchObject({ person: true, endedAt: promptLong + 900 });
  });

  test("a status reading keeps the login of the program that made it, and the reading after a turn hands it over", () => {
    const place = join(root, "stamped");
    const login = { configDir: "/logins/second", token: null };

    recordCall(place, "status", { session_id: "s", cost: { total_cost_usd: 0.1 } }, 1000);
    recordCall(place, "status", { session_id: "s", cost: { total_cost_usd: 0.2 } }, 2100, { login });

    expect(readStatus(place).map((record) => record.login ?? null)).toEqual([null, login]);
    expect(statusAround(readStatus(place), 1500, 2000, "s").login).toEqual(login);
    // A line written before records carried a login names nobody.
    expect(statusAround(readStatus(place).slice(0, 1), 0, 500, "s").login).toBeNull();
  });

  test("named a session, a reading another session wrote into the same place is not the one after the turn", () => {
    const place = join(root, "sessions");
    const other = { configDir: "/logins/other", token: null };
    const own = { configDir: "/logins/own", token: null };

    recordCall(place, "status", { session_id: "mine", cost: { total_cost_usd: 0.1 } }, 1000, { login: own });
    recordCall(place, "status", { session_id: "theirs", cost: { total_cost_usd: 5 } }, 2050, { login: other });
    recordCall(place, "status", { session_id: "mine", cost: { total_cost_usd: 0.2 } }, 2100, { login: own });

    const around = statusAround(readStatus(place), 1500, 2000, "mine");

    expect(around.after).toEqual({ session_id: "mine", cost: { total_cost_usd: 0.2 } });
    expect(around.login).toEqual(own);
    // Asked about no session, the first reading after the end is whoever wrote it.
    expect(statusAround(readStatus(place), 1500, 2000).login).toEqual(other);
  });

  test("a reading stamped in the very millisecond the turn ended is not the one after it", () => {
    const place = join(root, "same-millisecond");
    const login = { configDir: "/logins/second", token: null };
    const limits = { five_hour: { used_percentage: 36, resets_at: 9000 } };

    // The program's call as it came up, which the launcher waited for before it put the prompt in,
    // and a turn that ended within the same millisecond.
    recordCall(place, "status", { session_id: "s", cost: { total_cost_usd: 0 } }, 2000, { login });
    recordCall(place, "status", { session_id: "s", cost: { total_cost_usd: 0.02 }, rate_limits: limits }, 2001, { login });

    expect(statusAround(readStatus(place), 2000, 2000, "s").after).toEqual({ session_id: "s", cost: { total_cost_usd: 0.02 }, rate_limits: limits });
    expect(statusAround(readStatus(place), 2000, 2000, "s").before).toEqual({ session_id: "s", cost: { total_cost_usd: 0 } });
  });

  test("waits for the reading that follows a turn's end until it comes or the program is gone", async () => {
    const place = join(root, "wait");

    recordCall(place, "status", { session_id: "s", cost: { total_cost_usd: 0.1 } }, 1000);
    const missing = await statusAfterTurn(place, 1500, 2000, { standing: () => false, pollMs: 10 });

    expect(missing.after).toBeNull();

    const waiting = statusAfterTurn(place, 1500, 2000, { standing: () => true, pollMs: 10 });

    await Bun.sleep(50);
    recordCall(place, "status", { session_id: "s", cost: { total_cost_usd: 0.2 } }, 2100);
    const found = await waiting;

    expect(found.before).toEqual({ session_id: "s", cost: { total_cost_usd: 0.1 } });
    expect(found.after).toEqual({ session_id: "s", cost: { total_cost_usd: 0.2 } });
  });
});

describe("the settings a resident program is started with", () => {
  // Claude Code shows a picker row's label at the top of its screen in place of the model it is sent,
  // so a registration's program reads as the registration's label, node and all.
  test("give a model the endpoint labels a picker row under that label", () => {
    const model = { name: "local-0-1@charlie", label: "Local 0.1", contextTokens: 1, answerTokens: 1, thinking: false, home: root };
    const labelled = JSON.parse(readFileSync(endpointPlaceSettings(run, join(root, "labelled"), model), "utf8"));
    const plain = JSON.parse(readFileSync(endpointPlaceSettings(run, join(root, "plain"), { ...model, label: undefined }), "utf8"));

    expect(labelled.modelPicker).toEqual({ options: [{ model: "local-0-1@charlie", label: "Local 0.1" }] });
    expect(plain.modelPicker).toBeUndefined();
  });

  test("name the status line and the three hooks, beside the hooks the line already had", () => {
    const place = join(root, "settings");
    const path = writePlaceSettings(place, { hooks: { PreToolUse: [{ matcher: "Agent", hooks: [] }], Stop: [{ hooks: [{ type: "command", command: "x" }] }] } }, codeRoot);
    const settings = JSON.parse(readFileSync(path, "utf8"));
    const binary = join(codeRoot, "bin", "bmo-agency");

    expect(settings.statusLine.command).toBe(`'${binary}' 'agent-status' 'status' '--place' '${place}'`);
    expect(settings.hooks.PreToolUse).toHaveLength(1);
    expect(settings.hooks.Stop).toHaveLength(2);
    expect(settings.hooks.UserPromptSubmit[0].hooks[0].command).toContain("'submit'");
    expect(settings.hooks.SessionStart[0].hooks[0].command).toContain("'start'");

    // A line that asks for them has its stop hook started with the holds and its submit hook with the
    // typed line's composition, given the time the composing takes; the hooks themselves are the binary's.
    const held = JSON.parse(readFileSync(writePlaceSettings(join(root, "held"), {}, codeRoot, { holdForTasks: true, holdForWording: true, composeTyped: true }), "utf8"));

    expect(held.hooks.Stop[0].hooks[0].command).toEndWith(`'${holdFlag}' '${wordingHoldFlag}'`);
    expect(held.hooks.UserPromptSubmit[0].hooks[0].command).toEndWith(`'${typedFlag}'`);
    expect(held.hooks.UserPromptSubmit[0].hooks[0].timeout).toBe(typedHookSeconds);
    expect(settings.hooks.Stop[1].hooks[0].command).not.toContain(holdFlag);
  });
});
