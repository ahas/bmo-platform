import type { MemoriesView, MemoryRecord } from "#shared/types";

// The memories as a table: every row, the ones that are down and the ones that are finished among them. A screen
// that hid what has come down would lose exactly what the no-delete rule protects, so the list carries them and
// the screen filters. Each row comes with the names it is read by.
export default defineEventHandler(async (event): Promise<MemoriesView> => {
  await requireUserSession(event);

  const chat = getQuery(event).chat;
  const where = typeof chat === "string" && chat.trim() !== "" ? `?chat=${encodeURIComponent(chat.trim())}` : "";

  const { memories } = await brainRead<{ memories: MemoryRecord[] }>(`/memories${where}`);

  return { memories: await memoryViews(memories) };
});
