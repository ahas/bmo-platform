import { provider as providerNamed } from "./agent-providers";

import type { TierMapping } from "@agency/archive/model-tiers";
import type { HarnessRun } from "./harness";
import type { AgentReasoning } from "./reasoning";

// A run with a tier mapping laid on it: the mapping's model and tier, and the effort word its provider
// makes of the mapping's word and the agent's own choice the run carries (docs/model-tiers.md, 「An
// agent's own thinking and effort」). Every place a mapping is laid on a run lays it this way, so the
// record, the session and the prefix of the run carry the word it went out with.
export async function mappedRun(run: HarnessRun, mapping: Pick<TierMapping, "provider" | "model" | "effort" | "tier">): Promise<HarnessRun> {
  return { ...run, model: mapping.model, effort: await agentEffort(mapping, run.reasoning) ?? "", tier: mapping.tier };
}

// The effort word a run of a mapping goes out with where an agent's choice reaches it: the provider's
// own answer, and the mapping's word where no choice does.
export async function agentEffort(mapping: Pick<TierMapping, "provider" | "model" | "effort">, choice: AgentReasoning | null | undefined): Promise<string | null> {
  return choice == null ? mapping.effort : await providerNamed(mapping.provider).reasoning.effort(mapping.effort, choice, mapping.model);
}

export { configuredProviderEnabled } from "./provider-order";
