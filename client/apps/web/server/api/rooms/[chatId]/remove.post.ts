import type { RoomRemovalAnswer } from "#shared/types";

// This room's registration removed with its whole record, carried to the receiver under the administrator's name.
// It waits out the room's turns being cut and every holder's part, so it is given long.
const REMOVE_WAIT_MS = 180_000;

export default defineEventHandler(async (event): Promise<RoomRemovalAnswer> => {
  const signed = await requireAdministrator(event);
  const chatId = requireChatId(event);

  return await receiverPost<RoomRemovalAnswer>("/rooms/remove", { chat: chatId, by: signed.name }, { timeoutMs: REMOVE_WAIT_MS });
});
