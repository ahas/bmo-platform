import "@agency/test-support/setup";

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { Archive } from "@agency/archive/archive";
import { dataRoot } from "@agency/support/roots";
import { emptyDatabase } from "@agency/test-support/store";
import { frontAlone, frontBinary } from "./control-contract-base";

import type { ControlBase } from "./control-contract-base";

// A supervisor's reset by range (docs/supervisors.md, 「Starting it afresh」; docs/api.md, 「Agents」), through
// agency's own server on a store of the test's. Each test stands a supervisor of its own: a room it
// answers alone and a room it shares, its room sessions with their kept turns, its run log lines, and
// the records Claude Code keeps of the sessions run in its home and in its brain folder: under the registered
// road's home, and under the home of a login the store holds, as a turn gone out on that login writes them. What
// it remembers is the brain's, the supervisor's own brain found by its key, so the brain is a stand-in that writes
// down what it was asked.

const token = "secret";
// The binary is read as the file loads, so no hook spends its own time on it.
const binary = frontBinary();
const brainRoot = join(dataRoot(), "brain");
// The configuration home of the login the store holds, where a turn gone out on it keeps its records.
const loginHome = join(dataRoot(), "env", "tester-login");
const brainAsked: { method: string; path: string; query: string }[] = [];
const counts = { brains: 1 };
const BRAIN_ID = 7;
const brain = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  fetch(request) {
    const url = new URL(request.url);

    brainAsked.push({ method: request.method, path: url.pathname, query: url.search });

    const member = { id: BRAIN_ID, accountKey: "1", agentKey: url.searchParams.get("agentKey") ?? "", name: "tester", createdAt: 1 };

    return request.method === "GET" ? Response.json({ items: [member], more: false }) : Response.json(member);
  },
});

let front: ControlBase | null = null;
let archive: Archive | null = null;

interface Stood {
  agent: string;
  alone: number;
  shared: number;
  // Claude Code's records of the sessions run in its home, and of those run in its brain folder.
  home: string[];
  brainFolder: string[];
}

interface ResetAnswer {
  agent: string;
  label: string;
  ranges: string[];
  alone: { chatId: number; title: string | null }[];
  shared: { chatId: number; title: string | null }[];
  rows: { table: string; range: string; rows: number }[];
  files: string[];
  brain: { took: unknown; couldNot: string[] } | null;
  failures?: string[];
}

function ask(query: Record<string, string>, authorization = `Bearer ${token}`): Promise<Response> {
  const { agent, ...rest } = query;

  return fetch(`http://127.0.0.1:${front!.port}/supervisors/${agent ?? "-"}/resets`, {
    method: "POST",
    headers: { authorization, "content-type": "application/json" },
    body: JSON.stringify({ ...rest, dryRun: true }),
  });
}

function reset(body: Record<string, unknown>): Promise<Response> {
  const { agent, ...rest } = body;

  return fetch(`http://127.0.0.1:${front!.port}/supervisors/${agent ?? "-"}/resets`, {
    method: "POST",
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify({ by: "tester", ...rest }),
  });
}

async function answered(response: Promise<Response>): Promise<ResetAnswer> {
  const done = await response;
  const body = (await done.json()) as ResetAnswer & { error?: string };

  expect([done.status, body.error]).toEqual([200, undefined]);

  return body;
}

async function refusal(response: Promise<Response>): Promise<[number, string]> {
  const done = await response;

  return [done.status, ((await done.json()) as { error: string }).error];
}

const count = async (sql: string, ...args: (string | number)[]): Promise<number> => Number(((await archive!.sql.get(`SELECT count(*) AS n FROM ${sql}`, ...args)) as { n: number }).n);

const rowsOf = (answer: ResetAnswer): Record<string, number> => Object.fromEntries(answer.rows.filter((one) => one.rows > 0).map((one) => [one.table, one.rows]));

// The records Claude Code keeps of one session run in `cwd` under a configuration home: its transcript and its
// folder.
function recordsUnder(config: string, cwd: string, session: string): string[] {
  const project = join(config, "projects", cwd.replace(/[^A-Za-z0-9]/g, "-"));
  const records = [join(project, `${session}.jsonl`), join(project, session)];

  mkdirSync(records[1]!, { recursive: true });
  writeFileSync(records[0]!, "{}\n");

  return records;
}

// The records of a session run in `cwd` on the registered road, and of another run there on the login.
function harnessRecords(cwd: string, session: string): string[] {
  return [...recordsUnder(join(dataRoot(), "claude-roads", "registered", "m"), cwd, session), ...recordsUnder(loginHome, cwd, `${session}-login`)];
}

