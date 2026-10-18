import type { FallbackView } from "#shared/types";

// What the console may know about the ordered fallback: the owner's word, provider readiness,
// whether it is carrying runs right now, and each stable work key's two choices. The OpenRouter
// secret itself is not in the agency's answer and so cannot be in this one — a key
// that reached the browser would be a key in a page's memory and in whatever cached it.
export default defineEventHandler(async (event): Promise<FallbackView> => {
  await requireUserSession(event);

  return await agencyFetch<FallbackView>("/fallback");
});
