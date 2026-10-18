import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

import type { CompanionResetAnswer } from "#shared/agency";

// A companion's reset of the ranges the administrator chose, carried to the agency under their name
// (the agency's docs/companions.md, 「Starting it afresh」). It walks every archive copy under the data root,
// which takes seconds and more under load, so it is given longer than a setting's save.
const RESET_WAIT_MS = 150_000;

export default defineEventHandler(async (event): Promise<CompanionResetAnswer> => {
  const signed = await requireAdministrator(event);
  const body = await readBody<{ agent?: unknown; conversation?: unknown; person?: unknown }>(event);

  if (!isAgentName(body?.agent)) throw fail(400, wording("agents.error.no_such_agent_name"));

  return await agencyChange<CompanionResetAnswer>(
    "/companions/reset",
    { agent: body.agent, conversation: body.conversation === true, person: body.person === true },
    signed.name,
    RESET_WAIT_MS,
  );
});
