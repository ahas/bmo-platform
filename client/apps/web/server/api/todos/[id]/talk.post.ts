import { TALK_LIMIT } from "#shared/config";
import { wording } from "#shared/wording";

interface TalkBody {
  text?: unknown;
}

// One turn of the talk about an item. bmo-job writes the words down at once and hands the turn to the chat
// agent, which answers when its place in line comes — so this only carries the words over, and the answer
// arrives as news of the talk moving rather than as this request's reply.
export default defineEventHandler(async (event): Promise<{ ok: true }> => {
  await requireUserSession(event);

  const id = requireTodoId(event);
  const body = await readBody<TalkBody>(event);
  const text = typeof body?.text === "string" ? body.text.trim() : "";

  if (text === "") {
    throw fail(400, wording("common.error.say_to_bmo_empty"));
  }

  if (text.length > TALK_LIMIT) {
    throw fail(400, wording("common.error.too_long", { n: TALK_LIMIT }));
  }

  // A second word while the first is still being answered is refused by its code.
  await jobPost("/todo-talk", { id, text });

  return { ok: true };
});
