import type { ResearchSettingsView } from "#shared/research";

// The pool, the research profiles and the engines as bmo-artifact answers them (bmo-artifact's `docs/api.md`,
// 「Research settings」); the console keeps no copy.
export default defineEventHandler(async (event): Promise<ResearchSettingsView> => {
  await requireUserSession(event);

  return await researchRead();
});
