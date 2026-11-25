import type { TodoItem } from "#shared/types";

// How many numbers one ask may carry. A screen asks for the numbers it shows, which is a handful.
const ITEMS_LIMIT = 50;

// The numbers asked for, as `?ids=1,2,3`; anything that is not a whole number is passed over.
function idsOf(value: unknown): number[] {
  const raw = typeof value === "string" ? value.split(",") : [];

  return raw.map(Number).filter((id) => Number.isSafeInteger(id) && id > 0).slice(0, ITEMS_LIMIT);
}

// Several items, each whole, for the screens that show an item by its number alone and name it on
// hover (docs/web.md, 「An item by its number」). A number with no item behind it is not answered.
export default defineEventHandler(async (event): Promise<TodoItem[]> => {
  await requireUserSession(event);

  return readTodoItems(idsOf(getQuery(event).ids));
});
