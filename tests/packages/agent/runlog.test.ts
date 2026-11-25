import "@agency/test-support/setup";

import { describe, expect, setSystemTime, test } from "bun:test";
import { writeFileSync } from "node:fs";
import { join } from "node:path";

import { scratchDir } from "@agency/test-support/scratch";
import { Archive } from "@agency/archive/archive";
import { scratchArchive } from "@agency/test-support/store";
import { OWNER_ID, ROOM_ID, UNIT } from "@agency/test-support/ids";
import { maxAgeSeconds, maxPromptChars, maxToolChars, mcpCallOf, pruneRunLog, RunLog, runLogLines, systemPromptLine } from "@agency/agent/runlog";

interface LogRow {
  id: number;
  agent: string;
  run: string;
  chat_id: number | null;
  at: number;
  kind: string;
  name: string | null;
  parent: string | null;
  ref: string | null;
  truncated: number;
  text: string;
  job_id: number | null;
  sequence: number | null;
  source: string | null;
  source_event: number | null;
  source_block: number | null;
  source_id: string | null;
  state: string | null;
  upstream_truncated: number;
  detail_complete: number;
  detail_length: number;
  metadata_length: number;
  detail: string | null;
  metadata: string | null;
}

// The bot never reads the log back, so what was written is read here the way the web reads it,
// straight off the table. In memory unless the test is about the file itself.
async function logArchive(): Promise<{ path: string; archive: Archive; rows: () => Promise<LogRow[]> }> {
  const archive = await scratchArchive();
  const path = archive.path;

  return { path, archive, rows: async () => await archive.sql.all("SELECT * FROM agent_log ORDER BY id") as LogRow[] };
}

function stream(events: unknown[]): string {
  return `${events.map((event) => JSON.stringify(event)).join("\n")}\n`;
}

const encoder = new TextEncoder();

const turn: unknown[] = [
  { type: "system", subtype: "init", model: "claude-opus-5", tools: ["Bash", "Read"] },
  { type: "assistant", message: { content: [{ type: "thinking", thinking: "where to start" }, { type: "text", text: "let me look" }] } },
  { type: "assistant", message: { content: [{ type: "tool_use", id: "toolu_1", name: "Bash", input: { command: "ls" } }] } },
  { type: "user", message: { content: [{ type: "tool_result", tool_use_id: "toolu_1", content: "src\nweb" }] } },
  { type: "result", is_error: false, result: "seen it all" },
];

describe("what one event of a stream leaves in the log", () => {
  test("a text, a thinking and a tool call are three lines of their own", () => {
    expect(runLogLines(turn[1])).toEqual([
      { kind: "thinking", text: "where to start", parent: null, sourceBlock: 0, sourceId: null },
      { kind: "text", text: "let me look", parent: null, sourceBlock: 1, sourceId: null },
    ]);
    expect(runLogLines(turn[2])).toEqual([
      {
        kind: "tool",
        name: "Bash",
        ref: "toolu_1",
        text: '{"command":"ls"}',
        parent: null,
        sourceBlock: 0,
        sourceId: null,
        state: "requested",
        upstreamTruncated: false,
      },
    ]);
    expect(runLogLines(turn[0])[0]).toEqual({ kind: "start", text: "claude-opus-5 · tools: Bash, Read", parent: null });
    expect(runLogLines(turn[4])[0]?.kind).toBe("end");
  });

  test("a thinking block with nothing in it is no line at all", () => {
    // What the CLI writes for every run whose thinking is not surfaced: the block and its
    // signature, and no thinking. An empty line would say nothing anyone could read.
    const event = {
      type: "assistant",
      message: { content: [{ type: "thinking", thinking: "", signature: "abc" }, { type: "text", text: "an answer" }] },
    };

    expect(runLogLines(event)).toEqual([{ kind: "text", text: "an answer", parent: null, sourceBlock: 1, sourceId: null }]);
  });

  test("a tool result finds the call it answers through its ref", () => {
    const call = runLogLines(turn[2])[0];
    const result = runLogLines(turn[3])[0];

    expect(result?.kind).toBe("tool-result");
    expect(result?.ref).toBe(call?.ref ?? "");
    expect(result?.text).toBe("src\nweb");
  });

  test("everything an event made inside a Task branch carries that branch", () => {
    const lines = runLogLines({
      type: "assistant",
      parent_tool_use_id: "toolu_task",
      message: { content: [{ type: "text", text: "read this far" }, { type: "tool_use", id: "toolu_2", name: "Read", input: {} }] },
    });

    expect(lines.map((line) => line.parent)).toEqual(["toolu_task", "toolu_task"]);
  });

  test("an MCP tool keeps its own name and a Task call records the co-worker it opened", () => {
    const mcp = runLogLines({
      type: "assistant",
      message: { content: [{ type: "tool_use", id: "toolu_3", name: "mcp__web__web_search", input: { query: "the weather" } }] },
    });
    const task = runLogLines({
      type: "assistant",
      message: {
        content: [
          { type: "tool_use", id: "toolu_4", name: "Task", input: { subagent_type: "scout", description: "find", prompt: "where is it" } },
        ],
      },
    });

    // The CLI answers to more than one name for the call that opens a branch, and both carry
    // the kind of co-worker the same way.
    const agent = runLogLines({
      type: "assistant",
      message: {
        content: [{ type: "tool_use", id: "toolu_5", name: "Agent", input: { subagent_type: "verifier" } }],
      },
    });

    expect(mcp[0]?.name).toBe("mcp__web__web_search");
    expect(task[0]?.name).toBe("Task(scout)");
    expect(task[0]?.text).toContain("where is it");
    expect(agent[0]?.name).toBe("Agent(verifier)");
  });

  test("an event that says nothing about what the run did leaves no line", () => {
    expect(runLogLines({ type: "system", subtype: "api_retry", attempt: 1 })).toEqual([]);
    expect(runLogLines("not an event")).toEqual([]);
  });
});

