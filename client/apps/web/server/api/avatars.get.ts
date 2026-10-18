import type { AvatarRef } from "#shared/types";

// The pictures the console can show, all of them at once. It is a short list — one row per room
// and per person — and it is asked for once a screen rather than once a name.
export default defineEventHandler(async (event): Promise<AvatarRef[]> => {
  await requireUserSession(event);

  return avatarRefs();
});
