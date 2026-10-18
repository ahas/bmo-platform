import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

import type { AgentLogPage } from "#shared/types";

// One page of an agent's own log: the page one named run begins at, and the lines of that run after an id, as
// the agency's run log holds them. Every other page — the newest, the ones before an id, the page standing at
// a moment — is the log across the agent's runs, which no unit serves yet, and answers as unavailable. The log
// holds the rooms' conversations and whatever a browse task was handed, so it stands behind the same session
// rule as the rest of the app and goes nowhere else.
export default defineEventHandler(async (event): Promise<AgentLogPage> => {
  await requireUserSession(event);

  const name = getRouterParam(event, "name", { decode: true }) ?? "";

  if (!isAgentName(name)) {
    throw fail(400, wording("agents.error.bad_agent_name"));
  }

  const query = getQuery(event);
  const run = typeof query.run === "string" ? query.run : "";

  return run === "" ? UNAVAILABLE : await readRunPage(name, run, readCursor(query.after) ?? 0, readLimit(query.limit));
});
