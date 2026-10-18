import type { JudgementPage } from "#shared/types";

// A room's judgements newest first. They are the gate's, which serves the console none yet (docs/boundary.md,
// 「Gaps the console meets」), so the list stands empty.
export default defineEventHandler(async (event): Promise<JudgementPage> => {
  await requireUserSession(event);
  requireChatId(event);

  return readJudgementPage();
});
