import type { RoomPicturePage } from "#shared/types";

// The pictures a room holds, which the picture viewer walks across the whole room. No unit serves a room's pictures
// or their files to the console (docs/boundary.md, 「Gaps the console meets」), so the stretch stands empty.
export default defineEventHandler(async (event): Promise<RoomPicturePage> => {
  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);

  return { pictures: [], hasMore: false, before: 0, total: 0 };
});
