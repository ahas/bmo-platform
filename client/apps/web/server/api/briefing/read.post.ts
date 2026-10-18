import { wording } from "#shared/wording";

// A line's full reading, asked for from its chip. The receiver has it made, or answers that it is already made or
// being made; a refusal — the whole briefing switched off — is said by the pack's sentence for its code.
export default defineEventHandler(async (event): Promise<{ ok: true; state: "making" | "ready" }> => {
  await requireUserSession(event);

  const body = await readBody<{ chatId?: unknown; messageId?: unknown }>(event);

  if (!Number.isSafeInteger(body?.chatId) || !Number.isSafeInteger(body?.messageId)) {
    throw fail(400, wording("briefing.error.which_line"));
  }

  return await receiverPost<{ ok: true; state: "making" | "ready" }>("/voice-briefing/line", { chatId: body.chatId, messageId: body.messageId });
});
