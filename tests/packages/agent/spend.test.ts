import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { claudeSpendOf } from "@agency/agent/spend";

// The one harness that reports what a run cost today, read off its answer in its own field names.
// What is asked of it is that what comes out is the shape the archive stores, so a row written
// from any road says what it cost in the same words — and that a road which reports nothing
// writes nothing rather than a row of noughts.

describe("the Claude CLI's own answer", () => {
  test("keeps the four kinds it states apart, and sums them beside", () => {
    expect(
      claudeSpendOf({
        "claude-opus-5": {
          contextWindow: 1_000_000,
          inputTokens: 12,
          outputTokens: 480,
          cacheReadInputTokens: 30_000,
          cacheCreationInputTokens: 1_500,
          costUSD: 0.42,
        },
      }),
    ).toEqual([
      {
        mode: "claude-code",
        model: "claude-opus-5",
        tokens: 31_992,
        inputTokens: 12,
        outputTokens: 480,
        cacheReadTokens: 30_000,
        cacheWriteTokens: 1_500,
        cost: 0.42,
      },
    ]);
  });

  test("a run that reported nothing at all is no spend rather than a row of noughts", () => {
    expect(claudeSpendOf(undefined)).toEqual([]);
    expect(claudeSpendOf({})).toEqual([]);
  });
});