describe("the writer", () => {
  test("a job preview never splits a Unicode code point and keeps its full canonical detail", async () => {
    const { archive, rows } = await logArchive();
    const jobId = 41;
    const output = `${"x".repeat(maxToolChars - 1)}🙂tail`;
    const log = new RunLog({ archive, agent: "worker", run: "unicode-preview", chatId: OWNER_ID, jobId });

    await log.write({ kind: "tool-result", name: "Bash", text: output });

    const stored = (await rows())[0]!;
    expect(stored).toMatchObject({ truncated: 1, detail: output, detail_complete: 1 });
    expect([...stored.text]).toHaveLength(maxToolChars);
    expect(stored.text.endsWith("🙂")).toBe(true);
  });

  test("the lines are in the table as they arrive, with nothing to flush", async () => {
    const { archive, rows } = await logArchive();
    const log = new RunLog({ archive, agent: "worker", run: UNIT, chatId: ROOM_ID });

    // The run is still going: no end frame has been read and nothing has been closed.
    await log.feed(encoder.encode(stream(turn.slice(0, 3))));

    expect((await rows()).map((row) => row.kind)).toEqual(["start", "thinking", "text", "tool"]);
    expect((await rows())[0]).toMatchObject({ agent: "worker", run: UNIT, chat_id: ROOM_ID, truncated: 0 });
    expect((await rows())[3]).toMatchObject({ kind: "tool", name: "Bash", ref: "toolu_1" });
  });

  test("a watcher reads each line of the stream as it is written, until it lets go", async () => {
    const { archive } = await logArchive();
    const log = new RunLog({ archive, agent: "worker", run: UNIT, chatId: ROOM_ID });
    const seen: string[] = [];
    const letGo = log.watch((line) => void seen.push(line.kind));

    await log.feed(encoder.encode(stream(turn.slice(0, 3))));
    letGo();
    await log.write({ kind: "note", text: "after" });

    expect(seen).toEqual(["start", "thinking", "text", "tool"]);
  });

  test("a stream cut in the middle of a line loses nothing", async () => {
    const whole = await logArchive();
    const text = stream(turn);
    const at = Math.floor(text.length / 2);

    await new RunLog({ archive: whole.archive, agent: "search", run: "bmo-search-1", chatId: ROOM_ID }).feed(encoder.encode(text));

    // Read before the second archive opens, which a run on PostgreSQL stands on the same clone.
    const wholeRows = await whole.rows();
    const split = await logArchive();
    const log = new RunLog({ archive: split.archive, agent: "search", run: "bmo-search-1", chatId: ROOM_ID });

    await log.feed(encoder.encode(text.slice(0, at)));
    await log.feed(encoder.encode(text.slice(at)));

    const same = (rows: LogRow[]): unknown[] => rows.map((row) => [row.kind, row.name, row.ref, row.text]);

    expect(same(await split.rows())).toEqual(same(wholeRows));
    expect((await split.rows()).length).toBe(6);
  });

  test("what a run wrote in its own words is stored whole, and only bulk is cut and marked", async () => {
    const { archive, rows } = await logArchive();
    const log = new RunLog({ archive, agent: "search", run: "search:1", chatId: ROOM_ID });
    const report = "€".repeat(10_000);

    await log.feed(
      encoder.encode(
        stream([
          { type: "assistant", message: { content: [{ type: "text", text: report }] } },
          { type: "user", message: { content: [{ type: "tool_result", tool_use_id: "toolu_1", content: "€".repeat(maxToolChars + 10) }] } },
        ]),
      ),
    );

    expect((await rows())[0]).toMatchObject({ kind: "text", truncated: 0, text: report });
    expect((await rows())[1]).toMatchObject({ kind: "tool-result", truncated: 1 });
    expect((await rows())[1]?.text.length).toBe(maxToolChars);

    await log.write({ kind: "note", text: `the line gave no answer — ${"€".repeat(10_000)}` });
    await log.write({ kind: "end", text: report });
    await log.write({ kind: "prompt", text: "€".repeat(maxPromptChars + 10) });

    expect((await rows())[2]).toMatchObject({ kind: "note", truncated: 0 });
    expect((await rows())[2]?.text.length).toBe(10_000 + "the line gave no answer — ".length);
    expect((await rows())[3]).toMatchObject({ kind: "end", truncated: 0, text: report });
    expect((await rows())[4]).toMatchObject({ kind: "prompt", truncated: 1 });
    expect((await rows())[4]?.text.length).toBe(maxPromptChars);
  });

  test("a log that keeps its launch stores every prompt whole, while its tools and a gate's prompt stay cut", async () => {
    const { archive, rows } = await logArchive();
    const chat = new RunLog({ archive, agent: "bmo", run: "chat:7:1", chatId: 7, keepsLaunch: true });
    const gate = new RunLog({ archive, agent: "classifier", run: "gate:1", chatId: 7 });
    const prompt = "€".repeat(maxPromptChars * 5);

    await chat.write({ kind: "prompt", text: prompt });
    await chat.write({ kind: "tool-result", text: "€".repeat(maxToolChars + 10) });
    await gate.write({ kind: "prompt", text: prompt });

    expect((await rows())[0]).toMatchObject({ kind: "prompt", truncated: 0, text: prompt });
    expect((await rows())[1]).toMatchObject({ kind: "tool-result", truncated: 1 });
    expect((await rows())[2]).toMatchObject({ kind: "prompt", truncated: 1 });
    expect((await rows())[2]?.text.length).toBe(maxPromptChars);
  });

  test("a launch writes its system prompt once per version and its prompt every time", async () => {
    const { archive, rows } = await logArchive();
    const systemPromptPath = join(scratchDir("bmo-runlog-system-"), "bmo.md");
    const launch = async (run: string, prompt: string): Promise<void> =>
      new RunLog({ archive, agent: "bmo", run, chatId: 7, keepsLaunch: true }).launched({ systemPromptPath, prompt });

    writeFileSync(systemPromptPath, "version one\n");
    await launch("chat:7:1", "first turn");
    await launch("chat:7:2", "second turn");
    writeFileSync(systemPromptPath, "version two\n");
    await launch("chat:8:3", "third turn");

    expect((await rows()).map((row) => [row.run, row.name, row.text])).toEqual([
      ["chat:7:1", systemPromptLine, "version one\n"],
      ["chat:7:1", null, "first turn"],
      ["chat:7:2", null, "second turn"],
      ["chat:8:3", systemPromptLine, "version two\n"],
      ["chat:8:3", null, "third turn"],
    ]);

    // A log that does not keep its launch is handed the same run and writes nothing of it.
    await new RunLog({ archive, agent: "classifier", run: "gate:1", chatId: 7 }).launched({ systemPromptPath, prompt: "a gate" });
    await new RunLog({ archive, agent: "bmo", run: "chat:7:4", chatId: 7, keepsLaunch: true }).launched({ systemPromptPath: null, prompt: "fourth turn" });

    expect((await rows()).slice(5).map((row) => [row.agent, row.name, row.text])).toEqual([["bmo", null, "fourth turn"]]);
  });

  test("a worker job keeps complete lazy detail and ordered provider evidence beside its preview", async () => {
    const { archive, rows } = await logArchive();
    const jobId = 42;
    const log = new RunLog({ archive, agent: "worker", run: "bmo-task-detail", chatId: OWNER_ID, jobId });
    const command = `printf %s ${"x".repeat(maxToolChars + 100)}`;
    const result = "€€".repeat(maxToolChars);

    await log.feed(encoder.encode(stream([
      {
        type: "assistant",
        message: { id: "msg-call", content: [{ type: "tool_use", id: "tool-long", name: "Bash", input: { command } }] },
      },
      {
        type: "user",
        message: {
          id: "msg-result",
          content: [{ type: "tool_result", tool_use_id: "tool-long", content: result, is_error: false }],
        },
        toolUseResult: { status: "completed", structuredPatch: [{ oldStart: 1, newStart: 1 }] },
      },
    ])));
    await new RunLog({ archive, agent: "worker", run: "bmo-task-detail", chatId: OWNER_ID, jobId })
      .write({ kind: "note", text: "continued" });

    expect((await rows()).map((row) => row.sequence)).toEqual([1, 2, 3]);
    expect((await rows())[0]).toMatchObject({
      job_id: jobId,
      source: "claude-code",
      source_event: 1,
      source_block: 0,
      source_id: "msg-call",
      state: "requested",
      truncated: 1,
      detail_complete: 1,
    });
    expect((await rows())[0]?.detail).toBe(JSON.stringify({ command }));
    expect((await rows())[1]).toMatchObject({
      source_event: 2,
      source_id: "msg-result",
      state: "succeeded",
      truncated: 1,
      detail_complete: 1,
    });
    expect((await rows())[1]?.detail).toBe(result);
    expect(JSON.parse((await rows())[1]?.metadata ?? "{}")).toEqual({
      toolUseResult: { status: "completed", structuredPatch: [{ oldStart: 1, newStart: 1 }] },
      isError: false,
    });
  });

  test("tool answers keep explicit failure and upstream-loss evidence without inventing success", () => {
    const failed = runLogLines({
      type: "user",
      truncated: true,
      message: { id: "failed", content: [{ type: "tool_result", tool_use_id: "edit", content: "no", is_error: true }] },
    })[0];
    const unknown = runLogLines({
      type: "user",
      message: { content: [{ type: "tool_result", tool_use_id: "write", content: "done" }] },
    })[0];
    const applied = runLogLines({
      type: "user",
      toolUseResult: { filePath: "/repo/a.ts", structuredPatch: [] },
      message: { content: [{ type: "tool_result", tool_use_id: "edit", content: "done" }] },
    })[0];

    expect(failed).toMatchObject({ state: "failed", upstreamTruncated: true, sourceId: "failed" });
    expect(unknown).toMatchObject({ state: "unverified" });
    expect(applied).toMatchObject({ state: "succeeded" });
    expect(runLogLines({ type: "assistant", isApiErrorMessage: true, message: { content: [{ type: "text", text: "limit" }] } })[0])
      .toMatchObject({ kind: "text", state: "failed" });
    expect(runLogLines({ type: "result", is_error: true, result: "limit" })[0])
      .toMatchObject({ kind: "end", state: "failed" });
  });
});

