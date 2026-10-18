import { wording } from "#shared/wording";

// The picture the page of one address in a room's message named. Reading pages for their pictures is no unit's to
// serve the console yet (docs/boundary.md, 「Gaps the console meets」), so every link card stands without one.
export default defineEventHandler(async (event) => {
  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);
  requireMessageId(event);

  throw fail(404, wording("common.error.image_unavailable"));
});
