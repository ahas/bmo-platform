import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { calledTool } from "@agency/agent/mcp-names";

describe("calledTool", () => {
  test("a toolbook call on the Claude road is the inner tool in the Claude spelling", () => {
    expect(calledTool("mcp__toolbook__call", { tool: "web/naver_price", arguments: { query: "RTX 5090" } })).toEqual({
      name: "mcp__web__naver_price",
      input: { query: "RTX 5090" },
      throughToolbook: true,
    });
  });

  test("a toolbook act on the Codex road is the inner tool in the Codex spelling", () => {
    expect(calledTool("toolbook/act", { tool: "bmo/remind", arguments: { at: "2026-10-06T09:00", text: "check" } })).toEqual({
      name: "bmo/remind",
      input: { at: "2026-10-06T09:00", text: "check" },
      throughToolbook: true,
    });
  });

  test("list and describe are the toolbook's own acts", () => {
    for (const name of ["mcp__toolbook__list", "toolbook/describe"]) {
      const input = { tool: "web/naver_price", arguments: {} };

      expect(calledTool(name, input)).toEqual({ name, input, throughToolbook: false });
    }
  });

  test("a malformed proxy call stays the proxy call", () => {
    const malformed: unknown[] = [
      null,
      "web/naver_price",
      { arguments: {} },
      { tool: "naver_price", arguments: {} },
      { tool: "web/", arguments: {} },
      { tool: "web/naver_price" },
      { tool: "web/naver_price", arguments: ["RTX 5090"] },
      { tool: "web/naver_price", arguments: "{}" },
    ];

    for (const input of malformed) {
      expect(calledTool("mcp__toolbook__call", input)).toEqual({ name: "mcp__toolbook__call", input, throughToolbook: false });
    }
  });

  test("a direct MCP call and a CLI tool come back as made", () => {
    const fetch = { url: "https://example.com" };
    const shell = { command: "ls" };

    expect(calledTool("mcp__web__web_fetch", fetch)).toEqual({ name: "mcp__web__web_fetch", input: fetch, throughToolbook: false });
    expect(calledTool("web/web_fetch", fetch)).toEqual({ name: "web/web_fetch", input: fetch, throughToolbook: false });
    expect(calledTool("Bash", shell)).toEqual({ name: "Bash", input: shell, throughToolbook: false });
  });
});
