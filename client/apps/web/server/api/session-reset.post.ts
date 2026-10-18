import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

import type { SessionView } from "#shared/agency";

// An ask to clear one agent's session, carried to the agency under the signed-in user, who it is kept as
// asked by (the agency's docs/session-resets.md). Which sessions there are is the agency's word.
export default defineEventHandler(async (event): Promise<SessionView> => {
  const session = await requireUserSession(event);
  const body = await readBody<{ agent?: unknown; chatId?: unknown }>(event);

  if (!isAgentName(body?.agent)) throw fail(400, wording("agents.error.no_such_agent_name"));
  if (body.chatId !== null && body.chatId !== undefined && typeof body.chatId !== "number") throw fail(400, wording("agents.error.session_reset"));

  return await agencyChange<SessionView>("/session-reset", { agent: body.agent, chatId: body.chatId ?? null }, session.user.name);
});
