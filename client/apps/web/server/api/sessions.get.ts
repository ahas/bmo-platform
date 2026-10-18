import type { SessionView } from "#shared/agency";

// Every session an agent holds now, as the agency reads them (the agency's docs/session-resets.md).
export default defineEventHandler(async (event): Promise<SessionView[]> => {
  await requireUserSession(event);

  return await agencyFetch<SessionView[]>("/sessions");
});
