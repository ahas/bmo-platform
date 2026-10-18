import { describe, expect, test } from "bun:test";

import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

import type { AgentList } from "../../../apps/web/shared/types";

// The catalogue as the agency answers it, with each agent's keepalive as the cache warmer holds it laid over its
// row. An agency that does not answer leaves the list empty and says so. The gate order is the gate unit's, which
// is not split, so none stands beside the list.

process.env.KEEPALIVE_CONSUMER = "agency";

let agencyAnswers = true;

standIn("AGENCY", () =>
  agencyAnswers
    ? answer({ agents: [{ name: "agent-a", kind: "chat", keepaliveStopped: false, keepalivePausedAt: null, lastKeepaliveAt: null }], roles: [{ name: "builder", label: "Builder", bundles: [] }] })
    : answer({ code: "service-unavailable", error: "the store is not open" }, 503)
);
standIn("CACHE_WARMER", (call) =>
  call.path.startsWith("/consumers/agency/subjects")
    ? answer({ subjects: [{ subject: "agent-a", switch: "off", paused_at: 7_000, targets: 1 }] })
    : answer({ targets: [{ subject: "agent-a", last_warmed_at: 5_500 }] })
);

const handler = await routeOf<AgentList>("../../apps/web/server/api/agents.get");

describe("the agent list", () => {
  test("is the agency's, with the warmer's keepalive laid over each row", async () => {
    agencyAnswers = true;

    expect((await asked(handler, {})).answer).toEqual({
      reachable: true,
      agents: [{ name: "agent-a", kind: "chat", keepaliveStopped: true, keepalivePausedAt: 7, lastKeepaliveAt: 5 }],
      roles: [{ name: "builder", label: "Builder", bundles: [] }],
      gateOrder: null
    } as unknown as AgentList);
  });

  test("stands empty, and says so, while the agency does not answer", async () => {
    agencyAnswers = false;

    expect((await asked(handler, {})).answer).toEqual({ reachable: false, slow: false, agents: [], roles: [], gateOrder: null });
  });
});