// The head pane of an agent's herdr workspace reads this back to draw the agent's session, which is
// the one thing standing there between runs.
describe("the newest lines read back", () => {
  test("one agent's own lines, oldest first, bounded by what was asked for", async () => {
    const { archive } = await logArchive();
    const log = new RunLog({ archive, agent: "worker", run: UNIT, chatId: null });
    const other = new RunLog({ archive, agent: "search", run: "bmo-search-1", chatId: null });

    await other.write({ kind: "text", text: "another agent" });

    for (const number of [1, 2, 3]) {
      await log.write({ kind: "text", text: `line ${number}` });
    }

    expect((await archive.runs().recentAgentLog("worker", 2)).map((line) => line.text)).toEqual(["line 2", "line 3"]);
    expect((await archive.runs().recentAgentLog("worker", 10)).map((line) => line.run)).toEqual([UNIT, UNIT, UNIT]);
    expect((await archive.runs().recentAgentLog("search", 10)).map((line) => line.text)).toEqual(["another agent"]);
  });

  test("an agent that has written nothing reads back as nothing, never as a failure", async () => {
    const { archive } = await logArchive();

    expect(await archive.runs().recentAgentLog("worker", 10)).toEqual([]);
    expect(await archive.runs().recentAgentLog("worker", 0)).toEqual([]);
  });
});

