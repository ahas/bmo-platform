import { wording } from "#shared/wording";

import type { WorkTextTranslation } from "#shared/types";

// One text of a work order as its translation stands. The translator and the translations it keeps are
// bmo-gate's, which serves them to the console by no route yet, so the text reads as it was written
// (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<WorkTextTranslation | null> => {
  await requireUserSession(event);
  requireTodoId(event);

  throw fail(503, wording("todo.error.translation_unavailable"));
});
