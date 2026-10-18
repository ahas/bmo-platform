import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

interface AgentGateBody {
  agent?: unknown;
  gate?: unknown;
  on?: unknown;
}

// One of an agent's own gates switched on or off (the gate unit's docs/gates.md, 「Each agent's own gates」). What is
// refused here is only a request of the wrong shape; which gates an agent's turn passes is the agency's
// word, its refusal said by the pack's sentence for its code.
export default defineEventHandler(async (event): Promise<{ ok: true }> => {
  await requireUserSession(event);

  const body = await readBody<AgentGateBody>(event);

  if (!isAgentName(body?.agent)) {
    throw fail(400, wording("agents.error.no_such_agent_name"));
  }

  if (typeof body.gate !== "string" || body.gate === "" || typeof body.on !== "boolean") {
    throw fail(400, wording("agents.error.bad_agent_gate"));
  }

  return await agencyFetch<{ ok: true }>(
    "/agent-gate",
    { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ agent: body.agent, gate: body.gate, on: body.on }) }
  );
});
