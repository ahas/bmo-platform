import type { OwnerChat } from "#shared/types";

// Whose chat is the owner's own, as the receiver names it (the receiver's `docs/boundary-api.md`, `GET /owner`), and
// whether the one signed in is the administrator, for what a room offers them alone. A receiver that is not reached
// names none.
export default defineEventHandler(async (event): Promise<OwnerChat> => {
  const signed = await signedInUser(event);

  return { chatId: await ownerChat(), administrator: signed.administrator };
});
