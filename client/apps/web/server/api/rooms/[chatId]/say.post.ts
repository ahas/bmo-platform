import { CONTROL_SLOW, MAX_SAY_TEXT_LENGTH } from "#shared/config";
import { wording } from "#shared/wording";

import type { SentMessage } from "#shared/types";

interface SayBody {
  text?: unknown;
  // The number of the message this answers, when it is a reply.
  replyTo?: unknown;
  // The key the screen put on this send, the same on every try of it (docs/web.md, the room screen's section on a send going once).
  key?: unknown;
}

// What the owner types here is said in the room under their own name, through the receiver (the receiver's
// `docs/boundary-api.md`, `POST /say`). From there it is an ordinary message: the receiver keeps it and the room's
// agent answers it.
export default defineEventHandler(async (event): Promise<SentMessage> => {
  await requireUserSession(event);

  const chatId = requireChatId(event);
  const body = await readBody<SayBody>(event);
  const text = typeof body?.text === "string" ? body.text.trim() : "";

  if (text === "") {
    throw fail(400, wording("room.error.say_empty"));
  }

  if (text.length > MAX_SAY_TEXT_LENGTH) {
    throw fail(400, wording("common.error.too_long", { n: MAX_SAY_TEXT_LENGTH.toLocaleString("ko-KR") }));
  }

  const replyTo = replyToOf(body?.replyTo);

  // The receiver's refusal of a reply — a message it cannot find — is said by the pack's sentence for its code. A
  // send the receiver has not answered in the screen's time comes back as the 504 of a unit held up, and the screen
  // checks it rather than calling it failed, while the send is still listened for and what it came to is written
  // under its key; a try of a send still going under its key says the same.
  return sendOnce<SentMessage>(
    chatId,
    sendKeyOf(body?.key),
    async () => {
      const said = await receiverPost<{ ok: true; messageId: number }>(
        "/say",
        { chatId, text, replyTo: replyTo ?? undefined },
        { timeoutMs: SEND_TIMEOUT_MS + SEND_LATE_MS }
      );

      return { messageId: said.messageId };
    },
    () => {
      throw fail(504, wording(CONTROL_SLOW));
    },
    sendKeysRoot(),
    SEND_TIMEOUT_MS
  );
});
