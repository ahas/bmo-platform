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

// Launching a runner on the owner's own words, the same way a chat session launches one. bmo-job answers as
// soon as the runner is started and the run's own result goes to the owner's private chat; what the run is
// doing meanwhile is in its log.
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

  const depth = agent === "search" ? body.depth : undefined;

  if (depth !== undefined && !isSearchDepth(depth)) {
    throw fail(400, wording("agents.error.bad_depth"));
  }

  await jobPost("/launch", { agent, text, depth });

  return { ok: true };
});
