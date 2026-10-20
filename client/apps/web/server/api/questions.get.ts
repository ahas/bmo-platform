import type { JobQuestion } from "#shared/types";

// What the jobs have stopped to ask the owner, as bmo-job's fixed API reads them (bmo-job's `docs/api.md`,
// 「Questions」): an open question's words and the answers to choose from. Which one the job would take by itself
// and why is no part of a question, so it stands as the first answer (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<JobQuestion[]> => {
  await requireUserSession(event);

  const page = await jobCall(job().GET("/questions", { params: { query: { state: "open", limit: 500 } } }));

  return page.items.map((question) => ({
    id: question.id,
    todoId: question.itemId ?? 0,
    question: question.text,
    options: question.choices,
    recommended: 0,
    why: "",
    askedAt: Math.floor(question.createdAt / 1000),
    dueAt: question.dueAt === null ? 0 : Math.floor(question.dueAt / 1000),
    chosen: null,
    answer: null,
    pausedLeft: null,
    settledAt: null,
    settledBy: null
  }));
});