async function stand(agent: string, rooms: number): Promise<Stood> {
  const db = archive!;
  const alone = -rooms;
  const shared = -rooms - 1;
  const run = (sql: string, ...args: (string | number)[]) => db.sql.run(sql, ...args);

  // A supervisor as one added on the console stands: the room scope, the pieces shape, its brain folder its own.
  await run("INSERT INTO supervisors (name, label, session_scope, turn_shape, brain_folder, created_at, updated_at) VALUES (?, 'Tester', 'room', 'pieces', ?, 1, 1)", agent, `agents/${agent}`);

  for (const chatId of [alone, shared]) {
    await run("INSERT INTO room_sessions (agent, chat_id, generation, opened_at, harness_session, harness_road) VALUES (?, ?, 1, 1, ?, 'registered::m')", agent, chatId, `s-${agent}-${chatId}`);
    await run("INSERT INTO room_session_turns (agent, chat_id, generation, position, role, text, at) VALUES (?, ?, 1, 0, 'user', 'hi', 1), (?, ?, 1, 1, 'assistant', 'hello', 2)", agent, chatId, agent, chatId);
  }

  // Its own line in the lone room, another agent's line there, whose room's whole log goes with it, and
  // another agent's line in the shared room, which stays. Each is written now: the server's log pass takes
  // every agent's lines past the log's age but its newest, so a line dated long ago could be gone before the
  // reset reads it.
  const now = Date.now();

  for (const [who, chatId, runName] of [[agent, alone, `chat:${alone}:1`], ["other", alone, `chat:${alone}:2`], ["other", shared, `chat:${shared}:1`]] as const) {
    await run("INSERT INTO agent_log (agent, run, chat_id, at, kind, text) VALUES (?, ?, ?, ?, 'prompt', 'x')", who, runName, chatId, now);
  }

  // Runs of the ledger: the supervisor's in either room and another agent's in the lone room go with it, and
  // another agent's in the shared room stays.
  for (const [who, chatId] of [[agent, alone], [agent, shared], ["other", alone], ["other", shared]] as const) {
    await run("INSERT INTO runs (agent, chat_id, unit, depth, started_at) VALUES (?, ?, ?, 0, 1)", who, chatId, `unit-${who}-${chatId}`);
  }

  return {
    agent,
    alone,
    shared,
    home: harnessRecords(join(dataRoot(), "supervisor-homes", agent), `s-${agent}`),
    brainFolder: harnessRecords(join(brainRoot, "agents", agent), `r-${agent}`),
  };
}

// Made at the file's top, as a scratch store must be.
const store = await emptyDatabase();

beforeAll(async () => {
  // The server migrates the store as it stands on it; the test writes its rows once it has.
  front = await frontAlone(binary, token, { BMO_ARCHIVE_URL: store, AGENCY_BRAIN_URL: `http://127.0.0.1:${brain.port}/` });
  archive = new Archive(store);
  await archive.sql.run("INSERT INTO claude_accounts (id, name, place, enabled, credential_kind, credential_path, added_at) VALUES ('tester-login', 'tester-login', 9, 0, 'directory', ?, 1)", loginHome);
});

afterAll(async () => {
  await front?.close();
  await archive?.close();
  await brain.stop(true);
});

