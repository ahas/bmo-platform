import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { mkdirSync, symlinkSync } from "node:fs";
import { join } from "node:path";

import { readClaudeOutput } from "@agency/agent/claude";
import { ClaudeLimitError } from "@agency/agent/claude-failure";
import { Archive } from "@agency/archive/archive";
import { SEED_ACCOUNT_ID } from "@agency/agent/claude-account";
import { standClaudeCatalogue } from "./claude-catalogue-fixture";
import { claudeCode } from "@agency/agent/claude-code";
import { installProviderSettings } from "@agency/agent/provider-order";
import { codex } from "@agency/agent/codex";
import { dsh } from "@agency/agent/dsh";
import { carriesRunLine, runLines } from "@agency/agent/run-lines";
import {
  harnessFor,
  harnessImplementations,
  provider,
  providerImplementations,
} from "@agency/agent/harnesses";
import { deployRoot, sourceRoot } from "@agency/support/roots";
import { workspaceBinary } from "@agency/test-support/binaries";

import type { HarnessRun } from "@agency/agent/harness";
import { scratchArchive, scratchDatabase } from "@agency/test-support/store";

// A run is carried on a harness, and the harness is the only thing that knows a flag. What is
// held here is the whole of that arrangement: that the commands are the commands they were down
// to the byte, which harnesses this build carries, that a run asks rather than writes its own
// command, and that the table saying which harness carries what answers Claude Code until the
// owner writes something else in it. What a run becomes on the second harness is its own file.

// A run of the chat line, stated rather than reached for, since what is under test is the command
// that comes back.
function chatRunFor(thinking: boolean, answerShape: HarnessRun["answerShape"] = "unread"): HarnessRun {
  return {
    line: "chat",
    model: "claude-sonnet-5",
    effort: "medium",
    systemPromptPath: "/tmp/chat-prompt.md",
    toolsShown: "",
    toolsAllowed: null,
    toolsDenied: null,
    mcpConfigPath: null,
    prompt: "say this",
    promptDelivery: "channel",
    resumeSessionId: null,
    keepSession: true,
    hostCommands: true,
    thinking,
    answerShape,
    cwd: sourceRoot(),
    env: {},
  };
}

// What the providers answer about the machine is the harness-providers binary's, the one Cargo built before
// the tests, standing where the code looks for it: the test home's deploy tree.
mkdirSync(join(deployRoot(), "bin"), { recursive: true });
symlinkSync(workspaceBinary("harness-providers"), join(deployRoot(), "bin", "harness-providers"));


// Every line as it stands today, flag for flag and value for value. A flag that moves, is added
// or is dropped fails here, which is the point: the prompt cache matches a byte-identical prefix,
// so a line that moved has cost every warm session standing on it its whole context.
describe("the commands are the commands they were", () => {
  // A supervisor with a worker's hands names what it keeps back before its mode, and with no allow
  // list every call goes ahead unasked, as a job's does (docs/supervisors.md, 「What a turn is given」).

  test("the chat line, and the one it runs with the thinking off", async () => {
    const command = await claudeCode.command(chatRunFor(true));

    expect(command.args).toEqual([
      "claude",
      "--model",
      "claude-sonnet-5",
      "--effort",
      "medium",
      "--system-prompt-file",
      "/tmp/chat-prompt.md",
      "--setting-sources",
      "",
      "--strict-mcp-config",
      "--no-chrome",
      "--permission-mode",
      "bypassPermissions",
      "--tools",
      "",
    ]);
    // Thinking is off in the environment and nowhere else, so the two runs stand on one prefix.
    expect(command.env.MAX_THINKING_TOKENS).toBeUndefined();
    expect((await claudeCode.command(chatRunFor(false))).args).toEqual(command.args);
    expect((await claudeCode.command(chatRunFor(false))).env.MAX_THINKING_TOKENS).toBe("0");
  });

  // A supervisor's program names what it is denied and nothing else: every tool of its own but those, each
  // call unasked, with no list of what it is shown or allowed (docs/launch-lines.md, 「What a line's program
  // is allowed, by road」).
  test("a supervisor's chat line bypasses permissions with its denied list alone", async () => {
    const run = { ...chatRunFor(true), toolsShown: "Bash,Read", toolsAllowed: "Bash,Read", toolsDenied: "WebSearch,mcp__job__job_item_create" };
    const args = (await claudeCode.command(run)).args;

    expect(args.slice(args.indexOf("--permission-mode"))).toEqual([
      "--permission-mode",
      "bypassPermissions",
      "--disallowedTools",
      "WebSearch,mcp__job__job_item_create",
    ]);
    expect(args).not.toContain("--tools");
    expect(args).not.toContain("--allowedTools");
  });

  // Claude Code runs only as the interactive program (docs/agents.md, 「No print mode」).
  test("a run asking for its answer on the program's output is refused", async () => {
    for (const answerShape of ["object", "stream"] as const) {
      await expect(claudeCode.command(chatRunFor(true, answerShape))).rejects.toThrow("runs only as the interactive program");
    }
  });
});

