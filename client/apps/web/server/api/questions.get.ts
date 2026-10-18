import type { JobQuestion } from "#shared/types";

// What the jobs have stopped to ask the owner. bmo-job reads a room's open questions out without what they
// ask, and every question by no route, so none is listed (docs/boundary.md, 「Gaps the console meets」); the
// chat still carries each question and takes its answer.
export default defineEventHandler(async (event): Promise<JobQuestion[]> => {
  await requireUserSession(event);

  return [];
});
