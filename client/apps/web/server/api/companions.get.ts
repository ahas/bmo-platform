import type { CompanionView } from "#shared/agency";

// Every companion as the agency lists it (the agency's `docs/api.md`, 「companions」): its name, the label it is
// called by, and what answers for it. The settings beside it are read by no route of the agency, so they stand
// empty (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<CompanionView[]> => {
  await requireUserSession(event);

  const page = await agencyCall(agency().GET("/companions"));

  return page.items.map(companionViewOf);
});