describe("the providers there are, and the programs that carry them", () => {
  test("five providers, each on the program that carries it, offered in the order a screen lists them", () => {
    expect(providerImplementations().map((one) => [one.name, one.label, one.adapter.name])).toEqual([
      ["claude-code", "Claude Code", "claude-code"],
      ["registered", "Registered models", "claude-code:registered"],
      ["codex", "Codex", "claude-code:codex"],
      ["openrouter", "OpenRouter", "claude-code:openrouter"],
      ["deepseek", "DeepSeek", "claude-code:deepseek"],
    ]);
    // The registry, Codex, OpenRouter and DeepSeek are each Claude Code on the gateway under a model of
    // their own.
    expect(harnessImplementations()).toEqual([
      claudeCode,
      provider("registered").adapter,
      provider("codex").adapter,
      provider("openrouter").adapter,
      provider("deepseek").adapter,
    ]);
    expect(provider("codex")).toBe(providerImplementations()[2]!);
  });

  test("every line is spawned on the Claude road where no stand-in is installed", () => {
    for (const line of runLines) {
      expect(harnessFor(line).name).toBe("claude-code");
    }
  });

  // What each program says it can carry. Claude Code was written for every line; Codex and dsh
  // carry a judgement, a room's turn and a job, and neither carries a
  // runner, whose tools are agency's own MCP wiring and co-worker gate.
  test("each carries the lines its program offers what they need", () => {
    const carried = (harness: typeof claudeCode) => runLines.filter((line) => carriesRunLine(harness, line)).toSorted();

    expect(carried(claudeCode)).toEqual([...runLines].toSorted());
    expect(carried(provider("codex").adapter)).toEqual([...runLines].toSorted());
    expect(carried(provider("openrouter").adapter)).toEqual([...runLines].toSorted());
    expect(carried(dsh)).toEqual(["chat", "gate", "worker"]);
  });

  // What differs from one provider to the next is the provider's own to declare, so a consumer asks
  // the road rather than branching on its name. A provider that could not answer one of these would
  // not compile; this is the reading of the answers each of them gives.
  test("each provider answers for its own efforts, its ceiling and its logins", async () => {
    expect(providerImplementations().map((one) => [one.name, one.capsAnswerTokens, one.logins !== null])).toEqual([
      ["claude-code", false, true],
      ["registered", true, false],
      ["codex", true, false],
      ["openrouter", true, false],
      ["deepseek", true, false],
    ]);
    expect(provider("registered").efforts).toEqual([null, "thinking", "no-thinking"]);
    expect(provider("claude-code").efforts).toEqual([null, "low", "medium", "high", "xhigh", "max"]);
    expect(provider("deepseek").efforts).toEqual([null, "none", "low", "medium", "high", "max"]);

    // What one Claude model takes comes from the CLI's own catalogue, so a catalogue of the test's
    // own stands while these are read. What a model's entry names is what it takes; a model whose
    // entry names none because it does not think, and a name the catalogue does not carry at all,
    // both take every word the harness takes, since a name missing from a list is not evidence
    // against it.
    const stood = standClaudeCatalogue([
      { id: "claude-opus-5-5", efforts: ["low", "medium", "high", "xhigh", "max"] },
      { id: "claude-opus-4-6", inService: false, efforts: ["low", "medium", "high"] },
      { id: "claude-haiku-4-5-20251001" },
    ]);

    try {
      expect(await provider("claude-code").modelEfforts?.("claude-opus-4-6")).toEqual(["low", "medium", "high"]);
      expect(await provider("claude-code").modelEfforts?.("claude-haiku-4-5-20251001")).toEqual(["low", "medium", "high", "xhigh", "max"]);
      expect(await provider("claude-code").modelEfforts?.("claude-nothing-names-this")).toEqual(["low", "medium", "high", "xhigh", "max"]);
      expect((await provider("claude-code").models()).map((one) => one.id)).toEqual(["claude-opus-5-5", "claude-opus-4-6", "claude-haiku-4-5-20251001"]);
    } finally {
      stood.close();
    }

    for (const name of ["codex", "registered", "openrouter"] as const) {
      expect(provider(name).modelEfforts).toBeUndefined();
    }

    expect(provider("claude-code").effortFault(null)).toBe("This harness requires explicit effort");
    expect(provider("codex").effortFault("low")).toBeNull();
    expect(provider("openrouter").effortFault("low")).toBeNull();
    expect(provider("registered").effortFault(null)).toBeNull();
  });

  test("OpenRouter is shut on a machine that holds no key for it", async () => {
    const openrouter = provider("openrouter");

    expect(await openrouter.unavailable({ config: { openrouterKey: null } })).toContain("openrouter.key");
  });

  // Which login's catalogue answers matters: the CLI refreshes the file as it is used, so a login
  // agency never walks onto holds a list that stopped moving. Two catalogues stand here, one under the
  // login the walk takes first and one under the machine's own directory, and the answer says which
  // was read.
  test("the Claude models offered are the walking login's own catalogue, not the machine's", async () => {
    const walked = standClaudeCatalogue([{ id: "claude-the-walking-login", efforts: ["high"] }]);
    const machine = standClaudeCatalogue([{ id: "claude-the-machine", efforts: ["high"] }]);
    const archive = await scratchArchive();

    // The archive seeds itself with the machine's own login, so the second one is the walk's head.
    await archive.claudeAccounts().addClaudeAccount({ id: "walked", name: "Walked account", enabled: true, credentialKind: "directory", credentialPath: walked.configDir });
    await archive.claudeAccounts().setClaudeAccountOrder([{ id: "walked", place: 1, enabled: true }, { id: SEED_ACCOUNT_ID, place: 2, enabled: true }]);
    installProviderSettings(archive);

    try {
      expect((await provider("claude-code").models()).map((one) => one.id)).toEqual(["claude-the-walking-login"]);

      // With no store of logins installed — a script, a fixture — the machine's own directory is
      // what the CLI itself would read there, and it is what answers.
      installProviderSettings(null);
      expect((await provider("claude-code").models()).map((one) => one.id)).toEqual(["claude-the-machine"]);
    } finally {
      installProviderSettings(null);
      machine.close();
      walked.close();
    }
  });
});

