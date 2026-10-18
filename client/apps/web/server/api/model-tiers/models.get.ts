import { wording } from "#shared/wording";

import type { TierCatalogueEntry } from "#shared/agency";
export default defineEventHandler(async (event): Promise<TierCatalogueEntry[]> => {
  await requireUserSession(event);
  const provider = getQuery(event).provider;
  if (typeof provider !== "string") throw fail(400, wording("agents.error.pick_provider"));
  return await agencyFetch<TierCatalogueEntry[]>(`/model-tiers/models?provider=${encodeURIComponent(provider)}`, undefined, { timeoutMs: LISTING_TIMEOUT_MS });
});
