// An agent's own thinking and effort, over its tier (docs/model-tiers.md, 「An agent's own thinking
// and effort」). The agent names a level of one scale and whether it thinks; each provider says, beside
// its harness, which levels a run of one of its models takes, whether the thinking switch reaches it,
// and the effort word a run goes out with. Nothing here knows a provider.

import declaredScale from "@agency/policy/reasoning.toml";

// The one scale an agent's level is named on, lowest first: agency's policy
// (docs/policy.md).
export const reasoningLevels: readonly string[] = declaredScale.levels as string[];

// A level of the scale, which `isReasoningLevel` checks.
export type ReasoningLevel = string;

export function isReasoningLevel(value: unknown): value is ReasoningLevel {
  return typeof value === "string" && reasoningLevels.includes(value);
}

// What the agent's row names: whether it thinks and its level, each null where the tier and the line
// decide.
export interface AgentReasoning {
  thinking: boolean | null;
  effort: ReasoningLevel | null;
}

export const tierReasoning: AgentReasoning = { thinking: null, effort: null };

// What one provider makes of an agent's choice.
export interface ProviderReasoning {
  // Whether the agent's thinking switch reaches a run of this provider.
  thinking: boolean;
  // The levels a run of this model takes, in the scale's order; empty where none does.
  levels(model: string): Promise<readonly ReasoningLevel[]>;
  // The effort word a run of this model goes out with, given the word its tier mapping named and the
  // agent's choice.
  effort(mapped: string | null, choice: AgentReasoning, model: string): Promise<string | null>;
}

// A provider whose effort words include the scale's levels and whose program reads the run's own
// thinking switch: the agent's level stands where the model takes it, and the tier's word otherwise.
export function levelReasoning(levels: (model: string) => Promise<readonly ReasoningLevel[]>): ProviderReasoning {
  return {
    thinking: true,
    levels,
    effort: async (mapped, choice, model) => (choice.effort !== null && (await levels(model)).includes(choice.effort) ? choice.effort : mapped),
  };
}

// The scale's levels a vocabulary of effort words holds, in the scale's order.
export function levelsIn(words: readonly (string | null)[]): readonly ReasoningLevel[] {
  return reasoningLevels.filter((level) => words.includes(level));
}
