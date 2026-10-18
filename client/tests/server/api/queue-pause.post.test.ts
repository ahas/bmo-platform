import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";
import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

import type { QueuePauseChanged } from "../../../apps/web/shared/types";

// What the route refuses before bmo-job is ever called: a request that says neither yes nor no. Everything else
// about the switch — writing it, telling the queue on a resume — is bmo-job's, so what is checked here is that a
// good press reaches it unchanged and that what came of it is said in the pack's words.

const job = standIn("JOB", (one) =>
  answer({ paused: one.body?.paused, deploy_held: true, told: one.body?.paused === true ? null : "nothing_waiting" })
);

const handler = await routeOf<QueuePauseChanged>("../../apps/web/server/api/queue-pause.post");

describe("pausing the auto-progression", () => {
  test("a press reaches bmo-job exactly as it was made, and what came of it is said", async () => {
    job.asked.length = 0;

    expect((await asked(handler, { body: { paused: true } })).answer).toEqual({ ok: true, paused: true, queue: null, deployHold: true });
    expect(job.asked[0]).toMatchObject({ method: "POST", path: "/queue-pause", body: { paused: true } });

    expect((await asked(handler, { body: { paused: false } })).answer).toEqual({
      ok: true,
      paused: false,
      queue: wording("todo.message.queue_nothing_waiting"),
      deployHold: true
    });
    expect(job.asked[1]?.body).toEqual({ paused: false });
  });

  test("a request that switches nothing is refused here, and bmo-job never hears it", async () => {
    job.asked.length = 0;

    expect(await asked(handler, { body: {} })).toMatchObject({ status: 400, message: wording("todo.error.queue_pause_direction") });
    expect(await asked(handler, { body: undefined })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { paused: "true" } })).toMatchObject({ status: 400 });
    expect(job.asked).toEqual([]);
  });
});
