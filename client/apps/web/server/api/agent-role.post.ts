import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

interface RoleBody {
  agent?: unknown;
  role?: unknown;
}

// The role an agent holds from now on (the agency's docs/agent-roles.md, 「Changing a role」). What is refused here
// is only a request of the wrong shape; which roles there are is the agency's word, its refusal said
// by the pack's sentence for its code.
export default defineEventHandler(async (event): Promise<{ ok: true }> => {
  await requireUserSession(event);

  const body = await readBody<RoleBody>(event);

  if (!isAgentName(body?.agent)) {
    throw fail(400, wording("agents.error.no_such_agent_name"));
  }

  if (typeof body.role !== "string" || body.role.trim() === "") {
    throw fail(400, wording("agents.error.bad_role"));
  }

  await agencyCall(
    agency().PATCH("/agents/{agentName}", {
      params: { path: { agentName: body.agent } },
      body: { gates: null, reasoning: null, role: body.role, voice: null }
    })
  );

  return { ok: true };
});
