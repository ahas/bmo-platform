import type { BasePromptsView } from "#shared/agency";

// The base prompts the agency keeps and each agent's choice of them (the agency's docs/base-prompts.md).
export default defineEventHandler(async (event): Promise<BasePromptsView> => {
  await requireUserSession(event);

  return await agencyFetch<BasePromptsView>("/base-prompts");
});
