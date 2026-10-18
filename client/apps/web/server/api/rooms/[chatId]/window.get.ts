import type { RoomWindow } from "#shared/types";

// How long the chat agent's window in the room has left, which its listening row stands by. The receiver keeps the
// window and serves it by no route (docs/boundary.md, 「Gaps the console meets」), so no window stands open and the row
// is not drawn.
export default defineEventHandler(async (event): Promise<RoomWindow> => {
  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);

  return { openFor: 0 };
});
