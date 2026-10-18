import type { TodoNote } from "#shared/types";

// What has been added to one item since it was filed, oldest first, as bmo-job's reading of the item carries
// it, so opening the item again brings back every word that landed on it while the work was going on.
export default defineEventHandler(async (event): Promise<TodoNote[]> => {
  await requireUserSession(event);

  return readTodoNotes(requireTodoId(event));
});
