import type { CodexModel } from "#shared/types";

// The installed CLI's own catalogue, asked only when the owner opens a Codex chooser. The agency's fixed API serves
// no route for a provider's own catalogue, so a chooser stands with none and no executable is asked anything
// (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<CodexModel[]> => {
  await requireUserSession(event);

  return [];
});
