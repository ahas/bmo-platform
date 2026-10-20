import { emptyFallbackView } from "../../app/domain/agent-settings";

import type { FallbackProfile, FallbackView } from "#shared/types";

// What the console may know about the ordered fallback: the fallback's roads and whether it is answering. The
// provider readiness, the account list and the stand-in's own picture are read by no route of the agency's fixed
// API, so they stand empty (docs/boundary.md, 「Gaps the console meets」). The OpenRouter secret is not in the
// agency's answer and so cannot be in this one — a key that reached the browser would be a key in a page's memory
// and in whatever cached it.
export default defineEventHandler(async (event): Promise<FallbackView> => {
  await requireUserSession(event);

  const fallback = await agencyCall(agency().GET("/fallback"));

  return { ...emptyFallbackView(), profiles: fallback.routes as FallbackProfile[] };
});
