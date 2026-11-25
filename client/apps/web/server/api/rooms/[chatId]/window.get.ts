import type { RoomWindow } from "#shared/types";

// How long the chat agent's window in the room has left, which its listening row stands by: the receiver's
// `GET /rooms/{roomId}/window`, in whole seconds, none where the window stands closed.
export default defineEventHandler(async (event): Promise<RoomWindow> => {
  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);

  const { openForMs } = await receiverCall(
    receiver().GET("/rooms/{roomId}/window", { params: { path: { roomId: chatId } } })
  );

  return { openFor: Math.floor(openForMs / 1000) };
});
