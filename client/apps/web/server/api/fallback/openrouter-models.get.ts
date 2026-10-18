import type { OpenRouterModel } from "#shared/types";

// OpenRouter's live catalogue, asked only when the owner opens its chooser. No API key crosses
// this route: the public listing is fetched by the agency and only the model descriptions return.
export default defineEventHandler(async (event): Promise<OpenRouterModel[]> => {
  await requireUserSession(event);

  const answer = await agencyFetch<{ models: OpenRouterModel[] }>("/fallback/openrouter-models", undefined, {
    timeoutMs: LISTING_TIMEOUT_MS
  });

  return answer.models;
});
