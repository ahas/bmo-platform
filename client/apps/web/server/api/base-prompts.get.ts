import type { BasePromptsView } from "#shared/agency";

// The base prompts the agency deploys and each agent's choice of them (the agency's `docs/api.md`,
// 「prompt-documents」).
export default defineEventHandler(async (event): Promise<BasePromptsView> => {
  await requireUserSession(event);

  const page = await agencyCall(agency().GET("/prompt-documents", { params: { query: { kind: "base" } } }));

  return basePromptsOf(page.items);
});
