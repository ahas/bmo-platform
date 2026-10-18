import { wording } from "#shared/wording";

import type { CompanionView } from "#shared/agency";

// One save of a companion's settings, carried to the agency under the signed-in user, who the change is
// kept as made by (the agency's docs/companions.md, 「Its settings」).
export default defineEventHandler(async (event): Promise<CompanionView> => {
  const session = await requireUserSession(event);
  const body = await readBody<Record<string, unknown>>(event);

  if (typeof body !== "object" || body === null || typeof body.agent !== "string") throw fail(400, wording("agents.error.companion_change"));

  const { by: _ignored, ...change } = body;

  return await agencyChange<CompanionView>("/companions", change, session.user.name);
});
