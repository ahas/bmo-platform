import type { RoomStats } from "#shared/types";

// What one room adds up to: the receiver's figures for it (its `docs/boundary-api.md`, 「A room's figures」), its
// messages by the hour counted in the zone the console shows every time in.
export default defineEventHandler(async (event): Promise<RoomStats> => {
  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);

  const { zone } = useAppConfig().bmoUi as { zone: string };

  return await receiverRead<RoomStats>(`/console/rooms/${chatId}/figures?zone=${encodeURIComponent(zone)}`);
});
