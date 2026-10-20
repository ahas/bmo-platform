import type { ModelPage } from "#shared/agency";

// The models the registry holds (the agency's docs/api.md, 「providers, models, pin-checks, tiers and
// fallback」): each registration's key, the model behind it, its tier and whether its addresses answer.
export default defineEventHandler(async (event): Promise<ModelPage> => {
  await requireUserSession(event);

  return await agencyFetch<ModelPage>("/models");
});
