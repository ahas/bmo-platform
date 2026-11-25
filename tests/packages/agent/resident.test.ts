import "@agency/test-support/setup";

import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, describe, expect, test } from "bun:test";

import { codeRoot } from "@agency/support/roots";
import { claudeCodeProvider, readClaudeResidentTurn, residentTurnsOwed } from "@agency/agent/claude-code";
import { codexProvider, readCodexRollout } from "@agency/agent/codex";
import { agentProviderNames } from "@agency/agent/harness";
import { providerImplementations } from "@agency/agent/harnesses";
import { commandDigest } from "@agency/agent/resident";
import { scratchDir } from "@agency/test-support/scratch";

import type { HarnessRun, AgentProvider, ResidentSurface } from "@agency/agent/harness";

// The program a provider keeps standing in an agent's pane (docs/agent-control.md, 「The interactive
// surface a provider declares」): what each provider declares, how one of its turns is read back
// out of what the program wrote, and the one question everything else asks — resident or host —
// answered off the declaration and never off a name.

const root = scratchDir("bmo-resident-");
const promptPath = join(root, "system.md");
const settingsPath = join(root, "settings.json");

writeFileSync(promptPath, "You are the supervisor.\n");
writeFileSync(settingsPath, "{}\n");

afterAll(() => rmSync(root, { recursive: true, force: true }));

function run(overrides: Partial<HarnessRun> = {}): HarnessRun {
  return {
    line: "chat",
    model: "claude-sonnet-5",
    effort: "low",
    systemPromptPath: promptPath,
    toolsShown: "Bash",
    toolsAllowed: "Bash",
    toolsDenied: null,
    mcpConfigPath: null,
    prompt: "",
    promptDelivery: "channel",
    resumeSessionId: null,
    keepSession: true,
    openSessionId: "11111111-1111-1111-1111-111111111111",
    hostCommands: false,
    thinking: true,
    answerShape: "stream",
    cwd: root,
    env: { PATH: "/usr/bin" },
    resident: { place: join(root, "place"), settingsPath },
    ...overrides,
  };
}

const surfaceOf = (provider: AgentProvider): ResidentSurface => {
  if (provider.resident === null) throw new Error(`${provider.name} declares no resident program`);
  return provider.resident;
};

describe("resident or host", () => {
  const surface = surfaceOf(claudeCodeProvider);

  test("every provider stands a resident program, and none is left to the host alone", () => {
    expect(providerImplementations().filter((provider) => provider.resident === null).map((provider) => provider.name)).toEqual([]);
  });

  test("the resolution names no provider", () => {
    const source = readFileSync(join(codeRoot, "packages", "agent", "resident.ts"), "utf8");

    for (const name of agentProviderNames) {
      expect(source.includes(`"${name}"`)).toBe(false);
    }
  });
});

describe("command identity", () => {
  test("the same command is the same program and any change is another", () => {
    const one = { args: ["prog", "--model", "a"], cwd: "/x", env: { A: "1", B: undefined } };

    expect(commandDigest(one)).toBe(commandDigest({ ...one, env: { A: "1" } }));
    expect(commandDigest(one)).not.toBe(commandDigest({ ...one, args: ["prog", "--model", "b"] }));
    expect(commandDigest(one)).not.toBe(commandDigest({ ...one, cwd: "/y" }));
    expect(commandDigest(one)).not.toBe(commandDigest({ ...one, env: { A: "2" } }));
  });

  test("a program whose settings came to call other hooks is another program", () => {
    const settings = join(scratchDir("bmo-resident-settings-"), "settings.json");
    const one = { args: ["claude", "--settings", settings], cwd: "/x", env: {} };

    writeFileSync(settings, JSON.stringify({ statusLine: { command: "old" } }));
    const before = commandDigest(one);

    expect(commandDigest(one)).toBe(before);
    writeFileSync(settings, JSON.stringify({ statusLine: { command: "new" } }));
    expect(commandDigest(one)).not.toBe(before);
  });
});

