import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { Archive } from "@agency/archive/archive";
import { windowsOf } from "@agency/agent/claude";
import { claudeSpendOf } from "@agency/agent/spend";
import { figureText, kindFigures, kindsText, totalText } from "@agency/agent/usage";

describe("what a run's answer says about usage", () => {
  test("every window the API named is read, and one without a figure is not a window", () => {
    expect(
      windowsOf({
        unifiedWindows: {
          five_hour: { utilization: 0.04, resetsAt: 1_788_570_000 },
          seven_day: { utilization: 0.01, resetsAt: 1_789_074_000 },
          seven_day_overage_included: {},
        },
      }),
    ).toEqual([
      { name: "five_hour", utilization: 0.04, resetsAt: 1_788_570_000 },
      { name: "seven_day", utilization: 0.01, resetsAt: 1_789_074_000 },
    ]);

    expect(windowsOf(null)).toEqual([]);
  });

  test("what a model cost keeps the four kinds apart, and sums them beside", () => {
    expect(
      claudeSpendOf({
        "claude-haiku-4-5": {
          inputTokens: 9,
          outputTokens: 129,
          cacheReadInputTokens: 100,
          cacheCreationInputTokens: 6885,
          costUSD: 0.0144,
        },
      }),
    ).toEqual([
      {
        mode: "claude-code",
        model: "claude-haiku-4-5",
        tokens: 7123,
        inputTokens: 9,
        outputTokens: 129,
        cacheReadTokens: 100,
        cacheWriteTokens: 6885,
        cost: 0.0144,
      },
    ]);
  });

  test("a kind the answer never named is a kind the run did not use", () => {
    expect(claudeSpendOf({ "claude-haiku-4-5": { inputTokens: 40, costUSD: 0.01 } })).toEqual([
      {
        mode: "claude-code",
        model: "claude-haiku-4-5",
        tokens: 40,
        inputTokens: 40,
        outputTokens: 0,
        cacheReadTokens: 0,
        cacheWriteTokens: 0,
        cost: 0.01,
      },
    ]);
  });
});

// The words the shell says a run's cost in. Four kinds, each under its own name, and a total
// that stands with them rather than in place of them. A resumed session sends its whole prefix
// again on every turn, so the total climbs for as long as the session lives while nothing new is
// being bought — which is why the kinds are there, and why none of them is folded into another.
describe("the words a run's cost is said in", () => {
  const say = (tokens: number): string => tokens.toLocaleString("en-US");

  test("each kind under its own name, in the order a run makes them", () => {
    const split = { inputTokens: 100, outputTokens: 20, cacheReadTokens: 9_000_000, cacheWriteTokens: 300 };

    expect(kindFigures(split, say).map(figureText)).toEqual([
      "read 100",
      "write 20",
      "cache read 9,000,000",
      "cache write 300",
    ]);
    expect(kindsText(split, say)).toBe("read 100 · write 20 · cache read 9,000,000 · cache write 300");
    expect(totalText(9_000_420, say)).toBe("9,000,420 in all");
  });

  test("a kind nothing was measured for is a bare dash, with no word in front of it", () => {
    const unmeasured = { inputTokens: null, outputTokens: null, cacheReadTokens: null, cacheWriteTokens: null };

    // A word in front of a dash would name a figure that is not there, and a nought would claim
    // the run burnt none.
    expect(kindFigures(unmeasured, say)).toEqual([
      { label: "", text: "—" },
      { label: "", text: "—" },
      { label: "", text: "—" },
      { label: "", text: "—" },
    ]);
    expect(kindsText(unmeasured, say)).toBe("— · — · — · —");
  });
});

