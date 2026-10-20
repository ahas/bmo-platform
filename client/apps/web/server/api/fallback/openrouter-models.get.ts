import type { OpenRouterModel } from "#shared/types";

// OpenRouter's live catalogue, asked only when the owner opens its chooser. The agency's fixed API serves no route
// for a provider's own catalogue, so a chooser stands with none (docs/boundary.md, 「Gaps the console meets」). No
// API key crosses this route either way.
export default defineEventHandler(async (event): Promise<OpenRouterModel[]> => {
  await requireUserSession(event);

  return [];
});
