import { PUSH_UNCONFIGURED } from "#shared/config";
import { ownerRoomPath } from "#shared/paths";
import { wording } from "#shared/wording";

// Rings this account's devices once, so that a notification arriving on this device can be seen
// rather than assumed. It is a test of its own rather than one of the kinds, and every device of
// the account hears it whatever it has switched off: what is asked is whether a notification
// arrives at all.
export default defineEventHandler(async (event): Promise<{ sent: number; gone: number }> => {
  if (pushPublicKey() === null) {
    throw fail(503, wording(PUSH_UNCONFIGURED));
  }

  const session = await requireUserSession(event);

  return await ringAccount(session.user.name, {
    kind: "test",
    title: wording("app.label.name"),
    body: wording("push.message.test_body"),
    url: ownerRoomPath(await ownerChat()),
    tag: "push-test"
  });
});
