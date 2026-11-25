import type { AgentProviderName } from "./harness";
import type { AgentReasoning } from "./reasoning";

// One provider and one of its models a caller names for a run in place of the road its key's route
// would give it.
export interface NamedRoad {
  provider: AgentProviderName;
  model: string;
  // How the run thinks, where the caller names it: the choice of the agent the run works for, in
  // place of the one the run's own work names.
  reasoning?: AgentReasoning;
}

