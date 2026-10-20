import type { MemoriesView, MemoryRecord } from "#shared/types";

// The memories as a table: every row, the ones that are down and the ones that are finished among them. A screen
// that hid what has come down would lose exactly what the no-delete rule protects, so the list carries them and
// the screen filters. Each row comes with the names it is read by.
export default defineEventHandler(async (event): Promise<MemoriesView> => {
  await requireUserSession(event);

  const chat = getQuery(event).chat;
  const room = typeof chat === "string" && chat.trim() !== "" ? Number(chat.trim()) : null;
  const found: MemoryRecord[] = [];
  let after: number | undefined;

  for (;;) {
    const page = await brainCall(brain().GET("/memories", { params: { query: { limit: 500, ...(after === undefined ? {} : { after }) } } }));

    found.push(...page.items.map(memoryRecordOf));

    if (!page.more || page.items.length === 0) break;

    after = page.items[page.items.length - 1]!.id;
  }

  return { memories: await memoryViews(room === null || Number.isNaN(room) ? found : found.filter((memory) => memory.chatId === room)) };
});
