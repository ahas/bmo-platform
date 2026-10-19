import type { RoomPicturePage } from "#shared/types";

// The pictures a room holds, which the picture viewer walks across the whole room: the receiver's stretch of them
// (its `docs/boundary-api.md`, 「A room's pictures」), asked for by the picture the viewer holds and the side it walks.
const CURSORS = ["at", "before", "after", "limit"] as const;

export default defineEventHandler(async (event): Promise<RoomPicturePage> => {
  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);

  const query = getQuery(event);
  const asked = new URLSearchParams();

  for (const name of CURSORS) {
    const value = Number(query[name]);

    if (query[name] !== undefined && Number.isSafeInteger(value)) asked.set(name, String(value));
  }

  const search = asked.size === 0 ? "" : `?${asked}`;

  return await receiverRead<RoomPicturePage>(`/console/rooms/${chatId}/pictures${search}`);
});
