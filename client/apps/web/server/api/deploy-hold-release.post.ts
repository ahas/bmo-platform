import type { DeployHoldReleased } from "#shared/types";

// The deployment's hold let go by hand. The panel asks first, with what the hold waits for and what letting
// it go does in front of him, so the press arrives here meaning it; bmo-job is the one writer of the hold and
// tells the queue how it stands, which is said in the pack's words for what came of it.
export default defineEventHandler(async (event): Promise<DeployHoldReleased> => {
  await requireUserSession(event);

  const answer = await jobPost<{ released: boolean; told?: QueueTold | null }>("/deploy-hold-release", {});

  return { ok: true, queue: queueToldText(answer.told) };
});
