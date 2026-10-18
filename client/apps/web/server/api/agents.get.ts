import type { AgentList, AgentSummary, RoleView } from "#shared/types";

// The catalogue of agents as the agency answers it: who they are, what each is called and the roles an agent
// may hold, with each agent's keepalive as the cache warmer holds it laid over its row. An agency that does
// not answer leaves the list empty and says so. The gate order is the gate unit's, which is not split yet, so
// none stands beside the list (docs/boundary.md, 「Screens of units not yet split」).
export default defineEventHandler(async (event): Promise<AgentList> => {
  await requireUserSession(event);

  try {
    const answer = await agencyRead<{ agents?: AgentSummary[]; roles?: RoleView[] }>("/agents");

    return { reachable: true, agents: await withKeepalive(answer.agents ?? [], keepaliveConsumer()), roles: answer.roles ?? [], gateOrder: null };
  } catch (error) {
    // Which agents and roles there are is the agency's word, so none are offered while it is not answering.
    return { reachable: false, slow: unitSlow(error), agents: [], roles: [], gateOrder: null };
  }
});
