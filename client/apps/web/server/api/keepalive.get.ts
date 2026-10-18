import type { KeepaliveView } from "#shared/types";

// Whether the owner has stopped the warming round as a whole, as the cache warmer holds it. Which agents
// are stopped one by one comes over with the agents list, laid over each agent's own row.
export default defineEventHandler(async (event): Promise<KeepaliveView> => {
  await requireUserSession(event);

  return await keepaliveRound();
});
