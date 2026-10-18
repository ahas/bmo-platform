import type { RoomStats } from "#shared/types";

// What one room adds up to. The usage, the sessions and the turns it was counted from are served by no unit
// (docs/boundary.md, 「Gaps the console meets」), so the room's figures stand empty.
export default defineEventHandler(async (event): Promise<RoomStats> => {
  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);

  return emptyRoomStats(chatId);
});
