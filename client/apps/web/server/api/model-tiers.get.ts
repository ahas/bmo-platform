import type { TierSettingsView } from "#shared/agency";
export default defineEventHandler(async (event): Promise<TierSettingsView> => {
  await requireUserSession(event);
  return await agencyFetch<TierSettingsView>("/model-tiers");
});
