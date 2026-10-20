import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

interface ReasoningBody {
  agent?: unknown;
  thinking?: unknown;
  effort?: unknown;
}

// One agent's own thinking and effort, each null to leave it to the tier and the line
// (the agency's docs/model-tiers.md, 「An agent's own thinking and effort」). What is refused here is only a
// request of the wrong shape; which agents take a choice of their own and which levels there are is
// the agency's word, its refusal said by the pack's sentence for its code.
export default defineEventHandler(async (event): Promise<{ ok: true }> => {
  await requireUserSession(event);

  const body = await readBody<ReasoningBody>(event);

  if (!isAgentName(body?.agent)) {
    throw fail(400, wording("agents.error.no_such_agent_name"));
  }

  if (body.thinking !== null && typeof body.thinking !== "boolean") {
    throw fail(400, wording("agents.error.bad_thinking"));
  }

  if (body.effort !== null && typeof body.effort !== "string") {
    throw fail(400, wording("agents.error.bad_effort"));
  }

  await agencyCall(
    agency().PATCH("/agents/{agentName}", {
      params: { path: { agentName: body.agent } },
      body: { gates: null, reasoning: { thinking: body.thinking, effort: body.effort }, role: null, voice: null }
    })
  );

  return { ok: true };
});
