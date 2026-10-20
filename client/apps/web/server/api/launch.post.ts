import { isSearchDepth, MAX_RUN_PROMPT_LENGTH } from "#shared/config";
import { wording } from "#shared/wording";

import type { LaunchResult } from "#shared/types";

interface LaunchBody {
  agent?: unknown;
  text?: unknown;
  depth?: unknown;
}

// The runners this road launches. A worker is not among them: what is typed on a worker's screen
// is said to that worker's own session and launches nothing (`worker-word.post.ts`).
const RUNNERS = ["search", "browse"];

// Launching a runner on the owner's own words. The units' fixed APIs serve no route that launches a free runner
// any more — the agency's jobs run an item on its worker, and bmo-job launches nothing — so a launch is refused
// as unavailable (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<LaunchResult> => {
  await requireUserSession(event);

  const body = (await readBody<LaunchBody>(event)) ?? {};
  const agent = body.agent;
  const text = typeof body.text === "string" ? body.text.trim() : "";

  if (typeof agent !== "string" || !RUNNERS.includes(agent)) {
    throw fail(400, wording("agents.error.bad_launch_agent"));
  }

  if (text === "") {
    throw fail(400, wording("agents.error.launch_empty"));
  }

  if (text.length > MAX_RUN_PROMPT_LENGTH) {
    throw fail(400, wording("common.error.too_long", { n: MAX_RUN_PROMPT_LENGTH }));
  }

  if (agent === "search" && body.depth !== undefined && !isSearchDepth(body.depth)) {
    throw fail(400, wording("agents.error.bad_depth"));
  }

  throw fail(503, wording("agents.error.launch_failed"));
});
