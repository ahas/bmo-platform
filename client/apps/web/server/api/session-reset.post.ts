import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

import type { SessionView } from "#shared/agency";

// An ask to clear one agent's session, carried to the agency under the signed-in user, who it is kept as asked
// by (the agency's `docs/api.md`, 「sessions」). A room's session is cleared by ending the session that room holds;
// which sessions there are is the agency's word.
export default defineEventHandler(async (event): Promise<SessionView> => {
  await requireUserSession(event);
  const body = await readBody<{ agent?: unknown; chatId?: unknown }>(event);

  if (!isAgentName(body?.agent)) throw fail(400, wording("agents.error.no_such_agent_name"));
  if (body.chatId !== null && body.chatId !== undefined && typeof body.chatId !== "number") throw fail(400, wording("agents.error.session_reset"));

  const roomId = typeof body.chatId === "number" ? body.chatId : undefined;
  const page = await agencyCall(
    agency().GET("/sessions", { params: { query: { agentName: body.agent, ...(roomId === undefined ? {} : { roomId }) } } })
  );
  const session = page.items[0];

  if (session === undefined) throw fail(404, wording("agents.error.session_reset"));

  await agencyCall(agency().DELETE("/sessions/{sessionId}", { params: { path: { sessionId: session.id } } }));

  return { ...sessionView(session), lastDoneAt: Math.floor(Date.now() / 1000) };
});
