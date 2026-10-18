import type { LiveSnapshot, StatusView } from "#shared/types";

// What is running now, as the receiver's live picture holds it: the rooms whose session is folding, at the
// moment it was read. The runs going on beside the chat — a runner, a job worker, a gate, an editor — and what
// has just run are lists no unit serves, so they stand empty (docs/boundary.md, 「Gaps the console meets」). A
// receiver that does not answer leaves the picture empty rather than failing the route.
export default defineEventHandler(async (event): Promise<StatusView> => {
  await requireUserSession(event);

  const history = { executions: [], watches: [], gates: [], runners: [], jobs: [], machine: null, nodes: null };

  try {
    const snapshot = await unitFetch<Pick<LiveSnapshot, "at" | "compacting">>("RECEIVER", "/live");

    return { reachable: true, live: [], compacting: snapshot.compacting ?? [], liveAt: snapshot.at, ...history };
  } catch (error) {
    return { reachable: false, slow: unitSlow(error), live: [], compacting: [], liveAt: null, ...history };
  }
});
