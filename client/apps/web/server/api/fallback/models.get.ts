import type { OpenRouterModel } from "#shared/types";

// Compatibility name for a bookmarked client from before the two provider catalogues were split. New screens ask
// `/fallback/openrouter-models`; this keeps the old read-only URL truthful while every write and every current
// chooser uses the provider's explicit name. The agency serves no provider catalogue, so it stands with none.
export default defineEventHandler(async (event): Promise<OpenRouterModel[]> => {
  await requireUserSession(event);

  return [];
});
