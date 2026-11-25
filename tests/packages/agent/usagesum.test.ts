import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { claudeSpendOf } from "@agency/agent/spend";

import type { ModelSpend } from "@agency/agent/spend";
import type { TokenSplit } from "@agency/agent/usage";

// A row of what a run cost states the same run twice: how much traffic there was, and how that
// traffic divided into the four kinds. They are two readings of one thing, so the total is the
// four added and nothing else. Where the two disagree, a screen showing both is wrong twice
// over — it puts up a total and a division that cannot both be of the same runs, and nothing on
// the screen says which of them to believe.
//
// The disagreement has two ends it can come from. The writing — a path that records a row
// carrying the sum and no division — is read out of the source by `usagesum-scan.test.ts`. The
// reading — a tally that adds its totals over rows it does not add its kinds over — and every
// recorder driven end to end are here, against a real archive, because it is arithmetic, and
// arithmetic is answered by doing it.

// A row's four kinds added, or nothing at all where any of them is missing. It adds rather than
// writing the sum out, so the order of the four is not part of what is being claimed.
function kindsAdded(row: TokenSplit): number | null {
  const kinds = [row.inputTokens, row.outputTokens, row.cacheReadTokens, row.cacheWriteTokens];

  return kinds.includes(null) ? null : kinds.reduce((sum, kind) => (sum ?? 0) + (kind ?? 0), 0);
}

describe("what every path that records a run actually writes", () => {
  test("a spend built out of a run's own answer sums its four kinds and keeps them", () => {
    const [opus] = claudeSpendOf({
      "claude-opus-5": {
        inputTokens: 12,
        outputTokens: 480,
        cacheReadInputTokens: 30_000,
        cacheCreationInputTokens: 1_500,
        costUSD: 0.42,
      },
    });

    expect(opus?.tokens).toBe(31_992);
    expect(kindsAdded(opus as ModelSpend)).toBe(opus?.tokens ?? -1);

    // And an answer that named one kind and no other still sums to what it named: the kinds it
    // left out are kinds the run did not use, so the total stays the four added.
    const [haiku] = claudeSpendOf({ "claude-haiku-4-5": { inputTokens: 40, costUSD: 0.01 } });

    expect(kindsAdded(haiku as ModelSpend)).toBe(haiku?.tokens ?? -1);
  });
});

