import { wording } from "#shared/wording";

// A photo is served by the message it belongs to, never by a path the browser names. No unit serves a message's
// files to the console (docs/boundary.md, 「Gaps the console meets」), so every photo answers as not found, and the
// room stands its frame without it.
export default defineEventHandler(async (event) => {
  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);

  const messageId = Number(getRouterParam(event, "messageId") ?? "");

  if (!Number.isSafeInteger(messageId)) {
    throw fail(400, wording("room.error.bad_message_id"));
  }

  throw fail(404, wording("room.error.photo_not_found"));
});
