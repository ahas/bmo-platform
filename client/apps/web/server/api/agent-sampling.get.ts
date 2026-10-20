import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

import type { AgentSamplingView } from "#shared/agency";

// A worker's own sampling: the settings it takes with their bounds and what it holds of them (the agency's
// `docs/api.md`, 「workers」). Which workers hold one is the agency's word, and the settings themselves are the
// worker's own.
export default defineEventHandler(async (event): Promise<AgentSamplingView> => {
  await requireUserSession(event);

  const agent = getQuery(event).agent;

  if (!isAgentName(agent)) throw fail(400, wording("agents.error.no_such_agent_name"));

  const worker = await agencyCall(agency().GET("/workers/{workerName}", { params: { path: { workerName: agent } } }));

  return samplingView(worker.sampling, worker.name);
});
