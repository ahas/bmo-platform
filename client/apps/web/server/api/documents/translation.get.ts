import { wording } from "#shared/wording";

import type { DocumentTranslation } from "#shared/types";

// A document's translation. The translator and the translations it keeps are the gate unit's, which serves the
// viewer no route for them yet, so a translation is answered as unavailable (`docs/boundary.md`, 「Gaps the console
// meets」).
export default defineEventHandler(async (event): Promise<DocumentTranslation> => {
  await requireUserSession(event);

  const asked = getQuery(event);

  requireDocument(asked.root, asked.path);

  throw fail(503, wording("documents.error.translation_unavailable"));
});