describe("Claude Code's resident program", () => {
  const surface = surfaceOf(claudeCodeProvider);

  test("declares its door, its words mid-turn, its interrupt, fold and clear", () => {
    expect(surface.door).toBe("agent-prompt");
    expect(surface.takesWordsMidTurn).toBe(true);
    expect(surface.interrupt).toEqual({ keys: ["escape"] });
    expect(surface.fold).toBe("/compact");
    expect(surface.clear).toBe("/clear");
  });

  test("starts the chat line without print mode, with its slash commands, settings after the allowed tools", async () => {
    const command = await surface.command(run());
    const args = command.args;

    expect(args[0]).toBe("claude");
    expect(args).not.toContain("-p");
    expect(args).not.toContain("--output-format");
    expect(args).not.toContain("--disable-slash-commands");
    expect(args.indexOf("--settings")).toBeGreaterThan(args.indexOf("--allowedTools"));
    expect(args[args.indexOf("--settings") + 1]).toBe(settingsPath);
    expect(args.slice(-2)).toEqual(["--session-id", "11111111-1111-1111-1111-111111111111"]);
    expect(command.stdin).toBeNull();
  });

  test("resumes the agent's session where it has one", async () => {
    const args = (await surface.command(run({ resumeSessionId: "22222222-2222-2222-2222-222222222222" }))).args;

    expect(args.slice(-2)).toEqual(["--resume", "22222222-2222-2222-2222-222222222222"]);
  });

  test("refuses a line that keeps no session", () => {
    expect(() => surface.command(run({ line: "gate" }))).toThrow("keeps no session");
  });

  test("a changed model is a different program", async () => {
    expect(surface.identity(await surface.command(run()))).toBe(surface.identity(await surface.command(run())));
    expect(surface.identity(await surface.command(run()))).not.toBe(surface.identity(await surface.command(run({ model: "claude-opus-5" }))));
  });

  // A login change is a command change: the program is started again on the new login between
  // turns, never told to switch while it stands (「Stopping, falling back and standing up again」).
  test("another login is a different program", async () => {
    const login = (id: string) => ({ id, name: id, credential: { kind: "directory", path: `/nonexistent-logins/${id}` } });
    const first = surface.identity(await surface.command(run({ account: login("first") })));

    expect(surface.identity(await surface.command(run({ account: login("first") })))).toBe(first);
    expect(surface.identity(await surface.command(run({ account: login("second") })))).not.toBe(first);
  });
});

