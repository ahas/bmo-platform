import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

import type { AgentSamplingView } from "#shared/agency";

// One worker's own sampling: the settings it takes with their bounds and what it holds of them
// (the agency's docs/own-llm.md, 「A worker pinned to a provider」). Which workers hold one is the agency's word.
export default defineEventHandler(async (event): Promise<AgentSamplingView> => {
  await requireUserSession(event);

  const agent = getQuery(event).agent;

  if (!isAgentName(agent)) throw fail(400, wording("agents.error.no_such_agent_name"));

  return await agencyFetch<AgentSamplingView>(`/agent-sampling?agent=${encodeURIComponent(agent)}`);
});
