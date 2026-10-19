import type { RoomWindow } from "#shared/types";

// How long the chat agent's window in the room has left, which its listening row stands by: the receiver's
// `GET /console/rooms/{chat}/window`, 0 where the window stands closed.
export default defineEventHandler(async (event): Promise<RoomWindow> => {
  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);

  const { openFor } = await receiverRead<{ openFor: number; closesAt: number | null }>(`/console/rooms/${chatId}/window`);

  return { openFor };
});
