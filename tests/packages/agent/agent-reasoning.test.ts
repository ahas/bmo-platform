import "@agency/test-support/setup";

import { expect, test } from "bun:test";

import { agentEffort, mappedRun } from "@agency/agent/model-settings";

import type { HarnessRun } from "@agency/agent/harness";

// The effort word each provider makes of a level (docs/provider-order.md, 「Effort and thinking」).

test("each provider makes its own word of an agent's choice, and the tier's word stands where none reaches", async () => {
  const level = (effort: "high" | "max") => ({ thinking: null, effort });

  // Claude Code takes every level of a model its catalogue names no efforts for.
  expect(await agentEffort({ provider: "claude-code", model: "claude-unlisted", effort: "high" }, level("max"))).toBe("max");
  // OpenRouter's highest is xhigh; a mapping of it names no effort.
  expect(await agentEffort({ provider: "openrouter", model: "some/model", effort: null }, level("high"))).toBe("high");
  expect(await agentEffort({ provider: "openrouter", model: "some/model", effort: null }, level("max"))).toBeNull();
  // A registered model takes thinking alone, as its effort word.
  expect(await agentEffort({ provider: "registered", model: "local-0-1", effort: null }, { thinking: true, effort: "max" })).toBe("thinking");
  expect(await agentEffort({ provider: "registered", model: "local-0-1", effort: "thinking" }, { thinking: false, effort: null })).toBe("no-thinking");
  expect(await agentEffort({ provider: "registered", model: "local-0-1", effort: null }, level("max"))).toBeNull();
  // A run no agent's choice reaches goes out on its mapping.
  expect(await agentEffort({ provider: "claude-code", model: "claude-unlisted", effort: "high" }, undefined)).toBe("high");
  expect(await mappedRun({ effort: "high" } as HarnessRun, { provider: "codex", model: "terra", effort: "low", tier: "Fine" })).toMatchObject({ model: "terra", effort: "low", tier: "Fine" });
  expect((await mappedRun({ effort: "high", reasoning: level("max") } as HarnessRun, { provider: "codex", model: "terra", effort: "low", tier: "Fine" })).effort).toBe("max");
});