describe("what the log keeps", () => {
  test("an agent keeps its newest lines and loses the rest", async () => {
    const { archive, rows } = await logArchive();
    const log = new RunLog({ archive, agent: "worker", run: UNIT, chatId: null });
    const other = new RunLog({ archive, agent: "search", run: "bmo-search-1", chatId: null });

    for (const number of [1, 2, 3, 4]) {
      await log.write({ kind: "text", text: `line ${number}` });
    }

    // The count reaches a run that has ended, and the end is what says it may.
    await log.write({ kind: "end", text: "the end" });
    await other.write({ kind: "text", text: "another agent" });
    await other.write({ kind: "end", text: "the other end" });
    await archive.runs().pruneAgentLog({ agent: "worker", keep: 2, maxBytes: Infinity, before: 0 });

    expect((await rows()).map((row) => row.text)).toEqual(["line 4", "the end", "another agent", "the other end"]);

    // Every agent at once, the sweep of the whole table.
    await archive.runs().pruneAgentLog({ agent: null, keep: 1, maxBytes: Infinity, before: 0 });

    expect((await rows()).map((row) => row.text)).toEqual(["the end", "the other end"]);
  });

  test("a line past the age goes whatever the count", async () => {
    const { archive, rows } = await logArchive();
    const log = new RunLog({ archive, agent: "worker", run: UNIT, chatId: null });

    setSystemTime(new Date(Date.now() - (maxAgeSeconds + 60) * 1000));

    try {
      await log.write({ kind: "text", text: "last week" });
    } finally {
      setSystemTime();
    }

    await log.write({ kind: "text", text: "today" });
    await pruneRunLog(archive, "worker");

    expect((await rows()).map((row) => row.text)).toEqual(["today"]);
  });

  test("an agent past its share of text loses its oldest lines but never its newest system prompt", async () => {
    const { archive, rows } = await logArchive();
    const jobId = 43;
    const chat = new RunLog({ archive, agent: "bmo", run: "chat:7:1", chatId: 7, keepsLaunch: true });
    const job = new RunLog({ archive, agent: "bmo", run: "bmo-task-bytes", chatId: 7, jobId });

    await chat.write({ kind: "prompt", name: systemPromptLine, ref: "v1", text: "v1".repeat(5) });
    await chat.write({ kind: "prompt", name: systemPromptLine, ref: "v2", text: "v2".repeat(5) });
    await job.write({ kind: "text", text: "Job".repeat(100) });

    for (const number of [1, 2, 3]) {
      await chat.write({ kind: "prompt", text: `${number}`.repeat(10) });
    }

    await chat.write({ kind: "end", text: "the end" });

    // Twenty-seven characters hold the run's end and its two newest turns; the newest system line
    // stays past the bound, and a job's line is not counted or taken.
    await archive.runs().pruneAgentLog({ agent: "bmo", keep: 100, maxBytes: 27, before: 0 });

    expect((await rows()).map((row) => row.text)).toEqual(["v2".repeat(5), "Job".repeat(100), "2".repeat(10), "3".repeat(10), "the end"]);

    // The count and the age leave it too, on the sweep over every agent.
    await archive.runs().pruneAgentLog({ agent: null, keep: 0, maxBytes: Infinity, before: Math.floor(Date.now() / 1000) + 60 });

    expect((await rows()).map((row) => row.text)).toEqual(["v2".repeat(5), "Job".repeat(100)]);
  });

  test("a job's lines survive both the count and age prunes", async () => {
    const { archive, rows } = await logArchive();
    const jobId = 44;
    const job = new RunLog({ archive, agent: "worker", run: "bmo-task-kept", chatId: OWNER_ID, jobId });
    const rolling = new RunLog({ archive, agent: "worker", run: "other", chatId: null });

    setSystemTime(new Date(Date.now() - (maxAgeSeconds + 60) * 1000));
    try {
      await job.write({ kind: "tool-result", text: "job detail" });
      await rolling.write({ kind: "text", text: "old rolling line" });
    } finally {
      setSystemTime();
    }

    for (const number of [1, 2, 3]) await rolling.write({ kind: "text", text: `rolling ${number}` });
    await rolling.write({ kind: "end", text: "rolling end" });
    await archive.runs().pruneAgentLog({ agent: "worker", keep: 1, maxBytes: Infinity, before: Date.now() - maxAgeSeconds * 1000 });

    expect((await rows()).map((row) => row.text)).toEqual(["job detail", "rolling end"]);
  });

  test("a run that has not ended keeps its own lines, whatever the rooms beside it write", async () => {
    const { archive, rows } = await logArchive();
    const going = new RunLog({ archive, agent: "bmo", run: "chat:7:1000:1", chatId: 7, keepsLaunch: true });
    const busy = new RunLog({ archive, agent: "bmo", run: "chat:8:1001:2", chatId: 8, keepsLaunch: true });

    await going.write({ kind: "start", text: "the opening line" });
    await going.write({ kind: "prompt", text: "what this turn was given" });

    // The other room answers four times over while the first turn waits on a tool of its own.
    for (const number of [1, 2, 3, 4]) {
      await busy.write({ kind: "text", text: `next door ${number}` });
      await busy.write({ kind: "end", text: `next door end ${number}` });
    }

    // Twenty characters hold the other room's newest end and nothing before it.
    await archive.runs().pruneAgentLog({ agent: "bmo", keep: 2, maxBytes: 20, before: 0 });

    // The waiting turn is read from its start, and the rooms that ended lost all but their newest.
    expect((await rows()).map((row) => row.text)).toEqual(["the opening line", "what this turn was given", "next door end 4"]);

    // And once it ends, the budget reaches it like any other run.
    await going.write({ kind: "end", text: "the end" });
    await archive.runs().pruneAgentLog({ agent: "bmo", keep: 1, maxBytes: Infinity, before: 0 });

    expect((await rows()).map((row) => row.text)).toEqual(["the end"]);
  });

  test("the age takes a run that never ended", async () => {
    const { archive, rows } = await logArchive();
    const abandoned = new RunLog({ archive, agent: "bmo", run: "chat:7:1000:1", chatId: 7 });

    setSystemTime(new Date(Date.now() - (maxAgeSeconds + 60) * 1000));

    try {
      await abandoned.write({ kind: "start", text: "left open last week" });
    } finally {
      setSystemTime();
    }

    await pruneRunLog(archive, "bmo");

    expect((await rows())).toEqual([]);
  });
});

