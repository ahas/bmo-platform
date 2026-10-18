import { wording } from "#shared/wording";

import type { QueuePauseChanged } from "#shared/types";

interface QueuePauseBody {
  paused?: unknown;
}

// The auto-progression stopped or started again. What it holds is the calling of what comes next — the
// queue, the approvals and the order stand as they are, and a job or a co-work stage already under way runs
// to its end — so the only thing refused here is a request that says neither yes nor no. bmo-job is the one
// writer of the switch, as it is of the table.
export default defineEventHandler(async (event): Promise<QueuePauseChanged> => {
  await requireUserSession(event);

  const body = await readBody<QueuePauseBody>(event);

  if (typeof body?.paused !== "boolean") {
    throw fail(400, wording("todo.error.queue_pause_direction"));
  }

  const answer = await jobPost<{ paused: boolean; deploy_held: boolean; told?: QueueTold | null }>("/queue-pause", { paused: body.paused });

  return { ok: true, paused: answer.paused, queue: queueToldText(answer.told), deployHold: answer.deploy_held };
});
