import type { TodoApproval } from "#shared/types";

// Every approval of one item and every approval taken back, newest first. bmo-job keeps them but reads them
// out by no route, so the item stands with none listed (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<TodoApproval[]> => {
  await requireUserSession(event);
  requireTodoId(event);

  return [];
});
