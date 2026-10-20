import { signOut } from "../../utils/account-service";

// The seal goes, and the sumida session it held goes with it on the account service.
export default defineEventHandler(async (event): Promise<{ ok: true }> => {
  const session = await getUserSession(event);

  if (session.sid !== undefined) {
    forgetSession(session.sid);
    await signOut(session.sid, browserAddress(event)).catch(() => undefined);
  }

  await clearUserSession(event);

  return { ok: true };
});
