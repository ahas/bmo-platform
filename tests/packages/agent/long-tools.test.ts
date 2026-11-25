import "@agency/test-support/setup";

import { rmSync } from "node:fs";
import { afterAll, describe, expect, test } from "bun:test";

import { scratchDir } from "@agency/test-support/scratch";

import { claudeCode, claudeCodeAt, type ClaudeEndpoint } from "@agency/agent/claude-code";
import { codex } from "@agency/agent/codex";
import { dsh } from "@agency/agent/dsh";
import { gatewayLongToolHeaders } from "@agency/agent/gateway/address";
import type { HarnessRun } from "@agency/agent/harness";

// Each program names its own tools whose calls wait long, and a run on the gateway sends them with every
// request (docs/agent-control.md, 「What an agent provider is made of」; docs/own-llm.md, 「How requests go」).

const home = scratchDir("bmo-long-tools-");

afterAll(() => rmSync(home, { recursive: true, force: true }));

describe("a program's long tools", () => {
  test("are Claude Code's shell, sub-agents and fetches from the web, and Codex and dsh name none", () => {
    expect(claudeCode.longTools).toEqual(["Bash", "Agent", "Task", "WebFetch", "WebSearch"]);
    expect(codex.longTools).toBeUndefined();
    expect(dsh.longTools).toBeUndefined();
  });

  test("go to the gateway in one header, names joined by commas, and a program naming none sends none", () => {
    expect(gatewayLongToolHeaders(["Bash", "Agent"])).toEqual({ "x-bmo-long-tools": "Bash,Agent" });
    expect(gatewayLongToolHeaders([])).toEqual({});
    expect(gatewayLongToolHeaders(undefined)).toEqual({});
  });

  test("stand in the custom-headers line of a run of Claude Code on a road", async () => {
    let handed: readonly string[] | undefined;
    const endpoint: ClaudeEndpoint = {
      provider: "registered",
      baseUrl: "http://[::1]:9",
      keyPath: () => `${home}/key`,
      async model(_run, longTools) {
        handed = longTools;
        return {
          name: "qwen-test",
          contextTokens: 100_000,
          answerTokens: 8_000,
          headers: gatewayLongToolHeaders(longTools),
          thinking: true,
          home,
        };
      },
      effort: ({ thinking }) => (thinking ? "medium" : "low"),
      homes: home,
      offers: [],
    };
    const run: HarnessRun = {
      line: "chat", model: "qwen-test", effort: "high", systemPromptPath: null, toolsShown: "", toolsAllowed: null, toolsDenied: null,
      mcpConfigPath: null, prompt: "hello", promptDelivery: "input", resumeSessionId: null, keepSession: true, hostCommands: false,
      thinking: true, answerShape: "unread", cwd: home, env: { PATH: "/usr/bin:/bin" },
    };

    const command = await claudeCodeAt(endpoint).command(run);

    expect(handed).toEqual(claudeCode.longTools);
    expect(command.env?.ANTHROPIC_CUSTOM_HEADERS).toBe("x-bmo-long-tools: Bash,Agent,Task,WebFetch,WebSearch");
    expect(command.env?.API_TIMEOUT_MS).toBe("2147483647");
    expect(command.env?.BUN_CONFIG_HTTP_IDLE_TIMEOUT).toBe("2147483");
  });
});
