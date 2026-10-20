import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";
import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

// The console's own road for a word said to a worker. It launches nothing: the words and the worker are the whole
// of what is asked. The units' fixed APIs send a turn into a session but start none for a worker from a screen's
// words, so the road answers as unavailable until that route stands (docs/boundary.md, 「Gaps the console meets」).

const agency = standIn("AGENCY", () => answer({ code: "not-found", error: "no route" }, 404));

const handler = await routeOf<{ ok: true }>("../../apps/web/server/api/worker-word.post");

describe("a word from the console to one worker", () => {
  test("is refused as unavailable, asking no unit", async () => {
    agency.asked.length = 0;

    expect(await asked(handler, { body: { worker: "worker-b", text: "  stop what you are doing and report  " } })).toMatchObject({
      status: 503,
      message: wording("agents.error.word_unavailable")
    });
    expect(agency.asked).toEqual([]);
  });

  test("refuses a word with no worker to say it to, and words that are not there", async () => {
    agency.asked.length = 0;

    expect(await asked(handler, { body: { text: "where to?" } })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { worker: "worker-b", text: "   " } })).toMatchObject({ status: 400 });
    expect(agency.asked).toEqual([]);
  });

  test("refuses a word longer than one request may carry", async () => {
    agency.asked.length = 0;

    expect(await asked(handler, { body: { worker: "worker-b", text: "a".repeat(1_000_001) } })).toMatchObject({ status: 400 });
    expect(agency.asked).toEqual([]);
  });
});
