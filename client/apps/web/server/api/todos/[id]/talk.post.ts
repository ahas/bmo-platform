import { TALK_LIMIT } from "#shared/config";
import { wording } from "#shared/wording";

interface TalkBody {
  text?: unknown;
}

// One turn of the talk about an item. The unit that kept an item's talk is bmo-job, whose fixed API keeps no talk
// of an item; the receiver's talks answer one turn and keep none, so a turn cannot be written down and answered as
// the screen reads it. The words are refused as unavailable (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<{ ok: true }> => {
  await requireUserSession(event);

  requireTodoId(event);
  const body = await readBody<TalkBody>(event);
  const text = typeof body?.text === "string" ? body.text.trim() : "";

  if (text === "") {
    throw fail(400, wording("common.error.say_to_bmo_empty"));
  }

  if (text.length > TALK_LIMIT) {
    throw fail(400, wording("common.error.too_long", { n: TALK_LIMIT }));
  }

  throw fail(503, wording("todo.error.talk_unavailable"));
});
