import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

import type { JobPrompt } from "#shared/types";

// The prompt of the job a worker's screen keeps its line for, asked when the line is spread: the same job the
// line speaks of, which no unit serves yet (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<{ prompt: JobPrompt | null }> => {
  await requireUserSession(event);

  const name = getRouterParam(event, "name", { decode: true }) ?? "";

  if (!isAgentName(name)) {
    throw fail(400, wording("agents.error.bad_agent_name"));
  }

  return { prompt: null };
});
