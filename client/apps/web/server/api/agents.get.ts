import type { AgentList } from "#shared/types";

// The catalogue of agents as the agency answers it: its agents and its workers, and the roles an agent may hold,
// with each agent's keepalive as the cache warmer holds it laid over its row. An agency that does not answer
// leaves the list empty and says so. The gate order is the gate unit's, which is not split yet, so none stands
// beside the list (docs/boundary.md, 「Screens of units not yet split」).
export default defineEventHandler(async (event): Promise<AgentList> => {
  await requireUserSession(event);

  try {
    const [agents, workers, roles] = await Promise.all([
      agencyCall(agency().GET("/agents")),
      agencyCall(agency().GET("/workers")),
      agencyCall(agency().GET("/roles"))
    ]);

    return {
      reachable: true,
      agents: await withKeepalive(agentSummaries(agents.items, workers.items), keepaliveConsumer()),
      roles: roles.items.map(roleView),
      gateOrder: null
    };
  } catch (error) {
    // Which agents and roles there are is the agency's word, so none are offered while it is not answering.
    return { reachable: false, slow: unitSlow(error), agents: [], roles: [], gateOrder: null };
  }
});