describe("POST /supervisors/{supervisorName}/resets", () => {
  test("shows ahead of time every row and record a range would take and the brain's preview, and changes nothing", async () => {
    const stood = await stand("tester-show", 710);

    brainAsked.length = 0;

    const both = await answered(ask({ agent: stood.agent, conversation: "true", person: "true", alone: String(stood.alone), shared: String(stood.shared) }));

    expect([both.agent, both.label, both.ranges]).toEqual([stood.agent, "Tester", ["conversation", "person"]]);
    expect([both.alone, both.shared]).toEqual([[{ chatId: stood.alone, title: null }], [{ chatId: stood.shared, title: null }]]);
    expect(rowsOf(both)).toEqual({ agent_log: 2, runs: 3, room_session_turns: 4, room_sessions: 2 });
    expect(both.files.toSorted()).toEqual([...stood.home, ...stood.brainFolder].toSorted());
    expect(both.brain).toEqual({ took: counts, couldNot: [] });
    expect(brainAsked).toEqual([{ method: "GET", path: "/brains", query: `?kind=agent&agentKey=${stood.agent}&limit=1` }]);

    const conversation = await answered(ask({ agent: stood.agent, conversation: "1", alone: String(stood.alone) }));

    expect(conversation.files.toSorted()).toEqual(stood.home.toSorted());
    expect(conversation.brain).toBeNull();
    expect([await count("room_sessions WHERE agent = ?", stood.agent), await count("agent_log WHERE chat_id = ?", stood.alone)]).toEqual([2, 2]);
    expect([...stood.home, ...stood.brainFolder].every((path) => existsSync(path))).toBe(true);
  });

  test("of the conversation takes its rows and the lone room's whole log, ends its sessions on record, and keeps what it remembers", async () => {
    const stood = await stand("tester-talk", 720);

    await archive!.sql.run("INSERT INTO session_resets (agent, chat_id, asked_by, asked_at, reason) VALUES (?, ?, 'tester', 1, 'generation settings saved')", stood.agent, stood.alone);
    brainAsked.length = 0;

    const done = await answered(reset({ agent: stood.agent, conversation: true, alone: [stood.alone], shared: [stood.shared] }));

    expect(rowsOf(done)).toEqual({ agent_log: 2, runs: 3, room_session_turns: 4, room_sessions: 2 });
    expect(done.failures).toEqual([]);
    expect([await count("room_sessions WHERE agent = ?", stood.agent), await count("room_session_turns WHERE agent = ?", stood.agent)]).toEqual([0, 0]);
    expect([await count("agent_log WHERE chat_id = ?", stood.alone), await count("agent_log WHERE chat_id = ?", stood.shared)]).toEqual([0, 1]);
    expect(await archive!.sql.all("SELECT agent, chat_id AS room FROM runs WHERE chat_id IN (?, ?)", stood.alone, stood.shared)).toEqual([{ agent: "other", room: stood.shared }]);
    expect([stood.home.some((path) => existsSync(path)), stood.brainFolder.every((path) => existsSync(path))]).toEqual([false, true]);
    expect([done.brain, brainAsked]).toEqual([null, []]);
    expect(await archive!.sql.all("SELECT chat_id AS room, asked_by AS by, reason, CASE WHEN done_at IS NULL THEN 'waiting' ELSE 'done' END AS state FROM session_resets WHERE agent = ? ORDER BY id", stood.agent)).toEqual([
      { room: stood.alone, by: "tester", reason: "generation settings saved", state: "done" },
      { room: stood.shared, by: "tester", reason: "supervisor reset", state: "done" },
      { room: stood.alone, by: "tester", reason: "supervisor reset", state: "done" },
    ]);
  });

  test("of what it remembers has the brain let its brain go, takes the records kept in its brain folder, and keeps the conversation", async () => {
    const stood = await stand("tester-mind", 730);

    brainAsked.length = 0;

    const done = await answered(reset({ agent: stood.agent, person: true, alone: String(stood.alone), shared: String(stood.shared) }));

    expect([done.rows, done.files.toSorted()]).toEqual([[], stood.brainFolder.toSorted()]);
    expect(done.brain).toEqual({ took: counts, couldNot: [] });
    expect(done.failures).toEqual([]);
    expect(brainAsked).toEqual([
      { method: "GET", path: "/brains", query: `?kind=agent&agentKey=${stood.agent}&limit=1` },
      { method: "DELETE", path: `/brains/${BRAIN_ID}`, query: "" },
    ]);
    expect([stood.home.every((path) => existsSync(path)), stood.brainFolder.some((path) => existsSync(path))]).toEqual([true, false]);
    expect([await count("room_sessions WHERE agent = ?", stood.agent), await count("session_resets WHERE agent = ?", stood.agent), await count("runs WHERE agent = ?", stood.agent)]).toEqual([2, 0, 2]);
  });

  test("refuses a supervisor whose memory folder is the brain root, a name no supervisor holds, an ask naming no range, and a caller without the token", async () => {
    const stood = await stand("tester-none", 740);

    await archive!.sql.run("INSERT INTO supervisors (name, session_scope, turn_shape, brain_folder, created_at, updated_at) VALUES ('root-held', 'agent', 'window', '', 1, 1)");

    expect(await refusal(reset({ agent: "root-held", conversation: true }))).toEqual([400, "root-held's memory folder is the brain root, so it is not reset."]);
    expect(await refusal(reset({ agent: "nobody", conversation: true }))).toEqual([404, "There is no supervisor nobody."]);
    expect((await refusal(reset({ agent: stood.agent, conversation: false, person: false })))[1]).toStartWith("Choose at least one range to take");
    expect((await ask({ agent: stood.agent, conversation: "true" }, "Bearer wrong")).status).toBe(401);
    expect(await count("room_sessions WHERE agent = ?", stood.agent)).toBe(2);
  });
});
