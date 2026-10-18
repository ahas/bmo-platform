import type { CodexModel } from "#shared/types";

// The installed CLI's own catalogue, asked only when the owner opens a Codex chooser. The agency
// unwraps and validates the executable's output; its refusal is said by the pack's sentence for its code.
export default defineEventHandler(async (event): Promise<CodexModel[]> => {
  await requireUserSession(event);

  const answer = await agencyFetch<{ models: CodexModel[] }>("/fallback/codex-models", undefined, {
    timeoutMs: LISTING_TIMEOUT_MS
  });

  return answer.models;
});