describe("provider-specific worker messages", () => {
  test("Codex tools retain call identity and message updates do not duplicate completed text", () => {
    const item = { id: "cmd-1", type: "command_execution", command: "bun test", aggregated_output: "passed" };
    expect(runLogLines({ type: "item.started", item }, "codex")[0]).toMatchObject({ kind: "tool", name: "Bash", ref: "cmd-1" });
    expect(runLogLines({ type: "item.completed", item }, "codex")[0]).toMatchObject({ kind: "tool-result", ref: "cmd-1", text: "passed" });
    expect(runLogLines({ type: "item.updated", item: { type: "agent_message", text: "partial" } }, "codex")).toEqual([]);
    expect(runLogLines(turn[1], "codex")).toEqual([]);
    expect(runLogLines({ type: "item.completed", item: { id: "reason-1", type: "reasoning", text: "reviewing" } }, "codex")[0])
      .toMatchObject({ kind: "thinking", text: "reviewing", sourceId: "reason-1" });
  });
});

describe("an MCP call read back out of its logged name", () => {
  test("is its server and tool, on the Claude road's name and on Codex's", () => {
    expect(mcpCallOf("mcp__bmo__memory_add")).toEqual({ server: "bmo", tool: "memory_add" });
    expect(mcpCallOf("mcp__chrome-devtools__take_snapshot")).toEqual({ server: "chrome-devtools", tool: "take_snapshot" });
    expect(mcpCallOf("bmo/memory_add")).toEqual({ server: "bmo", tool: "memory_add" });
    expect(mcpCallOf("web/web_search")).toEqual({ server: "web", tool: "web_search" });
  });

  test("and nothing for a call that went to no server", () => {
    for (const name of ["Bash", "Read", "Agent(builder)", "MCP", "", null]) {
      expect(mcpCallOf(name)).toBeNull();
    }
  });
});

