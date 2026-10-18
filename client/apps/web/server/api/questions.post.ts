import { ANSWER_LIMIT, questionSettledNotice } from "#shared/config";
import { wording } from "#shared/wording";

import type { QuestionAnswerResult, QuestionSettler } from "#shared/types";

interface AnswerBody {
  id?: unknown;
  chosen?: unknown;
  answer?: unknown;
}

// What bmo-job answers an answer with: its code, and what the question was settled on where it was settled
// before this answer landed.
interface QuestionAnswer {
  code: string;
  settled_by?: QuestionSettler | null;
  answer?: string | null;
}

// Answering a question a job put to the owner. The row is bmo-job's to write, and its own conditional write
// decides it: an answer that arrives after the chat's or the deadline's changes nothing and comes back saying
// what the question was settled on instead.
export default defineEventHandler(async (event): Promise<QuestionAnswerResult> => {
  await requireUserSession(event);

  const body = await readBody<AnswerBody>(event);
  const id = body?.id;
  const chosen = body?.chosen;
  const answer = body?.answer;

  if (typeof id !== "number" || !Number.isInteger(id)) {
    throw fail(400, wording("question.error.bad_question_id"));
  }

  // One or the other: a body carrying both says two things, and neither of them is his answer.
  if ((chosen === undefined) === (answer === undefined)) {
    throw fail(400, wording("question.error.one_answer"));
  }

  if (chosen !== undefined && (typeof chosen !== "number" || !Number.isInteger(chosen) || chosen < 0)) {
    throw fail(400, wording("question.error.bad_option"));
  }

  if (answer !== undefined && (typeof answer !== "string" || answer.trim() === "" || answer.length > ANSWER_LIMIT)) {
    throw fail(400, wording("question.error.bad_own_answer"));
  }

  const settled = await jobPost<QuestionAnswer>("/question", { id, chosen, answer });

  if (settled.code === "question_settled_already") {
    throw fail(409, questionSettledNotice(wording, settled.settled_by ?? null, settled.answer ?? ""));
  }

  return { ok: true };
});
