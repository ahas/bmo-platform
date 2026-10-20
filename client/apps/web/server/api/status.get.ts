import type { JobRecord, LiveSnapshot, StatusView } from "#shared/types";

// What is running now, as the receiver's live picture holds it: the rooms whose session is folding, at the
// moment it was read; and the jobs that have just run, as bmo-job reads them. The runs going on beside the chat
// — a runner, a job worker, a gate, an editor — and the other records of what has just run are lists no unit
// serves, so they stand empty (docs/boundary.md, 「Gaps the console meets」). A unit that does not answer leaves
// its part of the picture empty rather than failing the route.
export default defineEventHandler(async (event): Promise<StatusView> => {
  await requireUserSession(event);

  const [snapshot, jobs] = await Promise.all([
    receiverCall(receiver().GET("/live")).then(
      (read) => ({ read: { at: read.at, compacting: read.compacting.map((one) => ({ chatId: one.room, since: one.since })) }, error: null }),
      (error: unknown) => ({ read: null, error })
    ),
    // A job not finished yet is under way rather than run, and the board tells what has run from what runs by
    // the live runs, which no unit serves yet; so only the finished ones stand among what has just run.
    readRecentJobs().then(
      (read) => read.filter((job) => job.finishedAt !== null),
      (): JobRecord[] => []
    )
  ]);
  const history = { executions: [], watches: [], gates: [], runners: [], jobs, machine: null, nodes: null };

  return snapshot.read === null
    ? { reachable: false, slow: unitSlow(snapshot.error), live: [], compacting: [], liveAt: null, ...history }
    : { reachable: true, live: [], compacting: snapshot.read.compacting ?? [], liveAt: snapshot.read.at, ...history };
});
