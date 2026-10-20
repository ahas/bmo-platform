import type { TierSettingsView } from "#shared/agency";

// The tiers and the providers that carry them, as the agency reads them (the agency's `docs/api.md`, 「providers,
// models, pin-checks, tiers and fallback」).
export default defineEventHandler(async (event): Promise<TierSettingsView> => {
  await requireUserSession(event);

  const [tiers, providers] = await Promise.all([
    agencyCall(agency().GET("/tiers")),
    agencyCall(agency().GET("/providers"))
  ]);

  return tierSettingsOf(tiers.items, providers.items);
});
