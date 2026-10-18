import { describe, expect, test } from "bun:test";

import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

// The console's own road for a word said to a worker. It launches nothing and names no model: the words and the
// worker are the whole of what travels, and bmo-job is what hands them over.

const globals = globalThis as unknown as Record<string, unknown>;

globals.getRequestHeader = () => undefined;

const job = standIn("JOB", () => new Response(null, { status: 204 }));

const handler = await routeOf<{ ok: true }>("../../apps/web/server/api/worker-word.post");

describe("a word from the console to one worker", () => {
  test("carries the worker and the words, and nothing that would launch anything", async () => {
    job.asked.length = 0;

    expect((await asked(handler, { body: { worker: "worker-b", text: "  stop what you are doing and report  " } })).answer).toEqual({ ok: true });
    expect(job.asked).toMatchObject([{ method: "POST", path: "/worker-word", body: { worker: "worker-b", text: "stop what you are doing and report" } }]);
    // No model and no agent: a word is not a job and opens none.
    expect(Object.keys(job.asked[0]?.body ?? {}).sort()).toEqual(["text", "worker"]);
  });

  test("refuses a word with no worker to say it to, and words that are not there", async () => {
    job.asked.length = 0;

    expect(await asked(handler, { body: { text: "where to?" } })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { worker: "worker-b", text: "   " } })).toMatchObject({ status: 400 });
    expect(job.asked).toEqual([]);
  });

  test("refuses a word longer than one request may carry", async () => {
    job.asked.length = 0;

    expect(await asked(handler, { body: { worker: "worker-b", text: "a".repeat(1_000_001) } })).toMatchObject({ status: 400 });
    expect(job.asked).toEqual([]);
  });
});