describe("a Claude Code resident turn", () => {
  const response = (id: string, text: string, usage: Record<string, number> = { input_tokens: 10, output_tokens: 5 }) => ({
    type: "assistant",
    sessionId: "s1",
    message: { id, model: "claude-sonnet-5", content: [{ type: "text", text }], usage },
  });
  const tool = (id: string) => ({
    type: "assistant",
    sessionId: "s1",
    message: { id, model: "claude-sonnet-5", content: [{ type: "tool_use", id: `t${id}`, name: "Bash", input: {} }], usage: { input_tokens: 3, output_tokens: 2 } },
  });
  const end = { type: "system", subtype: "turn_duration" };
  const status = (session: string, total: number, limits?: object) => ({
    session_id: session,
    model: { id: "claude-sonnet-5" },
    cost: { total_cost_usd: total },
    context_window: { used_percentage: 12 },
    ...(limits === undefined ? {} : { rate_limits: limits }),
  });

  test("is going until its end line and ended once it stands", () => {
    const going = readClaudeResidentTurn({ lines: [{ type: "user" }, tool("a")] }, run());
    const done = readClaudeResidentTurn({ lines: [{ type: "user" }, tool("a"), response("b", "answer"), end] }, run());

    expect(going.state).toBe("working");
    expect(done.state).toBe("ended");
    expect(done.reply.text).toBe("answer");
    expect(done.reply.spend[0]?.inputTokens).toBe(13);
  });

  test("a word taken inside the turn leaves nothing queued", () => {
    const reading = readClaudeResidentTurn(
      {
        lines: [
          tool("a"),
          { type: "queue-operation", operation: "enqueue" },
          { type: "queue-operation", operation: "remove" },
          { type: "attachment" },
          response("b", "both"),
          end,
        ],
      },
      run(),
    );

    expect(reading.queued).toBe(0);
    expect(reading.state).toBe("ended");
  });

  test("a word pushed after the last tool call opens a turn of its own, and the round answers with the last", () => {
    const lines = [
      tool("a"),
      { type: "queue-operation", operation: "enqueue" },
      response("b", "first answer"),
      end,
    ];
    const between = readClaudeResidentTurn({ lines }, run());
    const after = readClaudeResidentTurn(
      { lines: [...lines, { type: "queue-operation", operation: "dequeue" }, { type: "user", message: { content: "WORD-X" } }, response("c", "PINEAPPLE"), end] },
      run(),
    );

    expect(between.queued).toBe(1);
    expect(between.state).toBe("working");
    expect(after.queued).toBe(0);
    expect(after.state).toBe("ended");
    expect(after.reply.text).toBe("PINEAPPLE");
  });

  test("words that waited together are taken off together and open one turn between them", () => {
    // The round of session 05a34aec as its record stands: a word taken inside the turn, two pushed
    // after its last tool call, both taken off in one stretch before its end line, and one turn
    // answering both.
    const lines = [
      { type: "user", message: { content: "ROUND" } },
      tool("a"),
      { type: "queue-operation", operation: "enqueue" },
      { type: "queue-operation", operation: "remove" },
      tool("b"),
      { type: "queue-operation", operation: "enqueue" },
      { type: "queue-operation", operation: "enqueue" },
      response("c", "first answer"),
      { type: "queue-operation", operation: "dequeue" },
      { type: "queue-operation", operation: "dequeue" },
      { type: "system", subtype: "stop_hook_summary" },
      end,
    ];
    const between = readClaudeResidentTurn({ lines }, run());
    const after = readClaudeResidentTurn(
      {
        lines: [
          ...lines,
          { type: "user", message: { content: "WORD-X" } },
          { type: "attachment" },
          { type: "user", message: { content: "WORD-Y" } },
          response("d", "both words"),
          { type: "system", subtype: "stop_hook_summary" },
          end,
        ],
      },
      run(),
    );

    expect(between.queued).toBe(0);
    expect(between.state).toBe("working");
    expect(after.state).toBe("ended");
    expect(after.reply.text).toBe("both words");
  });

  test("an answer written again whole after pushed words goes out once", () => {
    // The round of message -998954711 as its record stands: the owner's word pushed after the last
    // tool call, the draft answer ending the first turn, the word taken off into a turn of its own
    // that ran two tools and wrote the whole reply again with the word taken in.
    const draft = "Not writing it yet.\n\n| gate | accuracy |\n|---|---|\n| people-search | 0.878 |";
    const rewritten = `${draft}\n\nThe audio for B-1000 sentence 2 is up again.`;
    const lines = [
      { type: "user", message: { content: "ROUND" } },
      tool("a"),
      { type: "queue-operation", operation: "enqueue" },
      response("b", draft),
      { type: "queue-operation", operation: "dequeue" },
      { type: "system", subtype: "stop_hook_summary" },
      end,
      { type: "user", message: { content: "More for the answer you are making now: and put the audio up so I can check it" } },
      tool("c"),
      tool("d"),
      response("e", rewritten),
      { type: "system", subtype: "stop_hook_summary" },
      end,
    ];
    const reading = readClaudeResidentTurn({ lines }, run());

    expect(reading.state).toBe("ended");
    expect(reading.reply.text).toBe(rewritten);
    expect(reading.reply.text.split("Not writing it yet.").length).toBe(2);
  });

  test("words taken off in stretches of their own owe a turn each", () => {
    const lines = [
      tool("a"),
      { type: "queue-operation", operation: "enqueue" },
      { type: "queue-operation", operation: "enqueue" },
      response("b", "one"),
      { type: "queue-operation", operation: "dequeue" },
      end,
      { type: "user", message: { content: "WORD-X" } },
      response("c", "two"),
      { type: "queue-operation", operation: "dequeue" },
      end,
      { type: "user", message: { content: "WORD-Y" } },
    ];

    expect(residentTurnsOwed(lines)).toEqual({ queued: 0, owed: 3 });
    expect(readClaudeResidentTurn({ lines }, run()).state).toBe("working");
    expect(readClaudeResidentTurn({ lines: [...lines, response("d", "three"), end] }, run()).state).toBe("ended");
  });

  // A word pushed after the turn had ended never waits on the queue: the program takes it straight as
  // a prompt of its own, and the round owes the turn it opens.
  test("a word taken straight as a prompt after the turn's end owes a turn of its own", () => {
    const lines = [
      { type: "user", message: { content: "ROUND" } },
      response("a", "first"),
      { type: "system", subtype: "stop_hook_summary" },
      end,
      { type: "user", message: { content: "More for the answer you are making now: 2 new message(s)" } },
    ];

    expect(residentTurnsOwed(lines)).toEqual({ queued: 0, owed: 2 });
    expect(readClaudeResidentTurn({ lines }, run()).state).toBe("working");

    const ended = readClaudeResidentTurn({ lines: [...lines, response("b", "rewritten"), end] }, run());

    expect(ended.state).toBe("ended");
    expect(ended.reply.text).toBe("rewritten");
  });

  test("words pulled back into the input leave nothing queued", () => {
    const lines = [tool("a"), { type: "queue-operation", operation: "enqueue" }, { type: "queue-operation", operation: "enqueue" }, { type: "queue-operation", operation: "popAll" }, response("b", "x"), end];

    expect(residentTurnsOwed(lines)).toEqual({ queued: 0, owed: 1 });
    expect(readClaudeResidentTurn({ lines }, run()).state).toBe("ended");
  });

  test("an Escape is an interrupted turn, not an answer", () => {
    const reading = readClaudeResidentTurn(
      { lines: [tool("a"), { type: "user", message: { content: [{ type: "text", text: "[Request interrupted by user for tool use]" }] } }] },
      run(),
    );

    expect(reading.state).toBe("interrupted");
  });

  test("the money is the running total's difference within one session and nothing across two", () => {
    const lines = [response("b", "x"), end];
    const same = readClaudeResidentTurn({ lines, statusBefore: status("s1", 0.25), statusAfter: status("s1", 0.3) }, run());
    const cleared = readClaudeResidentTurn({ lines, statusBefore: status("s1", 0.25), statusAfter: status("s2", 0.01) }, run());
    const unread = readClaudeResidentTurn({ lines, statusAfter: status("s1", 0.3) }, run());

    expect(same.money).toBeCloseTo(0.05, 10);
    expect(same.reply.spend[0]?.cost).toBeCloseTo(0.05, 10);
    expect(cleared.money).toBeNull();
    expect(unread.money).toBeNull();
  });

  test("windows come from the reading after the turn, as fractions, and are absent before a request", () => {
    const limits = { five_hour: { used_percentage: 22, resets_at: 1789962600 }, seven_day: { used_percentage: 10, resets_at: 1790528400 } };
    const read = readClaudeResidentTurn({ lines: [response("b", "x"), end], statusAfter: status("s1", 0.1, limits) }, run());
    const none = readClaudeResidentTurn({ lines: [response("b", "x"), end], statusAfter: status("s1", 0.1) }, run());

    expect(read.reply.sessionUsage).toEqual({ utilization: 0.22, resetsAt: 1789962600, weeklyUtilization: 0.1, weeklyResetsAt: 1790528400 });
    expect(read.reply.windows.map((window) => window.name)).toEqual(["five_hour", "seven_day"]);
    expect(read.reply.contextRatio).toBeCloseTo(0.12, 10);
    expect(read.reply.sessionId).toBe("s1");
    expect(none.reply.sessionUsage).toBeNull();
    expect(none.reply.windows).toEqual([]);
  });

  test("the program's own words in the answer's place are an error, not an answer", () => {
    const failed = { type: "assistant", isApiErrorMessage: true, message: { id: "e", model: "<synthetic>", content: [{ type: "text", text: "API Error: 529 Overloaded" }] } };
    const reading = readClaudeResidentTurn({ lines: [failed, end] }, run());

    expect(reading.error).toBe("API Error: 529 Overloaded");
    expect(reading.reply.text).toBe("");
  });
});


