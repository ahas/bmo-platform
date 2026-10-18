import { wording } from "#shared/wording";

import type { RegistryView } from "#shared/agency";

// One change of the registered models, carried to the agency, which checks it and writes it. A key the owner
// gives travels in this body once, to the agency, which writes it into a file of its own; it is never logged
// here or sent back. A refusal comes back with the field it is about, so the box can show it under that field.
export default defineEventHandler(async (event): Promise<RegistryView> => {
  await requireUserSession(event);

  const body = await readBody<Record<string, unknown>>(event);

  if (typeof body !== "object" || body === null || typeof body.action !== "string") throw fail(400, wording("agents.error.registry_action"));

  return await agencyPost<RegistryView>("/registry", body);
});
