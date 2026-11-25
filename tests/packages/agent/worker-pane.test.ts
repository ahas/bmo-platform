import "@agency/test-support/setup";

import { Archive } from "@agency/archive/archive";
import { installProviderSettings, setProviderEnabled } from "@agency/agent/provider-order";
import { beforeEach, afterAll, afterEach, describe, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { appendFileSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { transcriptPath } from "@agency/agent/freshstart";
import { scratchDir } from "@agency/test-support/scratch";

import { folderTrustKeys, lastActivityOf, pluginOfferKeys, newLines, pastedLead, personTurnDone, promptedSince, readTurn, residentOf, runTurnInPane, spendSince } from "@agency/agent/worker-pane";
import type { TurnOverdue } from "@agency/agent/overtime-clock";
import type { RunLog } from "@agency/agent/runlog";
import { recordCall } from "@agency/agent/agent-place";
import type { PaneConfirmation } from "@agency/agent/pane-confirm";
import { recordStarted } from "@agency/agent/resident-pane";
import { scratchArchive, scratchDatabase } from "@agency/test-support/store";

// A record is not a stream, and what is held here is every place the two differ in a way that
// would be wrong in silence: a turn spanning several lines, a co-worker's lines mixed into the
// same file, and the two figures a record simply does not have.

const assistant = (options: {
  text?: string;
  model?: string;
  effort?: string;
  sidechain?: boolean;
  read?: number;
  made?: number;
  in?: number;
  out?: number;
  tool?: { id: string; name: string; input?: Record<string, unknown> };
}) => ({
  type: "assistant",
  isSidechain: options.sidechain ?? false,
  effort: options.effort ?? "high",
  message: {
    role: "assistant",
    model: options.model ?? "claude-opus-5",
    content: [
      ...(options.text === undefined ? [] : [{ type: "text", text: options.text }]),
      ...(options.tool === undefined
        ? []
        : [{ type: "tool_use", id: options.tool.id, name: options.tool.name, input: options.tool.input ?? {} }]),
    ],
    usage: {
      input_tokens: options.in ?? 0,
      output_tokens: options.out ?? 0,
      cache_read_input_tokens: options.read ?? 0,
      cache_creation_input_tokens: options.made ?? 0,
    },
  },
});

const answered = (id: string) => ({
  type: "user",
  message: { role: "user", content: [{ type: "tool_result", tool_use_id: id, content: "done" }] },
});

// The folder-trust question as a real pane showed it, measured in a fresh clone on the worker's own
// line: the cursor stands on refusing.
const trustScreen = [
  " Accessing workspace:",
  " /tmp/t308b5/checkout",
  " Quick safety check: Is this a project you created or one you trust? (Like your own code, a well-known open source",
  " project, or work from your team). If not, take a moment to review what's in this folder first.",
  " Claude Code'll be able to read, edit, and execute files here.",
  " Security guide",
  " ❯ No, exit",
  "   Yes, I trust this folder",
  " Enter to confirm · Esc to cancel",
].join("\n");

describe("the folder-trust question", () => {
  test("is accepted by moving down to the accepting row and confirming", () => {
    expect(folderTrustKeys(trustScreen)).toEqual(["down", "enter"]);
  });

  test("is confirmed where it stands when the cursor is already on accepting", () => {
    expect(folderTrustKeys(trustScreen.replace(" ❯ No, exit", "   No, exit").replace("   Yes, I trust", " ❯ Yes, I trust"))).toEqual(["enter"]);
  });

  // Enter on the cursor's own row refuses and ends the CLI, so a screen whose cursor cannot be found
  // is not answered at all.
  test("is not answered when it is not on the screen, or its cursor cannot be read", () => {
    expect(folderTrustKeys("a screen")).toBeNull();
    expect(folderTrustKeys("❯ Try \"write a test for <filepath>\"")).toBeNull();
    expect(folderTrustKeys(trustScreen.replace("❯", " "))).toBeNull();
  });
});

// The offer as a worker's pane drew it before its turn could be put in.
const offerScreen = [
  " LSP plugin recommendation",
  "   Plugin: rust-analyzer-lsp",
  "   Triggered by: .rs files",
  "   Would you like to install this LSP plugin?",
  "   ❯ 1. Yes, install",
  "     2. No, not now",
  "     3. Never for this plugin",
  "     4. Disable all LSP recommendations",
].join("\n");

describe("the plugin offer", () => {
  test("is declined for every plugin by moving down to the last row and confirming", () => {
    expect(pluginOfferKeys(offerScreen)).toEqual(["down", "down", "down", "enter"]);
  });

  test("is not answered when it is not on the screen, or its cursor cannot be read", () => {
    expect(pluginOfferKeys(offerScreen.replace("LSP plugin recommendation", "A question"))).toBeNull();
    expect(pluginOfferKeys(offerScreen.replace("❯", " "))).toBeNull();
  });
});

describe("reading a turn out of the record", () => {
  // Measured on a real ping: one response written as a thinking line and a text line, both carrying
  // the same message id and the whole response's usage. Adding the lines up counts the request twice.
  test("a response split into several lines is counted once and answers with all of its text", () => {
    const split = (block: Record<string, unknown>) => {
      const line = assistant({ made: 53_544, in: 2, out: 100 });

      return { ...line, message: { ...line.message, id: "msg_one", content: [block] } };
    };
    const turn = readTurn(
      [split({ type: "thinking", thinking: "…" }), split({ type: "text", text: "Kept " }), split({ type: "text", text: "warm." })],
      0,
    );

    expect(turn.spend).toEqual([
      {
        mode: "claude-code",
        model: "claude-opus-5",
        tokens: 53_646,
        inputTokens: 2,
        outputTokens: 100,
        cacheReadTokens: 0,
        cacheWriteTokens: 53_544,
        cost: 0,
      },
    ]);
    expect(turn.text).toBe("Kept warm.");
  });

  test("the answer is the last thing the run said", () => {
    const turn = readTurn([assistant({ text: "thinking about it" }), assistant({ text: "the answer" })], 0);

    expect(turn.text).toBe("the answer");
  });

  // A turn that uses a tool writes two assistant lines, and the cost of the turn is both of them.
  // Reading only the last one would report a turn that used ten tools as though it used none.
  test("the cost is every line the turn added", () => {
    const turn = readTurn(
      [
        assistant({ tool: { id: "t1", name: "Bash" }, read: 33330, made: 31080, in: 2, out: 122 }),
        answered("t1"),
        assistant({ text: "DONE", read: 64410, made: 161, in: 2, out: 5 }),
      ],
      0,
    );

    expect(turn.spend).toEqual([
      {
        mode: "claude-code",
        model: "claude-opus-5",
        tokens: 33330 + 31080 + 2 + 122 + 64410 + 161 + 2 + 5,
        inputTokens: 4,
        outputTokens: 127,
        cacheReadTokens: 33330 + 64410,
        cacheWriteTokens: 31080 + 161,
        cost: 0,
      },
    ]);
  });

  test("what came before this turn is not this turn's", () => {
    const records = [assistant({ text: "an earlier turn", out: 500 }), assistant({ text: "this one", out: 7 })];
    const turn = readTurn(records, 1);

    expect(turn.text).toBe("this one");
    expect(turn.spend[0]?.outputTokens).toBe(7);
  });

  // A co-worker writes into the same file. Its tokens were spent by this run and count; its answer
  // was to the agent that asked, not to whoever is reading the run.
  test("a co-worker's lines are paid for but not read as the answer", () => {
    const turn = readTurn(
      [
        assistant({ text: "the answer", out: 10 }),
        assistant({ text: "a co-worker's report", sidechain: true, out: 4_000, model: "claude-opus-5" }),
      ],
      0,
    );

    expect(turn.text).toBe("the answer");
    expect(turn.spend[0]?.outputTokens).toBe(4_010);
  });

  test("two models in one turn are two rows", () => {
    const turn = readTurn(
      [
        assistant({ text: "the answer", out: 10 }),
        assistant({ sidechain: true, model: "claude-haiku-4-5", out: 30 }),
      ],
      0,
    );

    expect(turn.spend.map((one) => [one.model, one.outputTokens])).toEqual([
      ["claude-opus-5", 10],
      ["claude-haiku-4-5", 30],
    ]);
  });

  test("the model and the effort come off the line that answered", () => {
    const turn = readTurn([assistant({ text: "ok", model: "claude-opus-5", effort: "high" })], 0);

    expect(turn.model).toBe("claude-opus-5");
    expect(turn.effort).toBe("high");
  });

  // What the model was holding is stated per request rather than as a share of a window, so it is
  // the last request's own three figures and never a ratio worked out from them.
  test("the context is what the last request carried", () => {
    const turn = readTurn([assistant({ text: "ok", read: 64_410, made: 161, in: 2 })], 0);

    expect(turn.contextTokens).toBe(64_573);
  });

  test("a turn that added nothing says so rather than guessing", () => {
    const turn = readTurn([assistant({ text: "an earlier turn" })], 1);

    expect(turn.text).toBe("");
    expect(turn.model).toBeNull();
    expect(turn.contextTokens).toBeNull();
    expect(turn.spend).toEqual([]);
  });

  test("several text blocks in one line are one answer", () => {
    const turn = readTurn(
      [
        {
          type: "assistant",
          isSidechain: false,
          message: {
            role: "assistant",
            model: "claude-opus-5",
            content: [
              { type: "text", text: "first line\n" },
              { type: "text", text: "second line" },
            ],
            usage: {},
          },
        },
      ],
      0,
    );

    expect(turn.text).toBe("first line\nsecond line");
  });
});

// A record says what was asked, what came back and, later, how background work ended. The wordings
// here are the CLI's own, as real transcripts carry them: a launch is answered at once, and its
// ending arrives as a task notification naming the call.
const launchedAs = (id: string, text: string) => ({
  type: "user",
  message: { role: "user", content: [{ type: "tool_result", tool_use_id: id, content: [{ type: "text", text }] }] },
});
const endedAs = (id: string, status: string) => ({
  type: "queue-operation",
  operation: "enqueue",
  content: `<task-notification>\n<task-id>t-${id}</task-id>\n<tool-use-id>${id}</tool-use-id>\n<status>${status}</status>\n<summary>done</summary>\n</task-notification>`,
});
const workflowLaunch = "Workflow launched in background. Task ID: wqdscirps\nSummary: two answers";

describe("whether a prompt was submitted", () => {
  // A program started again for the turn writes lines of its own as it comes up, and a pane that has only
  // just come up can read at rest from the program before it: neither is the prompt having gone in.
  test("only a person's prompt after the point counts, never a line the program wrote coming up or a tool's answer", () => {
    const before = [{ type: "user", message: { role: "user", content: "an earlier turn" } }];
    const coming = [
      { type: "system", subtype: "local_command" },
      { type: "attachment" },
      { type: "user", isMeta: true, message: { role: "user", content: "<local-command-caveat>" } },
      { type: "user", message: { role: "user", content: [{ type: "tool_result", content: "done" }] } },
      { type: "user", isSidechain: true, message: { role: "user", content: "a co-worker's" } },
    ];

    expect(promptedSince([...before, ...coming], before.length)).toBe(false);
    expect(promptedSince([...before, ...coming, { type: "user", message: { role: "user", content: "the job" } }], before.length)).toBe(true);
    expect(promptedSince([...before, { type: "user", message: { role: "user", content: [{ type: "text", text: "the job" }] } }], before.length)).toBe(true);
  });
});

describe("the work a run walked away from", () => {
  const workflow = (name: string) =>
    assistant({ tool: { id: "w1", name: "Workflow", input: { script: `export const meta = { name: '${name}', description: 'x' }` } } });

  test("a Claude Workflow launched and never heard from again is one it left behind", () => {
    const turn = readTurn([workflow("review-changes"), launchedAs("w1", workflowLaunch), assistant({ text: "going" })], 0);

    expect(turn.workflows).toEqual([{ call: "w1", name: "review-changes", status: null }]);
  });

  test("a Claude Workflow whose ending was reported carries that ending", () => {
    const stopped = readTurn([workflow("stopped-one"), launchedAs("w1", workflowLaunch), endedAs("w1", "stopped")], 0);
    const takenUp = { type: "attachment", attachment: { type: "queued_command", prompt: endedAs("w1", "completed").content } };
    const finished = readTurn([workflow("done-one"), launchedAs("w1", workflowLaunch), takenUp], 0);

    expect(stopped.workflows).toEqual([{ call: "w1", name: "stopped-one", status: "stopped" }]);
    expect(finished.workflows).toEqual([{ call: "w1", name: "done-one", status: "completed" }]);
  });

  test("a Claude Workflow the CLI refused to launch is no Claude Workflow at all", () => {
    const turn = readTurn([workflow("broken"), launchedAs("w1", "<tool_use_error>Invalid workflow script</tool_use_error>")], 0);

    expect(turn.workflows).toEqual([]);
  });

  test("a branch cut off before it answered is a task still going", () => {
    const turn = readTurn(
      [assistant({ tool: { id: "b1", name: "Task", input: { subagent_type: "scout", description: "find the caller" } } })],
      0,
    );

    expect(turn.liveTasks).toEqual([{ id: "b1", description: "find the caller" }]);
  });

  test("a branch that answered is not", () => {
    const turn = readTurn(
      [assistant({ tool: { id: "b1", name: "Task", input: { subagent_type: "scout" } } }), answered("b1")],
      0,
    );

    expect(turn.liveTasks).toEqual([]);
  });

  test("a command sent to the background is going until its ending is reported", () => {
    const launch = [
      assistant({ tool: { id: "c1", name: "Bash", input: { command: "bun run test", description: "seed the repository" } } }),
      launchedAs("c1", "Command running in background with ID: bz1925uj5. Output is being written to: /tmp/x.output."),
    ];

    expect(readTurn(launch, 0).liveTasks).toEqual([{ id: "c1", description: "seed the repository" }]);
    expect(readTurn([...launch, endedAs("c1", "completed")], 0).liveTasks).toEqual([]);
  });

  // A session carries every turn it ever had, and a launch some earlier run abandoned was that
  // run's to be judged by.
  test("a launch an earlier turn left open is not this turn's", () => {
    const records = [workflow("left-behind"), launchedAs("w1", workflowLaunch), assistant({ text: "done" })];

    expect(readTurn(records, 2).workflows).toEqual([]);
  });

  test("an ordinary tool call is neither a branch nor a Claude Workflow", () => {
    const turn = readTurn([assistant({ tool: { id: "t1", name: "Bash", input: { command: "ls" } } }), answered("t1")], 0);

    expect(turn.liveTasks).toEqual([]);
    expect(turn.workflows).toEqual([]);
  });
});

// What a run cost is the CLI's own account, written as its process ends and stating the whole
// session's total — measured: absent after two turns in a live pane, present once the pane closed.
describe("what a run cost", () => {
  const account = (usage: Record<string, Record<string, number>>) => ({ type: "cost-state", modelUsage: usage });

  test("is the account written after the run began, less the one written before it", () => {
    const records = [
      account({ "claude-opus-5": { inputTokens: 4, outputTokens: 10, cacheReadInputTokens: 0, cacheCreationInputTokens: 53_544, costUSD: 0.54 } }),
      assistant({ text: "again" }),
      account({
        "claude-opus-5": { inputTokens: 6, outputTokens: 15, cacheReadInputTokens: 53_544, cacheCreationInputTokens: 57_400, costUSD: 0.60 },
        "claude-haiku-4-5-20251001": { inputTokens: 1_790, outputTokens: 24, costUSD: 0.002 },
      }),
    ];
    const spend = spendSince(records, 1) ?? [];

    expect(spend.map((one) => [one.model, one.inputTokens, one.outputTokens, one.cacheReadTokens, one.cacheWriteTokens])).toEqual([
      ["claude-opus-5", 2, 5, 53_544, 3_856],
      ["claude-haiku-4-5-20251001", 1_790, 24, 0, 0],
    ]);
    expect(spend[0]?.cost).toBeCloseTo(0.06, 6);
  });

  test("is nothing where no account was written after the run began, or it names no model", () => {
    expect(spendSince([account({ "claude-opus-5": { costUSD: 1 } }), assistant({ text: "x" })], 1)).toBeNull();
    expect(spendSince([assistant({ text: "x" }), account({})], 0)).toBeNull();
  });
});

// The CLI writes a failed request into the record as a line of its own, in the answer's place.
describe("a turn that ended on the CLI's own error", () => {
  test("carries the error's wording and is not an answer", () => {
    const limit = { ...assistant({ text: "You've hit your session limit · resets 8pm (Asia/Seoul)", model: "<synthetic>" }), isApiErrorMessage: true };

    expect(readTurn([limit], 0).error).toBe("You've hit your session limit · resets 8pm (Asia/Seoul)");
    expect(readTurn([assistant({ text: "the answer" })], 0).error).toBeNull();
  });
});

// The console follows a run as it works, and it follows it by reading the same lines it always
// read: a record's assistant and user lines are a stream's assistant and user events, so they go
// over as they stand rather than being translated a second time here.
describe("what the log is fed while the run is going", () => {
  test("an assistant line and a tool answer both go over, in the stream's own shape", () => {
    const result = { status: "completed", structuredPatch: [{ oldStart: 1, newStart: 1 }] };
    const fed = newLines(
      [
        { ...assistant({ text: "at it", tool: { id: "t1", name: "Bash", input: { command: "ls" } } }), uuid: "call-row" },
        { ...answered("t1"), uuid: "result-row", toolUseResult: result },
      ],
      0,
    );
    const events = fed.trim().split("\n").map((line) => JSON.parse(line) as {
      type: string;
      uuid: string;
      toolUseResult?: unknown;
      message: { content: unknown[] };
    });

    expect(events.map((event) => event.type)).toEqual(["assistant", "user"]);
    expect(events[0]?.message.content).toEqual([
      { type: "text", text: "at it" },
      { type: "tool_use", id: "t1", name: "Bash", input: { command: "ls" } },
    ]);
    expect(events[1]?.message.content).toEqual([{ type: "tool_result", tool_use_id: "t1", content: "done" }]);
    expect(events.map((event) => event.uuid)).toEqual(["call-row", "result-row"]);
    expect(events[1]?.toolUseResult).toEqual(result);
  });

  test("only what arrived since the last look goes over, so nothing is logged twice", () => {
    const records = [assistant({ text: "first" }), assistant({ text: "second" })];

    expect(newLines(records, 1).trim().split("\n")).toHaveLength(1);
    expect(newLines(records, 1)).toContain("second");
    expect(newLines(records, 1)).not.toContain("first");
  });

  test("nothing new is nothing fed, rather than an empty line the reader would have to skip", () => {
    expect(newLines([assistant({ text: "only this" })], 1)).toBe("");
  });

  // The record carries the CLI's own bookkeeping beside the conversation, and none of it is an
  // event the log knows how to read.
  test("the CLI's own bookkeeping lines are not events and do not go over", () => {
    const fed = newLines([{ type: "ai-title", message: { role: "assistant" } }, assistant({ text: "the answer" })], 0);

    expect(fed.trim().split("\n")).toHaveLength(1);
    expect(fed).toContain("the answer");
  });
});

// The turn itself, against a stand-in herdr. What is held here is the one property the whole
// channel exists for — that a word can reach the run while it works — and the sequencing that
// decides whether it reaches the right turn.
// The settings archive the turns read their providers from, made once for the file.
const providerArchive = await scratchArchive();

describe("a turn driven over a socket", () => {
  beforeEach(() => installProviderSettings(providerArchive));
  afterEach(() => installProviderSettings(null));
  const servers: { stop: () => void }[] = [];
  const written: string[] = [];

  afterEach(() => {
    for (const server of servers.splice(0)) {
      server.stop();
    }

    for (const path of written.splice(0)) {
      rmSync(path, { force: true });
    }
  });

  interface Driven {
    path: string;
    asked: { method: string; params: Record<string, unknown> }[];
    // The statuses agent.list answers with, one per look, the last repeating.
    statuses: string[];
    stop(): void;
  }

  // A herdr of our own, and a transcript written where the session id says it will be.
  function herdrLike(
    name: string,
    options: {
      onRequest?: (method: string) => void | Promise<unknown>;
      statuses: string[];
      transcript: unknown[];
      cwd: string;
      sessionId: string;
      // What the pane shows, read by read, and the statuses it goes through once it has been sent
      // keys, by how many times it has been.
      screen?: string | ((read: number) => string);
      afterKeys?: string[] | ((sent: number) => string[]);
      // The account the CLI writes as its process ends, model by model. None names no model.
      account?: Record<string, Record<string, number>>;
      // Lines of the answer that reach the record only this long after the pane is back at rest,
      // ahead of the line that closes the turn.
      late?: { lines: unknown[]; afterMs: number };
      // Whether herdr answers this list call — agent.list and pane.list counted together from one —
      // or fails it, as herdr does when it is stalled.
      silent?: (call: number) => boolean;
    },
  ): Driven {
    const path = join(tmpdir(), `bmo-worker-pane-${name}-${process.pid}.sock`);

    rmSync(path, { force: true });

    const kept: Driven = { path, asked: [], statuses: [...options.statuses], stop: () => { server.stop(true); rmSync(path, { force: true }); } };
    let looks = 0;
    let reads = 0;
    let sends = 0;
    let worked = false;
    let closed = false;
    let listCalls = 0;
    // Words typed into the program at rest after its turn closed, which the next look finds taken.
    let prompted: string | null = null;
    // The status the last agent.list answered with. `gone` is a pane herdr no longer lists at all.
    let standing = "";
    const answer = (method: string, params: Record<string, unknown>): unknown => {
      if (method === "workspace.create") {
        return { workspace: { workspace_id: "w1" }, tab: { tab_id: "w1:t1" }, root_pane: { pane_id: "w1:p1" } };
      }

      if (method === "layout.apply") {
        return { layout: { root: { pane_id: "w1:p2" } } };
      }

      if (method === "agent.list") {
        const at = Math.min(looks, kept.statuses.length - 1);

        looks += 1;

        if (prompted !== null) {
          appendFileSync(transcriptPath(options.cwd, options.sessionId), `\n${[{ type: "user", message: { role: "user", content: prompted } }, { type: "system", subtype: "turn_duration" }].map((one) => JSON.stringify(one)).join("\n")}`);
          prompted = null;
        }

        // The record appears once the turn is under way, which is what the CLI does.
        if (kept.statuses[at] === "working") {
          const file = transcriptPath(options.cwd, options.sessionId);

          mkdirSync(dirname(file), { recursive: true });
          writeFileSync(file, options.transcript.map((one) => JSON.stringify(one)).join("\n"));
          written.push(file);
          worked = true;
        }

        // Back at rest after the work, the turn is closed with the line the CLI ends one with.
        if (worked && !closed && (kept.statuses[at] === "idle" || kept.statuses[at] === "done")) {
          const file = transcriptPath(options.cwd, options.sessionId);
          const close = () => appendFileSync(file, `\n${[...(options.late?.lines ?? []), { type: "system", subtype: "turn_duration" }].map((one) => JSON.stringify(one)).join("\n")}`);

          closed = true;

          if (options.late === undefined) {
            close();
          } else {
            setTimeout(close, options.late.afterMs);
          }
        }

        standing = kept.statuses[at] as string;

        return { agents: standing === "gone" ? [] : [{ pane_id: "w1:p2", agent: "claude", agent_status: standing }] };
      }

      if (method === "pane.list") {
        return { panes: standing === "gone" ? [] : [{ pane_id: "w1:p2", workspace_id: "w1" }] };
      }

      if (method === "pane.read") {
        const screen = options.screen ?? "a screen";

        reads += 1;

        return { read: { text: typeof screen === "string" ? screen : screen(reads) } };
      }

      if (method === "pane.close") {
        const file = transcriptPath(options.cwd, options.sessionId);

        mkdirSync(dirname(file), { recursive: true });
        appendFileSync(file, `\n${JSON.stringify({ type: "cost-state", modelUsage: options.account ?? {} })}\n`);
        written.push(file);
      }

      if (method === "pane.send_keys" && options.afterKeys !== undefined) {
        sends += 1;
        kept.statuses = typeof options.afterKeys === "function" ? options.afterKeys(sends) : [...options.afterKeys];
        looks = 0;
      }

      // Words typed into a program at rest after its turn closed are a prompt of their own, which it
      // takes and closes as a turn by the next look.
      if (method === "agent.prompt" && closed && (standing === "idle" || standing === "done")) {
        prompted = String(params.text ?? "");
      }

      // A word put into a working turn is queued at once and taken at the next tool result.
      if (method === "agent.prompt" && standing === "working") {
        const file = transcriptPath(options.cwd, options.sessionId);

        appendFileSync(file, `\n${[{ type: "queue-operation", operation: "enqueue" }, { type: "queue-operation", operation: "remove" }].map((one) => JSON.stringify(one)).join("\n")}`);
      }

      return { type: "ok" };
    };
    const server = Bun.listen<undefined>({
      unix: path,
      socket: {
        async data(connection, chunk) {
          for (const line of chunk.toString().split("\n")) {
            if (line.trim() === "") {
              continue;
            }

            const request = JSON.parse(line) as { id: string; method: string; params?: Record<string, unknown> };

            kept.asked.push({ method: request.method, params: request.params ?? {} });
            await options.onRequest?.(request.method);

            const listing = request.method === "agent.list" || request.method === "pane.list";

            if (listing && options.silent?.(++listCalls) === true) {
              connection.write(`${JSON.stringify({ id: request.id, error: { code: "timeout", message: "herdr is stalled" } })}\n`);
              connection.end();
              continue;
            }

            connection.write(`${JSON.stringify({ id: request.id, result: answer(request.method, request.params ?? {}) })}\n`);
            connection.end();
          }
        },
      },
    });

    servers.push(kept);

    return kept;
  }

  const cwd = "/tmp/bmo-worker-pane-turn";
  const sessionId = "11111111-2222-3333-4444-555555555555";

  for (const refusal of ["disabled", "cancelled", "missing-archive"] as const) {
    test(`interactive ${refusal} before delivery never sends a prompt`, async () => {
      const archive = await scratchArchive();
      installProviderSettings(refusal === "missing-archive" ? null : archive);
      const herdr = herdrLike(refusal, {
        statuses: ["idle"], transcript: [], cwd, sessionId,
        async onRequest(method) {
          if (method === "pane.report_metadata") expect(await archive.executions().executions()).toHaveLength(1);
          if (method === "agent.list") {
            if (refusal === "disabled") await setProviderEnabled(archive, "claude-code", false);
            if (refusal === "cancelled") await archive.settings().setSetting("execution_cancel:test-start", "1");
          }
        },
      });
      await expect(runTurnInPane({name: "test-start", title: "test", cwd, command: ["claude"], env: {}, prompt: "must not send", sessionId, from: 0, socket: herdr.path, pollMs: 1})).rejects.toThrow();
      expect(herdr.asked.some(call => call.method === "agent.prompt")).toBe(false);
      if (refusal === "missing-archive") expect(herdr.asked).toEqual([]);
      else {
        expect((await archive.executions().executions())[0]).toMatchObject({ state: "launch-error" });
        expect((await archive.executions().executions())[0]!.endedAt).not.toBeNull();
        expect(herdr.asked.some(call => call.method === "pane.close")).toBe(true);
      }
    });
  }

  test("the turn is told its work, watched until it stops, and read out of the record", async () => {
    const archive = await scratchArchive();
    installProviderSettings(archive);
    const herdr = herdrLike("plain", {
      statuses: ["idle", "working", "working", "idle"],
      transcript: [assistant({ tool: { id: "native-1", name: "Agent" } }), answered("native-1"), assistant({ text: "the answer", out: 11 })],
      cwd,
      sessionId,
    });
    const turn = await runTurnInPane({
      name: "bmo-run-9-build-0-1",
      title: "#9 a build",
      cwd,
      command: ["claude", "--session-id", sessionId],
      env: {},
      prompt: "the work",
      sessionId,
      from: 0,
      pollMs: 5,
      socket: herdr.path,
    });

    installProviderSettings(null);
    expect((await archive.executions().executions()).find(row => row.line === "native-child")).toMatchObject({ sharedPane: true, state: "unknown", nativeState: "unresolved" });
    expect((await archive.executions().executions()).find(row => row.line === "worker")).toMatchObject({ parent: "bmo-run-9-build-0-1", state: "exited" });
    expect(turn.outcome).toBe("answered");
    expect(turn.outcome === "answered" ? turn.reply.text : "").toBe("the answer");
    expect(turn.outcome === "answered" ? turn.reply.sessionId : "").toBe(sessionId);
    // Told after it was ready, and never with a wait attached — a wait would hold this call for
    // the whole length of the turn.
    const told = herdr.asked.filter((one) => one.method === "agent.prompt");

    expect(told).toHaveLength(1);
    expect(told[0]?.params.text).toBe("the work");
    expect(told[0]?.params).not.toHaveProperty("wait");
    // The pane and never its workspace, which a standing agent's head and other runs may share.
    expect(herdr.asked.some((one) => one.method === "pane.close")).toBe(true);
    expect(herdr.asked.some((one) => one.method === "workspace.close")).toBe(false);
  });

  // Herdr stalls for seconds under a burst of calls, and every turn on the machine asks it at once:
  // a look it does not answer is no word on the pane (docs/agent-control.md, 「A pane is gone only on
  // herdr's own word」).
  describe("a herdr that does not answer", () => {
    const turnOn = (herdr: Driven, extra: Partial<Parameters<typeof runTurnInPane>[0]> = {}) => runTurnInPane({
      name: "bmo-run-9-build-0-1", title: "#9 a build", cwd, command: ["claude", "--session-id", sessionId],
      env: {}, prompt: "the work", sessionId, from: 0, pollMs: 5, socket: herdr.path, goneConfirm: { gapMs: 1 }, ...extra,
    });

    const noted = () => {
      const notes: string[] = [];

      return { notes, log: { write: (line: { text?: string }) => notes.push(line.text ?? ""), feed: () => {} } as unknown as RunLog };
    };

    // A look whose call herdr fails is a silent look, so four silent calls are four silent looks.
    test("for a look or two does not end the turn, which notes herdr back and answers once it is", async () => {
      installProviderSettings(await scratchArchive());
      const herdr = herdrLike("stalled", {
        statuses: ["idle", "working", "working", "working", "idle"],
        transcript: [assistant({ text: "the answer", out: 11 })],
        cwd,
        sessionId,
        silent: (call) => call >= 4 && call <= 7,
      });
      const { notes, log } = noted();
      const turn = await turnOn(herdr, { log });

      installProviderSettings(null);
      expect(turn.outcome).toBe("answered");

      const began = notes.findIndex((note) => note.startsWith("herdr is not answering the question about the pane, so the turn waits reading the record"));
      const ended = notes.findIndex((note) => /^herdr answered again after \d+ seconds; the pane is working$/.test(note));

      expect(began).toBeGreaterThanOrEqual(0);
      expect(ended).toBeGreaterThan(began);
    });

    test("a pane missing from one answered look but listed at the next is not gone", async () => {
      installProviderSettings(await scratchArchive());
      const herdr = herdrLike("blink", {
        statuses: ["idle", "working", "gone", "working", "idle"],
        transcript: [assistant({ text: "the answer", out: 11 })],
        cwd,
        sessionId,
      });
      const turn = await turnOn(herdr);

      installProviderSettings(null);
      expect(turn.outcome).toBe("answered");
    });

    test("a pane herdr answers is gone, look after look, ends the turn as gone", async () => {
      installProviderSettings(await scratchArchive());
      const herdr = herdrLike("closed", { statuses: ["idle", "working", "gone"], transcript: [assistant({ text: "half" })], cwd, sessionId });
      const turn = await turnOn(herdr);

      installProviderSettings(null);
      expect(turn.outcome).toBe("vanished");
      expect(herdr.asked.filter((one) => one.method === "pane.list").length).toBeGreaterThanOrEqual(3);
    });

    test("however long it lasts ends nothing: the turn waits it out and answers once herdr does", async () => {
      installProviderSettings(await scratchArchive());
      const herdr = herdrLike("down", {
        statuses: ["idle", "working", "working", "idle"],
        transcript: [assistant({ text: "the answer", out: 11 })],
        cwd,
        sessionId,
        silent: (call) => call >= 4 && call <= 200,
      });
      const turn = await turnOn(herdr);

      installProviderSettings(null);
      expect(turn.outcome).toBe("answered");
    });
  });

  test("the cost is read out of the account the CLI writes once its pane is closed", async () => {
    const herdr = herdrLike("account", {
      statuses: ["idle", "working", "idle"],
      transcript: [assistant({ text: "done", read: 100, made: 10, in: 2, out: 5 })],
      account: { "claude-opus-5": { inputTokens: 2, outputTokens: 5, cacheReadInputTokens: 100, cacheCreationInputTokens: 10, costUSD: 0.25 } },
      cwd,
      sessionId,
    });
    const turn = await runTurnInPane({
      name: "bmo-run-9-build-0-1",
      title: "#9 a build",
      cwd,
      command: ["claude", "--session-id", sessionId],
      env: {},
      prompt: "the work",
      sessionId,
      from: 0,
      pollMs: 5,
      socket: herdr.path,
    });

    expect(turn.outcome === "answered" ? turn.reply.spend : []).toEqual([
      { mode: "claude-code", model: "claude-opus-5", tokens: 117, inputTokens: 2, outputTokens: 5, cacheReadTokens: 100, cacheWriteTokens: 10, cost: 0.25 },
    ]);
  });

  // A long answer's words are written after its thinking, once their stream is over, and the pane
  // can read as at rest before they land (docs/agent-control.md 「The order for putting in one turn and waiting for it to end」).
  test("a turn back at rest is read only once its record says the turn is over, however late the answer's words land", async () => {
    const response = (content: unknown[]) => ({ type: "assistant", isSidechain: false, message: { id: "msg-plan", role: "assistant", model: "claude-opus-5", content, usage: {} } });
    const herdr = herdrLike("late", {
      statuses: ["idle", "working", "idle"],
      transcript: [response([{ type: "thinking", thinking: "" }])],
      late: { lines: [response([{ type: "text", text: "the plan" }])], afterMs: 150 },
      cwd,
      sessionId,
    });
    const turn = await runTurnInPane({
      name: "bmo-run-9-plan-1",
      title: "#9 a plan",
      cwd,
      command: ["claude", "--session-id", sessionId],
      env: {},
      prompt: "plan it",
      sessionId,
      from: 0,
      pollMs: 5,
      socket: herdr.path,
    });

    expect(turn.outcome).toBe("answered");
    expect(turn.outcome === "answered" ? turn.reply.text : "").toBe("the plan");
  });

  // The property the item was raised for.
  test("a word is pushed into the turn while it works, and never before or after", async () => {
    // Two idles before the work starts: the first is eaten waiting for the pane to be ready, and
    // the second falls while the prompt is being seen submitted — which is where a word must not go,
    // because the turn's own prompt has not been taken up yet. The first look at work is the one
    // that sees it submitted, and the next falls inside the loop.
    const herdr = herdrLike("aside", {
      statuses: ["idle", "idle", "working", "working", "idle"],
      transcript: [assistant({ text: "done", out: 2 })],
      cwd,
      sessionId,
    });
    const asked: string[] = [];

    await runTurnInPane({
      name: "bmo-run-9-build-0-1",
      title: "#9 a build",
      cwd,
      command: ["claude", "--session-id", sessionId],
      env: {},
      prompt: "the work",
      sessionId,
      from: 0,
      pollMs: 5,
      socket: herdr.path,
      aside: async (say) => {
        asked.push("asked");
        await say("a word appended while it works");
      },
    });

    // Asked on each look of the loop that found it working, and on no other: the look that saw the
    // prompt submitted is not one of them.
    expect(asked.length).toBeGreaterThan(0);
    expect(asked).toHaveLength(herdr.statuses.filter((one) => one === "working").length - 1);

    const words = herdr.asked.filter((one) => one.method === "agent.prompt").map((one) => one.params.text);

    expect(words[0]).toBe("the work");
    expect(words.slice(1)).toEqual(Array.from({ length: asked.length }, () => "a word appended while it works"));
  });

  test("a turn whose last word was a usage limit is not answered, and carries the limit's wording", async () => {
    const limit = { ...assistant({ text: "You've hit your session limit · resets 8pm (Asia/Seoul)", model: "<synthetic>" }), isApiErrorMessage: true };
    const herdr = herdrLike("limit", { statuses: ["idle", "working", "idle"], transcript: [limit], cwd, sessionId });
    const turn = await runTurnInPane({
      name: "bmo-run-9-build-0-1",
      title: "#9 a build",
      cwd,
      command: ["claude", "--session-id", sessionId],
      env: {},
      prompt: "the work",
      sessionId,
      from: 0,
      pollMs: 5,
      socket: herdr.path,
    });

    expect(turn).toEqual({ outcome: "errored", error: "You've hit your session limit · resets 8pm (Asia/Seoul)" });
  });

  // A marker by itself only refuses the next launch. A turn already under way would go on
  // working, and every step of its own that needs a launch would be refused while it did: alive,
  // and unable to move. The turn ends itself on meeting it instead, and its row says so.
  test("a turn cancelled while it works ends itself, closes its pane and says it was stopped", async () => {
    const archive = await scratchArchive();
    installProviderSettings(archive);
    const herdr = herdrLike("stopped", {
      statuses: ["idle", "working", "working", "working", "working"],
      transcript: [assistant({ text: "half of an answer", out: 3 })],
      cwd,
      sessionId,
      async onRequest(method) {
        if (method === "agent.prompt") await archive.settings().setSetting("execution_cancel:bmo-task-609", "1");
      },
    });

    try {
      const turn = await runTurnInPane({
        name: "bmo-task-609",
        title: "#609 a job",
        cwd,
        command: ["claude", "--session-id", sessionId],
        env: {},
        prompt: "the work",
        sessionId,
        from: 0,
        pollMs: 5,
        socket: herdr.path,
      });

      expect(turn.outcome).toBe("stopped");
      // The pane goes with the turn, so the CLI it held is not left running with nobody collecting
      // it. The row is closed as cancelled by whichever side first hears that the pane is gone,
      // which this herdr goes on saying it is not.
      expect(herdr.asked.some((call) => call.method === "pane.close")).toBe(true);
      expect((await archive.executions().executions())[0]?.state).not.toBe("exited");
    } finally { installProviderSettings(null); }
  });

  test("a prompt that was taken and closed with nothing to say comes back silent", async () => {
    // Taken is the record gaining the prompt's own line; nothing after it ever starts, and the program
    // closes the turn.
    const file = transcriptPath(cwd, sessionId);
    const herdr = herdrLike("unstarted", {
      statuses: ["idle"],
      transcript: [],
      cwd,
      sessionId,
      onRequest: async (method) => {
        if (method !== "agent.prompt") return;
        mkdirSync(dirname(file), { recursive: true });
        writeFileSync(file, `${[{ type: "user", message: { role: "user", content: "the work" } }, { type: "system", subtype: "turn_duration" }].map((one) => JSON.stringify(one)).join("\n")}\n`);
      },
    });
    const turn = await runTurnInPane({
      name: "bmo-run-9-build-0-1",
      title: "#9 a build",
      cwd,
      command: ["claude", "--session-id", sessionId],
      env: {},
      prompt: "the work",
      sessionId,
      from: 0,
      pollMs: 5,
      socket: herdr.path,
    });

    expect(turn.outcome).toBe("silent");
    rmSync(file, { force: true });
  });

  test("a pane that never comes ready is waiting on a person, and says what is on its screen", async () => {
    const herdr = herdrLike("blocked", { statuses: ["blocked"], transcript: [], cwd, sessionId });
    const turn = await runTurnInPane({
      name: "bmo-run-9-build-0-1",
      title: "#9 a build",
      cwd,
      command: ["claude"],
      env: {},
      prompt: "the work",
      sessionId,
      from: 0,
      pollMs: 5,
      socket: herdr.path,
    });

    expect(turn.outcome).toBe("blocked");
    expect(turn.outcome === "blocked" ? turn.screen : "").toBe("a screen");
    // Nothing was said to it: a blocked agent refuses the words anyway.
    expect(herdr.asked.some((one) => one.method === "agent.prompt")).toBe(false);
  });

  // A first run in a checkout nobody has accepted sits on the trust question before anything.
  test("a run that trusts its folder answers the trust question, and only then is told its work", async () => {
    const herdr = herdrLike("trust", {
      statuses: ["unknown", "blocked"],
      screen: trustScreen,
      afterKeys: ["blocked", "idle", "working", "idle"],
      transcript: [assistant({ text: "the answer", out: 3 })],
      cwd,
      sessionId,
    });
    const turn = await runTurnInPane({
      name: "bmo-run-9-build-0-1",
      title: "#9 a build",
      cwd,
      command: ["claude", "--session-id", sessionId],
      env: {},
      prompt: "the work",
      sessionId,
      from: 0,
      pollMs: 5,
      trustsFolder: true,
      socket: herdr.path,
    });
    const methods = herdr.asked.map((one) => one.method);
    const keys = herdr.asked.filter((one) => one.method === "pane.send_keys");

    expect(turn.outcome).toBe("answered");
    // Answered once, with the keys counted off the screen, and before the prompt went in.
    expect(keys.map((one) => one.params.keys)).toEqual([["down", "enter"]]);
    expect(methods.indexOf("pane.send_keys")).toBeLessThan(methods.indexOf("agent.prompt"));
  });

  test("a run that was not told it trusts its folder leaves the question to a person", async () => {
    const herdr = herdrLike("untrusted", { statuses: ["blocked"], screen: trustScreen, transcript: [], cwd, sessionId });
    const turn = await runTurnInPane({
      name: "bmo-run-9-build-0-1",
      title: "#9 a build",
      cwd,
      command: ["claude"],
      env: {},
      prompt: "the work",
      sessionId,
      from: 0,
      pollMs: 5,
      socket: herdr.path,
    });

    expect(turn.outcome).toBe("blocked");
    expect(herdr.asked.some((one) => one.method === "pane.send_keys")).toBe(false);
  });

  test("a run at the plugin offer declines it for good, whether or not it trusts its folder, and is then told its work", async () => {
    const herdr = herdrLike("offer", {
      statuses: ["blocked"],
      screen: offerScreen,
      afterKeys: ["idle", "working", "idle"],
      transcript: [assistant({ text: "the answer", out: 3 })],
      cwd,
      sessionId,
    });
    const turn = await runTurnInPane({
      name: "bmo-run-9-build-0-1",
      title: "#9 a build",
      cwd,
      command: ["claude", "--session-id", sessionId],
      env: {},
      prompt: "the work",
      sessionId,
      from: 0,
      pollMs: 5,
      socket: herdr.path,
    });
    const methods = herdr.asked.map((one) => one.method);

    expect(turn.outcome).toBe("answered");
    expect(herdr.asked.filter((one) => one.method === "pane.send_keys").map((one) => one.params.keys)).toEqual([["down", "down", "down", "enter"]]);
    expect(methods.indexOf("pane.send_keys")).toBeLessThan(methods.indexOf("agent.prompt"));
  });

  // The confirmation the #364 job stood on, as a real pane drew it (docs/agent-control.md).
  const removalScreen = [
    "─".repeat(60),
    " Bash command",
    '   S=work; echo "$(cat $S/a.md)"; rm -f $S/*.md',
    " Dangerous rm operation on possibly-empty variable path: $S/*.md",
    " Do you want to proceed?",
    " ❯ 1. Yes",
    "   2. No",
    " Esc to cancel · Tab to amend",
  ].join("\n");

  const midTurn = (
    name: string,
    screen: string | ((read: number) => string),
    confirm: Parameters<typeof runTurnInPane>[0]["confirm"],
    afterKeys: string[] | ((sent: number) => string[]) = ["working", "idle"],
  ) => {
    const herdr = herdrLike(name, {
      statuses: ["idle", "working", "blocked"],
      screen,
      afterKeys,
      transcript: [assistant({ text: "the answer", out: 3 })],
      cwd,
      sessionId,
    });
    const turn = runTurnInPane({
      name: "bmo-run-9-build-0-1",
      title: "#9 a build",
      cwd,
      command: ["claude", "--session-id", sessionId],
      env: {},
      prompt: "the work",
      sessionId,
      from: 0,
      pollMs: 5,
      redrawMs: 30,
      confirm,
      socket: herdr.path,
    });

    return { herdr, turn };
  };
  const keysSent = (herdr: Driven) => herdr.asked.filter((one) => one.method === "pane.send_keys").map((one) => one.params.keys);
  // The same question as the CLI draws it under its countdown, a second gone every read.
  const ticking = (read: number) =>
    removalScreen.replace(
      " Do you want to proceed?",
      ` ⚠ Claude Code will automatically deny this request in 0:${String(Math.max(0, 59 - read)).padStart(2, "0")}, to avoid blocking progress on an unattended session\n Do you want to proceed?`,
    );
  const allowLater = (asked: string[]) => async (question: PaneConfirmation) => {
    asked.push(question.kind === "permission" ? question.text : question.screen);
    await Bun.sleep(40);

    return { allow: true } as const;
  };

  test("a plugin offer in the middle of a turn is declined from the screen and never put to the caller", async () => {
    const asked: string[] = [];
    const { herdr, turn } = midTurn("offer-mid", offerScreen, allowLater(asked), ["idle"]);

    expect((await turn).outcome).toBe("answered");
    expect(keysSent(herdr)).toEqual([["down", "down", "down", "enter"]]);
    expect(asked).toEqual([]);
  });

  test("a confirmation in the middle of a turn is asked once, and a refusal is typed into the pane with its reason", async () => {
    const asked: string[] = [];
    const { herdr, turn } = midTurn("refuse", removalScreen, async (question) => {
      asked.push(question.kind === "permission" ? question.text : question.screen);

      return { allow: false, reason: "Refused.\nGuard the variable." };
    });

    expect((await turn).outcome).toBe("answered");
    expect(asked).toHaveLength(1);
    expect(asked[0]).toContain("$S/*.md");

    const typed = herdr.asked.filter((one) => one.method === "pane.send_keys" || one.method === "pane.send_text");

    // The prompt's typed lead comes first; the refusal is typed in the command's place after it.
    expect(typed.map((one) => one.params.keys ?? one.params.text)).toEqual([`${pastedLead} `, ["down", "tab"], "Refused. Guard the variable.", ["enter"]]);
    // The refusal went in the command's place; nothing was prompted after it.
    expect(herdr.asked.filter((one) => one.method === "agent.prompt")).toHaveLength(1);
  });

  // A turn working past a short clock, with its last line a timestamped tool call.
  const overrunning = (name: string, overtime: Parameters<typeof runTurnInPane>[0]["overtime"]) => {
    const herdr = herdrLike(name, {
      statuses: ["idle", ...Array.from({ length: 40 }, () => "working"), "idle"],
      transcript: [
        { ...assistant({ tool: { id: "t1", name: "Bash", input: { command: "bun test" } } }), timestamp: "2026-10-01T04:00:00.000Z" },
        assistant({ text: "the answer", out: 3 }),
      ],
      cwd,
      sessionId,
    });
    const turn = runTurnInPane({
      name: "bmo-word-worker",
      title: "a word",
      cwd,
      command: ["claude", "--session-id", sessionId],
      env: {},
      prompt: "the word",
      sessionId,
      from: 0,
      pollMs: 5,
      turnMs: 30,
      overtime,
      socket: herdr.path,
    });

    return { herdr, turn };
  };

  test("a turn past its clock asks its caller's policy, and runs on to its answer while the policy keeps it", async () => {
    const asked: TurnOverdue[] = [];
    const { turn } = overrunning("held", async (overdue) => {
      asked.push(overdue);

      return { cut: false, askAgainAt: Date.now() + 5 };
    });

    expect((await turn).outcome).toBe("answered");
    expect(asked.length).toBeGreaterThan(0);
    expect(asked[0]).toMatchObject({ boundMs: 30, lastActivityAt: null, lastActivity: "wrote words" });
  });

  test("a turn past its clock is cut when its caller's policy says cut", async () => {
    const asked: TurnOverdue[] = [];
    const { turn } = overrunning("cut", async (overdue) => {
      asked.push(overdue);

      return { cut: true };
    });

    expect((await turn).outcome).toBe("overran");
    expect(asked).toHaveLength(1);
  });

  test("a turn past its clock whose policy cannot answer runs on to its answer, asked again after the clock", async () => {
    let asks = 0;
    const { turn } = overrunning("unanswerable", async () => {
      asks += 1;
      throw new Error("the archive is gone");
    });

    expect((await turn).outcome).toBe("answered");
    expect(asks).toBeGreaterThan(0);
  });

  test("the record's last line says when it was written and what it called", () => {
    const records = [
      { ...assistant({ tool: { id: "t1", name: "Bash", input: { command: "bun test\n  --parallel" } } }), timestamp: "2026-10-01T04:00:00.000Z" },
      { ...answered("t1"), timestamp: "2026-10-01T04:01:00.000Z" },
    ];

    expect(lastActivityOf(records.slice(0, 1), 0)).toEqual({ at: Date.parse("2026-10-01T04:00:00.000Z"), what: "called Bash (bun test --parallel)" });
    expect(lastActivityOf(records, 0)).toEqual({ at: Date.parse("2026-10-01T04:01:00.000Z"), what: "a tool's answer came back" });
    expect(lastActivityOf(records, 2)).toEqual({ at: null, what: null });
  });

  test("an allowed confirmation is answered on its Yes row", async () => {
    const { herdr, turn } = midTurn("allow", removalScreen, async () => ({ allow: true }));

    expect((await turn).outcome).toBe("answered");
    expect(herdr.asked.filter((one) => one.method === "pane.send_keys").map((one) => one.params.keys)).toEqual([["enter"]]);
  });

  // The #858 and #860 jobs: the answer came back to a question whose countdown had moved on, was
  // never typed, and the same question went to the owner again every minute.
  test("a question ticking down while it is asked is asked once and answered", async () => {
    const asked: string[] = [];
    const { herdr, turn } = midTurn("ticking", ticking, allowLater(asked));

    expect((await turn).outcome).toBe("answered");
    expect(asked).toHaveLength(1);
    expect(keysSent(herdr)).toEqual([["enter"]]);
  });

  test("a question still standing after its answer is answered again, not asked again", async () => {
    const asked: string[] = [];
    const { herdr, turn } = midTurn("stands", ticking, allowLater(asked), (sent) => (sent === 1 ? ["blocked"] : ["working", "idle"]));

    expect((await turn).outcome).toBe("answered");
    expect(asked).toHaveLength(1);
    expect(keysSent(herdr)).toEqual([["enter"], ["enter"]]);
  });

  test("a question that outlasts every answer is left for a person after three", async () => {
    const asked: string[] = [];
    // The question stands on past its third answer, until a person deals with it and the run goes on.
    const { herdr, turn } = midTurn("outlasts", ticking, allowLater(asked), (sent) => (sent < 3 ? ["blocked"] : [...Array<string>(60).fill("blocked"), "idle"]));

    await turn;
    expect(asked).toHaveLength(1);
    expect(keysSent(herdr)).toEqual([["enter"], ["enter"], ["enter"]]);
  });

  test("the same question raised again in the turn is answered as it was the first time", async () => {
    const asked: string[] = [];
    const { herdr, turn } = midTurn("again", ticking, allowLater(asked), (sent) => (sent === 1 ? ["working", "blocked"] : ["working", "idle"]));

    expect((await turn).outcome).toBe("answered");
    expect(asked).toHaveLength(1);
    expect(keysSent(herdr)).toEqual([["enter"], ["enter"]]);
  });

  test("a closed unknown question is escaped and its reason prompted once the run is at rest", async () => {
    const { herdr, turn } = midTurn("close", "Pick a colour\n❯ 1. Red\n  2. Blue", async () => ({ allow: false, reason: "carry on" }));

    expect((await turn).outcome).toBe("answered");
    expect(herdr.asked.filter((one) => one.method === "pane.send_keys").map((one) => one.params.keys)).toEqual([["esc"]]);
    expect(herdr.asked.filter((one) => one.method === "agent.prompt").map((one) => one.params.text)).toEqual(["the work", "carry on"]);
  });

  test("a question the pane stops showing before the answer is dropped, and its asker told", async () => {
    let stopped = false;
    const herdr = herdrLike("elsewhere", {
      statuses: ["idle", "working", "blocked", "blocked", "working", "idle"],
      screen: removalScreen,
      transcript: [assistant({ text: "the answer", out: 3 })],
      cwd,
      sessionId,
    });
    const turn = await runTurnInPane({
      name: "bmo-run-9-build-0-1",
      title: "#9 a build",
      cwd,
      command: ["claude", "--session-id", sessionId],
      env: {},
      prompt: "the work",
      sessionId,
      from: 0,
      pollMs: 5,
      confirm: (_, answeredElsewhere) =>
        new Promise((resolve) => {
          answeredElsewhere.addEventListener("abort", () => {
            stopped = true;
            resolve({ leave: true });
          });
        }),
      socket: herdr.path,
    });

    expect(turn.outcome).toBe("answered");
    expect(stopped).toBe(true);
    expect(herdr.asked.some((one) => one.method === "pane.send_keys")).toBe(false);
  });

  test("any other question is a person's, even for a run that trusts its folder", async () => {
    const herdr = herdrLike("other", { statuses: ["blocked"], screen: "Allow this MCP server?\n❯ Yes\n  No", transcript: [], cwd, sessionId });
    const turn = await runTurnInPane({
      name: "bmo-run-9-build-0-1",
      title: "#9 a build",
      cwd,
      command: ["claude"],
      env: {},
      prompt: "the work",
      sessionId,
      from: 0,
      pollMs: 5,
      trustsFolder: true,
      socket: herdr.path,
    });

    expect(turn.outcome).toBe("blocked");
    expect(herdr.asked.some((one) => one.method === "pane.send_keys")).toBe(false);
  });
});

describe("the program a resident turn stands on", () => {
  const home = { agent: "opens", label: "opens", head: { command: [], cwd: tmpdir(), env: {} } };
  const turnOn = (flag: string) => ({ cwd: "/work/tree", command: ["claude", "--model", "m", flag, "s-1"], env: { PATH: "/usr/bin", CLAUDE_CONFIG_DIR: "/logins/a" }, sessionId: "s-1" });

  test("names the record of the session its command opens, and none for one it resumes", () => {
    expect(residentOf(home, turnOn("--session-id")).program.opened).toBe(transcriptPath("/work/tree", "s-1", "/logins/a/projects"));
    expect(residentOf(home, turnOn("--resume")).program.opened).toBeNull();
  });

  test("is the same program whichever room or run the turn is about", () => {
    const about = (room: string, run: string) => {
      const turn = turnOn("--resume");

      return residentOf(home, { ...turn, env: { ...turn.env, BMO_RUN_NAME: run, BMO_TURN_CHAT: room, BMO_TURN_VIA: "console" } }).program;
    };
    const one = about("-1", "chat:-1:1:1");
    const other = about("42", "chat:42:2:2");

    expect(one.identity).toBe(other.identity);
    expect(Object.keys(one.command.env).some((name) => name.startsWith("BMO_TURN_") || name === "BMO_RUN_NAME")).toBe(false);
  });
});

describe("a person's turn in the resident program", () => {
  const scratch = scratchDir("bmo-person-turn-");
  let sockets = 0;

  afterAll(() => rmSync(scratch, { recursive: true, force: true }));

  // A herdr answering one pane's agent status, whatever the status is now.
  function herdrWithStatus(status: () => string) {
    const path = join(scratch, `herdr-${++sockets}.sock`);
    const server = Bun.listen<undefined>({
      unix: path,
      socket: {
        data(connection, chunk) {
          for (const line of chunk.toString().split("\n").filter(Boolean)) {
            const request = JSON.parse(line);
            const result = request.method === "agent.list" ? { agents: [{ pane_id: "p-1", agent: "claude", agent_status: status() }] } : { panes: [{ pane_id: "p-1" }] };

            connection.write(`${JSON.stringify({ id: request.id, result })}\n`);
            connection.end();
          }
        },
      },
    });

    return { path, stop: () => server.stop(true) };
  }

  function standing(name: string): string {
    const place = join(scratch, name);

    recordStarted(place, { identity: "p", pid: 1, birth: null, loop: { pid: 1, birth: "x" }, starts: 1, startedAt: 0, paneId: "p-1" });

    return place;
  }

  const alive = () => true;

  test("is waited out while it works, and the wait ends with it", async () => {
    let working = true;
    const herdr = herdrWithStatus(() => (working ? "working" : "done"));
    const place = standing("working");
    let told = 0;

    recordCall(place, "submit", { prompt: "hello" });
    setTimeout(() => {
      recordCall(place, "stop", {});
      working = false;
    }, 150);

    try {
      const started = Date.now();

      expect(await personTurnDone(place, { socket: herdr.path, everyMs: 10, alive, onWait: async () => told++ })).toBe(true);
      expect(Date.now() - started).toBeGreaterThanOrEqual(140);
      expect(told).toBe(1);
    } finally {
      herdr.stop();
    }
  });

  test("is not waited for once it is at rest, whatever the place says", async () => {
    const herdr = herdrWithStatus(() => "idle");
    const place = standing("interrupted");

    recordCall(place, "submit", { prompt: "hello" });

    try {
      expect(await personTurnDone(place, { socket: herdr.path, everyMs: 10, alive })).toBe(false);
    } finally {
      herdr.stop();
    }
  });

  test("is waited out however long it works, ends only when it does, and a turn of the bot's is never waited for", async () => {
    let looks = 0;
    // Working for many looks with no end recorded — cut short by an Escape — and then at rest.
    const herdr = herdrWithStatus(() => (++looks < 40 ? "working" : "idle"));
    const place = standing("endless");

    recordCall(place, "submit", { prompt: "hello" });

    try {
      expect(await personTurnDone(place, { socket: herdr.path, everyMs: 10, alive })).toBe(true);
      expect(looks).toBeGreaterThanOrEqual(40);

      const bots = standing("bots");

      await Bun.write(join(bots, "run.json"), JSON.stringify({ name: "bmo-job-1", title: "#1", person: false, startedAt: 0, endedAt: null }));
      expect(await personTurnDone(bots, { socket: herdr.path, everyMs: 10, alive })).toBe(false);
    } finally {
      herdr.stop();
    }
  });
});
