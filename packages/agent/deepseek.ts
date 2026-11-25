import declaredDeepSeek from "@agency/policy/providers/deepseek.toml";

import { memberProvider } from "./member-provider";
import { reasoningLevels } from "./reasoning";

import type { ProviderReasoning } from "./reasoning";

// DeepSeek's own API, reached by Claude Code through agency's gateway member (docs/own-llm.md, 「The
// gateway's members」). Its models are the members of its road, its key the member's key file, and a run's
// thinking reaches it as the reasoning word its backend reads.

// The effort word DeepSeek's own API knows as a launch without thinking.
const off = "none";

// A level an agent chose reaches the route where it is one of the scale's levels; its thinking off is
// DeepSeek's `none`; and a mapping's word stands where the agent chose no level.
const deepSeekReasoning: ProviderReasoning = {
  thinking: true,
  levels: async () => reasoningLevels,
  effort: async (mapped, choice, model) => {
    if (choice.thinking === false) return off;
    const held = await deepSeekReasoning.levels(model);

    return choice.effort !== null && held.includes(choice.effort) ? choice.effort : mapped;
  },
};

export const deepSeekProvider = memberProvider({
  name: "deepseek",
  policy: declaredDeepSeek,
  reasoning: deepSeekReasoning,
});
