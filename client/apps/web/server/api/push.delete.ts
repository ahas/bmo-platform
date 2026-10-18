import { wording } from "#shared/wording";

import type { PushState } from "#shared/types";

// A device saying it no longer wants ringing. Only this account's own rows can go, so one login
// cannot silence another's device.
export default defineEventHandler(async (event): Promise<PushState> => {
  const session = await requireUserSession(event);
  const body = (await readBody(event)) as { endpoint?: unknown };
  const endpoint = typeof body.endpoint === "string" ? body.endpoint.trim() : "";

  if (endpoint === "") {
    throw fail(400, wording("push.error.no_device"));
  }

  await forgetDevice(session.user.name, endpoint);

  return { configured: pushPublicKey() !== null, publicKey: pushPublicKey(), devices: await devicesOf(session.user.name) };
});
