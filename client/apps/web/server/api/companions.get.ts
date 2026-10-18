import type { CompanionView } from "#shared/agency";

// Every companion's settings as the agency reads them (the agency's docs/companions.md, 「Its settings」): its model,
// the pieces of its system prompt, its persona, how its model is asked, and its latest changes.
export default defineEventHandler(async (event): Promise<CompanionView[]> => {
  await requireUserSession(event);

  return await agencyFetch<CompanionView[]>("/companions");
});
