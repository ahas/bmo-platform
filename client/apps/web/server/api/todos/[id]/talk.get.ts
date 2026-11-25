import type { TodoTalk } from "#shared/types";

// Where the chat agent stands on the talk about one item. The unit that kept an item's talk is bmo-job, whose fixed
// API keeps no talk of an item; the receiver's talks answer one turn and keep none. So the talk stands with no turns
// and nothing answering (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<TodoTalk> => {
  await requireUserSession(event);
  requireTodoId(event);

  return { turns: [], answering: false, failure: null };
});
