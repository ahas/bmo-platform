import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { claudeCode } from "@agency/agent/claude-code";

describe("a streamed response as its host's pane shows it", () => {
  test("names a call made through the toolbook by the tool it reached", () => {
    const line = JSON.stringify({
      type: "assistant",
      message: {
        content: [
          { type: "text", text: "checking the price" },
          { type: "tool_use", id: "t1", name: "mcp__toolbook__call", input: { tool: "web/naver_price", arguments: { query: "RTX 5090" } } },
        ],
      },
    });

    expect(claudeCode.progressOf?.(line)).toBe("checking the price → mcp__web__naver_price");
  });
});