describe("what a line may not carry", () => {
  const stored = async (write: (log: RunLog) => Promise<void>): Promise<LogRow[]> => {
    const { archive, rows } = await logArchive();
    const log = new RunLog({ archive, agent: "bmo", run: "chat:7:1000:1", chatId: 7 });

    await write(log);

    return (await rows());
  };

  const call = async (log: RunLog, ref: string, name: string, input: unknown): Promise<void> => {
    await log.write({ kind: "tool", name, ref, text: JSON.stringify(input) });
  };

  test("a credential handed to a command is masked, and the command still reads", async () => {
    const rows = await stored(async (log) => {
      await call(log, "a", "Bash", { command: "curl -H 'Authorization: Bearer sk-ant-oat01-never-in-a-report' https://api.anthropic.com/v1/me" });
      await call(log, "b", "Bash", { command: "psql 'postgres://bmo:hunter2@localhost/bmo' -c 'select 1'", password: "hunter2" });
      await call(log, "c", "Bash", { command: "deploy --token ghp_0123456789abcdef --dry-run" });
      await call(log, "d", "Bash", { command: "TELEGRAM_BOT_TOKEN=8100:AAH-secret bun server/main.ts" });
      await call(log, "e", "Bash", { command: "bun client/scripts/user.ts set owner passw0rd1234" });
    });
    const texts = rows.map((row) => row.text);

    // The value is gone from every one of them.
    for (const secret of ["sk-ant-oat01-never-in-a-report", "hunter2", "ghp_0123456789abcdef", "AAH-secret", "passw0rd1234"]) {
      expect(texts.join("\n")).not.toContain(secret);
    }

    // And what says what the command was doing is all still there.
    expect(texts[0]).toContain("Bearer «masked value»");
    expect(texts[0]).toContain("https://api.anthropic.com/v1/me");
    expect(texts[2]).toContain("deploy --token «masked value» --dry-run");
    expect(texts[3]).toContain("TELEGRAM_BOT_TOKEN=«masked value» bun server/main.ts");
    expect(texts[4]).toContain("user.ts set owner «masked value»");

    // A field of the arguments that names a credential loses the whole of it, a password written
    // into an address loses that much of the address, and the arguments are still the object the
    // screen reads a command out of.
    expect(JSON.parse(texts[1]!)).toEqual({
      command: "psql 'postgres://bmo:«masked value»@localhost/bmo' -c 'select 1'",
      password: "«masked value»",
    });
  });

  // A browse through agency's own server takes the login apart from the task, and the log keeps the task
  // to read and none of the login, whatever shape it was handed in.
  test("a browse call keeps its task and loses its secrets whole", async () => {
    const rows = await stored(async (log) => {
      await call(log, "s", "mcp__bmo__browse", { task: "read this week's notices on the intranet board", secrets: "id=boss pw=Tr0ub4dor&3" });
      await call(log, "o", "mcp__bmo__browse", { task: "check the mailbox", secrets: { user: "boss", pw: "Tr0ub4dor&3" } });
    });

    expect(rows.map((row) => row.text).join("\n")).not.toContain("Tr0ub4dor");
    expect(rows.map((row) => JSON.parse(row.text))).toEqual([
      { task: "read this week's notices on the intranet board", secrets: "«masked value»" },
      { task: "check the mailbox", secrets: "«masked value»" },
    ]);
  });

  test("what a call to open a file of credentials handed back is not kept, and the call still names the file", async () => {
    const rows = await stored(async (log) => {
      await call(log, "read", "Read", { file_path: "/home/someone/.local/share/bmo/data/env/bot.env" });
      await log.write({ kind: "tool-result", ref: "read", text: "TELEGRAM_BOT_TOKEN=8100:AAH-real\nBMO_OWNER_ID=5601\nGREETING=hello" });
      await call(log, "ls", "Bash", { command: "ls -la src" });
      await log.write({ kind: "tool-result", ref: "ls", text: "archive.ts\nrunlog.ts" });
    });

    expect(rows[0]?.text).toContain("data/env/bot.env");
    expect(rows[1]?.text).toBe("«masked value»");
    // A call that opened nothing of the sort is left exactly as it stands, both ways.
    expect(rows[2]?.text).toBe('{"command":"ls -la src"}');
    expect(rows[3]?.text).toBe("archive.ts\nrunlog.ts");
  });

  test("a credential in what a tool handed back is masked by its shape", async () => {
    const rows = await stored(async (log) => {
      await call(log, "grep", "Bash", { command: "grep -r api_key client/app" });
      await log.write({ kind: "tool-result", ref: "grep", text: 'client/app/x.ts: const api_key = "sk-org-abcdefghijkl";\nclient/app/y.ts: password=hunter2' });
    });

    expect(rows[1]?.text).not.toContain("abcdefghijkl");
    expect(rows[1]?.text).not.toContain("hunter2");
    expect(rows[1]?.text).toContain("client/app/x.ts");
    expect(rows[1]?.text).toContain("client/app/y.ts");
  });

  test("the words of a run and of the bot are left alone", async () => {
    const rows = await stored(async (log) => {
      await log.write({ kind: "text", text: "the password is set as password=hunter2" });
      await log.write({ kind: "note", text: "Bearer sk-ant-something" });
    });

    expect(rows[0]?.text).toBe("the password is set as password=hunter2");
    expect(rows[1]?.text).toBe("Bearer sk-ant-something");
  });
});

