import type { CatalogueView } from "#shared/agency";

// The model files the deployment's own engines serve, and the latest choice of one for each engine
// (the agency's docs/model-catalogue.md).
export default defineEventHandler(async (event): Promise<CatalogueView> => {
  await requireUserSession(event);

  return await agencyFetch<CatalogueView>("/models");
});
