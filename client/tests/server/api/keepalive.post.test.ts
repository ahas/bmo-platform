import { describe, expect, test } from "bun:test";

import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

// The keepalive switched at the cache warmer (the cache warmer's docs/api.md, 「Switch-over」): the round as a
// whole, one agent's subject under the consumer the deployment names, or one worker's pause until its next job.
// What the route refuses is only a request that says nothing — no yes or no, or a name no agent could have —
// since the round trip would answer the same.

process.env.KEEPALIVE_CONSUMER = "agency";

const warmer = standIn("CACHE_WARMER", (call) => answer(call.path === "/round" ? { stopped: call.body?.stopped ?? false } : { subject: "worker-b", switch: null, targets: 0 }));
const handler = await routeOf<{ ok: true }>("../../apps/web/server/api/keepalive.post");

describe("stopping the keepalive", () => {
  test("stops the whole round when no agent is named, and one agent's subject when one is", async () => {
    warmer.asked.length = 0;

    expect(await asked(handler, { body: { stopped: true } })).toMatchObject({ status: 200 });
    expect(warmer.asked[0]).toEqual({ method: "PUT", path: "/round", body: { stopped: true } });

    expect(await asked(handler, { body: { agent: "chat:-1001", stopped: false } })).toMatchObject({ status: 200 });
    expect(warmer.asked[1]).toEqual({ method: "PUT", path: "/consumers/agency/subjects/chat%3A-1001/switch", body: { on: true } });
  });

  test("refuses a request that switches nothing, and a name no agent could have", async () => {
    warmer.asked.length = 0;

    expect(await asked(handler, { body: {} })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { stopped: "true" } })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { agent: "../worker", stopped: true } })).toMatchObject({ status: 400 });
    expect(warmer.asked).toEqual([]);
  });
});

describe("turning a worker's keepalive off until its next job", () => {
  test("pauses the worker's subject and nothing else", async () => {
    warmer.asked.length = 0;

    expect(await asked(handler, { body: { agent: "worker-b", pause: true, stopped: true } })).toMatchObject({ status: 200 });
    expect(warmer.asked).toEqual([{ method: "POST", path: "/consumers/agency/subjects/worker-b/pause", body: null }]);
  });

  test("refuses a pause that names no worker", async () => {
    warmer.asked.length = 0;

    expect(await asked(handler, { body: { pause: true } })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { agent: "../worker-b", pause: true } })).toMatchObject({ status: 400 });
    expect(warmer.asked).toEqual([]);
  });
});