describe("what the turn was handed", () => {
  test("stands as its own lines after the prompt, in the order it entered it", async () => {
    const { archive, rows } = await logArchive();
    const log = new RunLog({ archive, agent: "bmo", run: "chat:7:1000:1", chatId: 7, keepsLaunch: true });

    await log.launched({ systemPromptPath: null, prompt: "this turn" }, [
      { source: "room", text: "chat 7" },
      { source: "memory", text: "memories/7" },
      { source: "person", text: "people/42/sheet.md" },
      { source: "document", text: "STRUCTURE.md" },
      { source: "rule", text: "rooms/7/rules/greeting.md" },
      { source: "knowledge", text: "knowledge/deploy.md" },
      { source: "archive", text: "chat 7 · #10–#24 · 15 messages" },
    ]);

    expect((await rows()).map((row) => [row.kind, row.name, row.text])).toEqual([
      ["prompt", null, "this turn"],
      ["context", "room", "chat 7"],
      ["context", "memory", "memories/7"],
      ["context", "person", "people/42/sheet.md"],
      ["context", "document", "STRUCTURE.md"],
      ["context", "rule", "rooms/7/rules/greeting.md"],
      ["context", "knowledge", "knowledge/deploy.md"],
      ["context", "archive", "chat 7 · #10–#24 · 15 messages"],
    ]);
  });

  test("a log that does not keep what a run was handed writes none of it", async () => {
    const { archive, rows } = await logArchive();
    const gate = new RunLog({ archive, agent: "classifier", run: "gate:1:1000", chatId: 7 });

    await gate.launched({ systemPromptPath: null, prompt: "something to judge" }, [{ source: "document", text: "STRUCTURE.md" }]);

    expect((await rows())).toEqual([]);
  });

});

