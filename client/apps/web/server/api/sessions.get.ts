import type { SessionView } from "#shared/agency";

// Every session an agent holds now, as the agency reads them (the agency's `docs/api.md`, 「sessions」).
export default defineEventHandler(async (event): Promise<SessionView[]> => {
  await requireUserSession(event);

  const page = await agencyCall(agency().GET("/sessions"));

  return page.items.map((session) => sessionView(session));
});
