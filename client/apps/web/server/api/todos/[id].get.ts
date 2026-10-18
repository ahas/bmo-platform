import type { TodoItem } from "#shared/types";

// One item, whole, for a link to a row the panel's board does not carry — a finished item further
// back than the page in hand.
export default defineEventHandler(async (event): Promise<TodoItem> => {
  await requireUserSession(event);

  const item = await readTodoItem(requireTodoId(event));

  if (item === null) {
    throw createError({ statusCode: 404, statusMessage: "no such item" });
  }

  return item;
});
