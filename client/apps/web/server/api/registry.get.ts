import type { RegistryView } from "#shared/agency";

// The registered models and the gateway's state, as the agency reads them (the agency's docs/own-llm.md,
// 「Registered models」). No key crosses this line: each address says only whether its key is usable.
export default defineEventHandler(async (event): Promise<RegistryView> => {
  await requireUserSession(event);

  return await agencyFetch<RegistryView>("/registry");
});