// The other half of the interface. A harness is asked for the command a run is carried on and
// then for the reply read out of what the finished process left behind, and both are asked of the
// same one — a run built by one program and read by another's vocabulary is the defect this
// closes.
describe("what a finished run left behind is read through the harness that built it", () => {
  const stream = [
    JSON.stringify({ type: "rate_limit_event", rate_limit_info: { unifiedWindows: { five_hour: { utilization: 0.4, resetsAt: 1_788_570_000 } } } }),
    JSON.stringify({
      type: "result",
      subtype: "success",
      is_error: false,
      result: "All done.",
      session_id: "s-9",
      usage: { input_tokens: 12, cache_read_input_tokens: 90_000 },
      modelUsage: { "claude-opus-5": { inputTokens: 12, cacheReadInputTokens: 90_000, contextWindow: 1_000_000, costUSD: 0.2 } },
    }),
  ].join("\n");

  // Claude Code's reader is the CLI's own reader and nothing beside it, so every line that runs
  // today reads exactly the bytes it read before the reading went behind the interface.
  test("Claude Code's reader is the CLI's own, answer for answer and failure for failure", async () => {
    expect(await claudeCode.readAnswer({ stdout: stream, stderr: "", exitCode: 0 })).toEqual(
      readClaudeOutput(stream, "", 0),
    );

    // The failures come back the same way too, which is what the callers that catch them go by.
    expect(() => claudeCode.readAnswer({ stdout: "", stderr: "Claude AI usage limit reached|1788469200", exitCode: 1 })).toThrow(
      ClaudeLimitError,
    );
    expect(() => claudeCode.readAnswer({ stdout: "", stderr: "boom", exitCode: 1 })).toThrow(/exited with code 1/);
  });

  // The lines that ask for one object rather than a stream — a browse, a fold, a judgement — read
  // through the same door and through the same reader. This is the shape that used to be taken
  // apart where it was read, so what is held here is that going through the harness changed the
  // reading not at all: the same reply, and in it the three figures a browse runner works from.
  test("an answer that came back as one object reads the way the CLI's own reader reads it", async () => {
    const object = JSON.stringify({
      type: "result",
      subtype: "success",
      is_error: false,
      result: "Found it.",
      session_id: "s-4",
      usage: { input_tokens: 30, cache_read_input_tokens: 4_000 },
      modelUsage: { "claude-opus-5": { inputTokens: 30, cacheReadInputTokens: 4_000, contextWindow: 1_000_000, costUSD: 0.1 } },
    });
    const reply = await claudeCode.readAnswer({ stdout: object, stderr: "", exitCode: 0 });

    expect(reply).toEqual(readClaudeOutput(object, "", 0));
    expect(reply.text).toBe("Found it.");
    expect(reply.contextTokens).toBe(4_030);
    expect(reply.spend.map((one) => one.model)).toEqual(["claude-opus-5"]);

    // An error result and an exit of its own are refused here as they always were, which is what
    // the runner that used to judge those fields itself now hears as a failure to read.
    expect(() =>
      claudeCode.readAnswer({ stdout: JSON.stringify({ type: "result", is_error: true, result: "boom", session_id: "s-4" }), stderr: "", exitCode: 0 }),
    ).toThrow(/error result/);
  });

  // What a judgement, a fold and a warming ping each want is narrower than a whole reply, and all
  // three of them are in the reply already: the text a gate answers with, the session a fold
  // folded, and what either of them cost. Nothing is asked of the interface beyond the two
  // questions it states.
  test("the narrower readings are all in the reply the interface already answers", async () => {
    const reply = await claudeCode.readAnswer({ stdout: stream, stderr: "", exitCode: 0 });

    expect(reply.text).toBe("All done.");
    expect(reply.sessionId).toBe("s-9");
    expect(reply.spend.map((one) => one.cacheReadTokens)).toEqual([90_000]);
  });
});

// A job's run on the Claude road resumes a session, and everything after it is that same session:
// the round that asks it again over words added while it worked, the record the next job on this
// model picks up, the fold a full one gets. A program that keeps no session names none in its
// reply, and the job runner answers for that case rather than assuming.
describe("what the job line needs of the harness carrying it", () => {
  test("a program that keeps no session names none, so a job on it has nothing to resume", async () => {
    expect((await dsh.readAnswer({ stdout: "All done.", stderr: "", exitCode: 0 })).sessionId).toBeNull();
  });
});

