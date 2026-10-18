import type { PoolBoard } from "#shared/agency";

// What holds each place of the common co-worker pool right now, as the agency reads it.
export default defineEventHandler(async (event): Promise<PoolBoard> => {
  await requireUserSession(event);
  return await agencyFetch<PoolBoard>("/coworker-pool");
});
