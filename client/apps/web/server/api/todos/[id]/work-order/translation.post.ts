import { wording } from "#shared/wording";

// Asking for one of a work order's texts to be translated. The translator is bmo-gate's, which takes such a
// request from the console by no route yet (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<{ ok: true; taken: number }> => {
  await requireUserSession(event);
  requireTodoId(event);

  throw fail(503, wording("todo.error.translation_unavailable"));
});
