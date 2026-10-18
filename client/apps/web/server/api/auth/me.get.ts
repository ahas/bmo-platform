import type { AccountUser } from "#shared/types";

export default defineEventHandler(async (event): Promise<{ user: AccountUser }> => {
  const session = await requireUserSession(event);

  return { user: { name: session.user.name } };
});
