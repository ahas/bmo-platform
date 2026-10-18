import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

import type { JobLine } from "#shared/types";

// The job a worker's screen keeps a line for at its foot: the run the screen was opened at where that is one
// of this worker's jobs, otherwise the one it is running, otherwise its last. No unit serves a worker's job
// with what it spent, so there is none to show (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<{ job: JobLine | null }> => {
  await requireUserSession(event);

  const name = getRouterParam(event, "name", { decode: true }) ?? "";

  if (!isAgentName(name)) {
    throw fail(400, wording("agents.error.bad_agent_name"));
  }

  return { job: null };
});
