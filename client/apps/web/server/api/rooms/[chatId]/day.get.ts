import type { RoomDayStart } from "#shared/types";

// Where a day of a room begins, for the room's date picker. The day is the owner's own: it starts
// at midnight in Seoul, as every date on this console does.
export default defineEventHandler(async (event): Promise<RoomDayStart> => {
  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);

  const date = getQuery(event).date;

  if (typeof date !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(date)) {
    throw createError({ statusCode: 400, statusMessage: "date must be YYYY-MM-DD" });
  }

  const from = Math.floor(Date.parse(`${date}T00:00:00+09:00`) / 1000);

  if (!Number.isFinite(from)) {
    throw createError({ statusCode: 400, statusMessage: "date must be a real day" });
  }

  return { start: await readDayStart(chatId, from) };
});
