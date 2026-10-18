import type { OpenRouterModel } from "#shared/types";

// Compatibility name for a bookmarked client from before the two provider catalogues were split.
// New screens ask `/fallback/openrouter-models`; this keeps the old read-only URL truthful while
// every write and every current chooser uses the provider's explicit name.
export default defineEventHandler(async (event): Promise<OpenRouterModel[]> => {
  await requireUserSession(event);

  const answer = await agencyFetch<{ models: OpenRouterModel[] }>("/fallback/openrouter-models", undefined, {
    timeoutMs: LISTING_TIMEOUT_MS
  });

  return answer.models;
});
