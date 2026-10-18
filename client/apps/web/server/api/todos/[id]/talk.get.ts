import { wording } from "#shared/wording";

import type { TodoTalk } from "#shared/types";

// Why a turn came back with nothing, by bmo-job's kind for it (bmo-job's `docs/api.md`, 「An unanswered turn」),
// each said in the pack's words; a kind it does not name here is said as any other.
const UNANSWERED: Readonly<Record<string, string>> = {
  resting: "todo.message.talk_resting",
  limit: "todo.message.talk_limit",
  outage: "todo.message.talk_outage",
  silent: "todo.message.talk_silent",
  other: "todo.message.talk_other"
};

// Where the chat agent stands on the talk about one item, as bmo-job holds it: whether a turn is being
// answered, and why the last one came back with nothing. bmo-job keeps the turns but reads them out by no
// route, so the talk stands with none (docs/boundary.md, 「Gaps the console meets」); a bmo-job that cannot be
// asked is answering nothing.
export default defineEventHandler(async (event): Promise<TodoTalk> => {
  await requireUserSession(event);

  const id = requireTodoId(event);

  try {
    const state = await jobRead<{ answering: boolean; failure?: string | null }>(`/todo-talk?id=${id}`);
    const kind = state.failure ?? null;

    return { turns: [], answering: state.answering, failure: kind === null ? null : wording(UNANSWERED[kind] ?? "todo.message.talk_other") };
  } catch {
    return { turns: [], answering: false, failure: null };
  }
});
