import type { ControlStatus } from "#shared/types";

// Whether the receiver answers, and which chat is the owner's own. Both come from the receiver's health
// (the receiver's `docs/boundary-api.md`): a screen that knows the rooms are there knows whose room it is
// standing in as well.
export default defineEventHandler(async (event): Promise<ControlStatus> => {
  await requireUserSession(event);

  try {
    const health = await unitFetch<{ ok: boolean; ownerId?: number }>("RECEIVER", "/health");

    return { reachable: health.ok === true, ownerChatId: health.ownerId ?? null };
  } catch (error) {
    return { reachable: false, slow: unitSlow(error), ownerChatId: null };
  }
});
