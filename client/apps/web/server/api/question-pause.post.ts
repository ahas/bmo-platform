import { wording } from "#shared/wording";

import type { QuestionPauseResult } from "#shared/types";

interface PauseBody {
  id?: unknown;
  paused?: unknown;
}

// What bmo-job answers a stop or a start of the clock with: its code, what the wait had left while stopped and
// when the question now settles, in milliseconds.
interface PauseAnswer {
  code: string;
  paused_left?: number | null;
  due_at?: number | null;
}

// Stopping a question's clock while he reads it, or starting it again. The row is bmo-job's to write, as the
// answer is: a stop of a clock already stopped, or a start of one running, changes nothing and says so.
export default defineEventHandler(async (event): Promise<QuestionPauseResult> => {
  await requireUserSession(event);

  const body = await readBody<PauseBody>(event);
  const id = body?.id;

  if (typeof id !== "number" || !Number.isInteger(id)) {
    throw fail(400, wording("question.error.bad_question_id"));
  }

  if (typeof body?.paused !== "boolean") {
    throw fail(400, wording("question.error.pause_direction"));
  }

  const answer = await jobPost<PauseAnswer>("/question-pause", { id, paused: body.paused });

  if (answer.code === "question_paused_already") throw fail(409, wording("question.error.already_paused"));
  if (answer.code === "question_not_paused") throw fail(409, wording("question.error.already_running"));

  return {
    ok: true,
    pausedLeft: typeof answer.paused_left === "number" ? Math.floor(answer.paused_left / 1000) : null,
    dueAt: typeof answer.due_at === "number" ? Math.floor(answer.due_at / 1000) : 0
  };
});
