import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

import type { AgentSamplingView } from "#shared/agency";

interface SamplingBody {
  agent?: unknown;
  sampling?: unknown;
}

// One save of a worker's own sampling, each setting named with its value or null for the engine's
// default (the agency's docs/own-llm.md, 「A worker pinned to a provider」). The bounds are the agency's to check, and a
// refusal comes back with the setting it is about.
export default defineEventHandler(async (event): Promise<AgentSamplingView> => {
  const session = await requireUserSession(event);
  const body = await readBody<SamplingBody>(event);

  if (!isAgentName(body?.agent)) throw fail(400, wording("agents.error.no_such_agent_name"));

  return await agencyChange<AgentSamplingView>("/agent-sampling", { agent: body.agent, sampling: body.sampling }, session.user.name);
});