describe("a call made through the toolbook", () => {
  test("is logged on the Claude road as the tool it reached, and its result pairs with it", () => {
    const [call] = runLogLines({
      type: "assistant",
      message: { content: [{ type: "tool_use", id: "toolu_9", name: "mcp__toolbook__call", input: { tool: "web/naver_price", arguments: { query: "RTX 5090" } } }] },
    });
    const [result] = runLogLines({ type: "user", message: { content: [{ type: "tool_result", tool_use_id: "toolu_9", content: "lowest price 3,190,000 won" }] } });

    expect(call).toMatchObject({ kind: "tool", name: "mcp__web__naver_price", text: '{"query":"RTX 5090"}', metadata: { throughToolbook: true } });
    expect(result).toMatchObject({ kind: "tool-result", ref: call?.ref ?? "" });
  });

  test("is logged on the Codex road as the tool it reached, and its result pairs with it", () => {
    const item = { id: "mcp-7", type: "mcp_tool_call", server: "toolbook", tool: "act", arguments: { tool: "bmo/remind", arguments: { text: "check" } }, result: "set" };
    const [call] = runLogLines({ type: "item.started", item }, "codex");
    const [result] = runLogLines({ type: "item.completed", item }, "codex");

    expect(call).toMatchObject({ kind: "tool", name: "bmo/remind", ref: "mcp-7", text: '{"text":"check"}', metadata: { throughToolbook: true } });
    expect(result).toMatchObject({ kind: "tool-result", ref: "mcp-7" });
  });

  test("leaves list and describe as the toolbook's own calls", () => {
    const [list] = runLogLines({
      type: "assistant",
      message: { content: [{ type: "tool_use", id: "toolu_10", name: "mcp__toolbook__list", input: { query: "price" } }] },
    });
    const [described] = runLogLines({ type: "item.started", item: { id: "mcp-8", type: "mcp_tool_call", server: "toolbook", tool: "describe", arguments: { tool: "web/naver_price" } } }, "codex");

    expect(list).toMatchObject({ name: "mcp__toolbook__list", text: '{"query":"price"}' });
    expect(list?.metadata).toBeUndefined();
    expect(described).toMatchObject({ name: "toolbook/describe", text: '{"tool":"web/naver_price"}' });
    expect(described?.metadata).toBeUndefined();
  });
});
