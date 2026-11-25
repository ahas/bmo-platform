// What a run cost, in the one shape this bot says it in, and the readers that put a harness's own
// answer into that shape. Claude Code and Codex both report the four kinds apart; the dsh harness
// prints its answer and nothing beside it, so a run carried there is recorded with no figures
// rather than with invented ones. Provider vocabulary travels no further than each reader.
// Everything downstream — the usage tables, the run rows, a job's spans, every screen that reads
// them — knows the shape below and nothing about who reported it.
//
// This is the seam another harness is added at. Adding one is writing one more reader beside this
// one, in the field names that harness uses, and touching nothing else: the shape is the one the
// archive already stores, so a run through a new harness is recorded exactly as the others.

// What one model cost in one run stands with the archive, which keeps it (`run-rows.ts`).
export type { ModelSpend } from "@agency/archive/run-rows";
import type { ModelSpend } from "@agency/archive/run-rows";

// What the Claude CLI says about one model in one run, under the CLI's own names. The context
// window stands here with the rest because the CLI states it in the same object; what a run cost
// and how much the model could have been told arrive together.
export interface ClaudeModelUsage {
  contextWindow?: number;
  inputTokens?: number;
  outputTokens?: number;
  cacheReadInputTokens?: number;
  cacheCreationInputTokens?: number;
  costUSD?: number;
}

// What a run through the Claude CLI cost, model by model. The CLI states the four kinds apart on
// every run and they are carried on apart: nothing here adds them together except to keep the sum
// beside them, and a kind the answer left out reads as nought because a kind the CLI does not
// name is a kind this run did not use.
export function claudeSpendOf(modelUsage: Record<string, ClaudeModelUsage> | undefined): ModelSpend[] {
  return Object.entries(modelUsage ?? {}).map(([model, usage]) => {
    const inputTokens = usage.inputTokens ?? 0;
    const outputTokens = usage.outputTokens ?? 0;
    const cacheReadTokens = usage.cacheReadInputTokens ?? 0;
    const cacheWriteTokens = usage.cacheCreationInputTokens ?? 0;

    return {
      mode: "claude-code",
      model,
      tokens: inputTokens + outputTokens + cacheReadTokens + cacheWriteTokens,
      inputTokens,
      outputTokens,
      cacheReadTokens,
      cacheWriteTokens,
      cost: usage.costUSD ?? 0,
    };
  });
}
