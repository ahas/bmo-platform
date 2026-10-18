import { wording } from "#shared/wording";

import type { CatalogueView } from "#shared/agency";

// One change of the model catalogue, or a choice of an entry for its engine, carried to the agency under
// the signed-in user (the agency's docs/model-catalogue.md). A choice asks the engine's health and stands a unit up
// before it answers, so it is given longer than a save.
export default defineEventHandler(async (event): Promise<CatalogueView> => {
  const session = await requireUserSession(event);
  const body = await readBody<Record<string, unknown>>(event);

  if (typeof body !== "object" || body === null || typeof body.action !== "string") throw fail(400, wording("agents.error.catalogue_action"));

  const { by: _ignored, ...change } = body;

  return await agencyChange<CatalogueView>("/models", change, session.user.name, body.action === "use" ? 20_000 : 8_000);
});
