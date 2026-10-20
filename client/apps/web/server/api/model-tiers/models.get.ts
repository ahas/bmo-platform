import { wording } from "#shared/wording";

import type { TierCatalogueEntry } from "#shared/agency";

// One provider's own catalogue of models, as a tier's save is checked against. The agency's fixed API serves a
// provider's registrations and the deployment's catalogue, not the catalogue its provider publishes, so a chooser
// stands with none (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<TierCatalogueEntry[]> => {
  await requireUserSession(event);

  const provider = getQuery(event).provider;

  if (typeof provider !== "string") throw fail(400, wording("agents.error.pick_provider"));

  return [];
});
