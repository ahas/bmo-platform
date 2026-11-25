import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { lastRequestTokensFromText } from "@agency/agent/cli-transcripts";

// A turn that called tools has a result summing every request, so what a session holds is its last
// request as the record keeps it; a zero count, as a stream's placeholder, is no request
// (docs/own-llm.md, 「How requests go」).
describe("what a session holds", () => {
  test("is the last request the record counts, cache and all", () => {
    const record = [
      { type: "assistant", message: { usage: { input_tokens: 8_000, cache_read_input_tokens: 30_000 } } },
      { type: "user", message: { content: "next" } },
      { type: "assistant", message: { usage: { input_tokens: 500, cache_creation_input_tokens: 100, cache_read_input_tokens: 38_000 } } },
      { type: "assistant", message: { usage: { input_tokens: 0, cache_read_input_tokens: 0 } } },
    ].map((entry) => JSON.stringify(entry)).join("\n");

    expect(lastRequestTokensFromText(`${record}\n{"type":"assis`)).toBe(38_600);
    expect(lastRequestTokensFromText("{\"type\":\"user\"}")).toBeNull();
  });
});
