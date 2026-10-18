import type { PushState } from "#shared/types";

// What this account can be rung on. The public key is the half a browser needs to subscribe and
// is public by design; the private half never leaves this server.
export default defineEventHandler(async (event): Promise<PushState> => {
  const session = await requireUserSession(event);
  const publicKey = pushPublicKey();

  return {
    configured: publicKey !== null,
    publicKey,
    devices: await devicesOf(session.user.name)
  };
});
