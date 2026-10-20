import { wording } from "#shared/wording";

import type { PinnedRooms } from "#shared/types";

// This room pinned or let go, for the administrator alone: the pinned order is one list for every reader, and the
// receiver writes it (its `POST /console/pinned-rooms/add` and `/remove`), answering the order as it now stands.
export default defineEventHandler(async (event): Promise<PinnedRooms> => {
  await requireAdministrator(event);
  const chatId = requireChatId(event);
  const body = await readBody<{ pinned?: unknown }>(event);

  if (typeof body?.pinned !== "boolean") {
    throw fail(400, wording("room.error.pin_direction"));
  }

  const order = body.pinned
    ? await receiverCall(receiver().POST("/pinned-rooms", { body: { room: chatId } }))
    : await receiverCall(receiver().DELETE("/pinned-rooms/{roomId}", { params: { path: { roomId: chatId } } }));

  return { rooms: order.roomIds } satisfies PinnedRooms;
});
