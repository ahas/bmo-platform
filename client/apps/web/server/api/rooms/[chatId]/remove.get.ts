import type { RoomRemovalPreview } from "#shared/types";

// What removing this room's registration would take and leave, read by the receiver with nothing written, for the
// administrator alone. It asks every holder and counts the room's files, which takes seconds and more under load.
const PREVIEW_WAIT_MS = 60_000;

export default defineEventHandler(async (event): Promise<RoomRemovalPreview> => {
  const signed = await requireAdministrator(event);
  const chatId = requireChatId(event);
  const asked = new URLSearchParams({ chat: String(chatId), by: signed.name });

  return await receiverRead<RoomRemovalPreview>(`/rooms/remove?${asked}`, { timeoutMs: PREVIEW_WAIT_MS });
});
