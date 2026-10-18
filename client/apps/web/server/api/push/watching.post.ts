import { wording } from "#shared/wording";

// An open tab saying which room it shows and whether it can be seen (docs/web.md, 「Notifications」).
// It is written against the account logged in, so a room's notification passes over exactly the
// devices of whoever is looking at it.
export default defineEventHandler(async (event): Promise<{ ok: true }> => {
  const session = await requireUserSession(event);
  const body = (await readBody(event)) as { tab?: unknown; chatId?: unknown; visible?: unknown } | null;
  const tab = typeof body?.tab === "string" ? body.tab.trim() : "";

  if (tab === "" || tab.length > 100 || typeof body?.visible !== "boolean") {
    throw fail(400, wording("push.error.bad_watch"));
  }

  const chatId = Number.isSafeInteger(body.chatId) ? (body.chatId as number) : null;

  roomWatchers.report({ tab, user: session.user.name, chatId, visible: body.visible }, Date.now());

  return { ok: true };
});
