import { wording } from "#shared/wording";

import type { BasePromptsView } from "#shared/agency";

// One change of the base prompts — one made, saved or removed, or an agent's choice — carried to the
// bot under the signed-in user, who it is kept as made by (the agency's docs/base-prompts.md, 「Saving」).
export default defineEventHandler(async (event): Promise<BasePromptsView> => {
  const session = await requireUserSession(event);
  const body = await readBody<Record<string, unknown>>(event);

  if (typeof body !== "object" || body === null || typeof body.action !== "string") throw fail(400, wording("agents.error.base_prompt_action"));

  const { by: _ignored, ...change } = body;

  return await agencyChange<BasePromptsView>("/base-prompts", change, session.user.name);
});
