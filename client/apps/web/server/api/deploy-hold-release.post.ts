import type { DeployHoldReleased } from "#shared/types";

// The deployment's hold let go by hand. The panel asks first, with what the hold waits for and what letting it go
// does in front of him, so the press arrives here meaning it; bmo-job is the one writer of the hold. The fixed API
// keeps a hold on a queue, not a hold of a deployment, so every queue's holds are let go.
export default defineEventHandler(async (event): Promise<DeployHoldReleased> => {
  await requireUserSession(event);

  const queues = await readQueues();

  await Promise.all(queues.map((queue) => jobCall(job().PATCH("/queues/{queueName}", {
    params: { path: { queueName: queue.name } },
    body: { holds: [] }
  }))));

  return { ok: true, queue: null } satisfies DeployHoldReleased;
});
