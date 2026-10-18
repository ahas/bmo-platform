import { wording } from "#shared/wording";

import type { SendOutcomeRecord, SentMessage } from "#shared/types";

// What a send's key came to, for the room screen checking a send that ran late (docs/web.md, the send
// checked after it ran late). It reads the key's record and sends nothing.
export default defineEventHandler(async (event): Promise<SendOutcomeRecord<SentMessage>> => {
  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);

  const key = sendKeyOf(getRouterParam(event, "key"));

  if (key === null) {
    throw fail(400, wording("room.error.bad_send_key"));
  }

  return readSend<SentMessage>(chatId, key);
});
