import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";
import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

// What the route does before bmo-job is ever called: it reads a target out of the body, still takes the unit
// the board used to send, and passes the owner's one press for everything through. Which kind is reached by
// what, and every refusal, are bmo-job's own (bmo-job's docs/api.md, 「Stopping runs」), so what is checked here
// is that what a press would send arrives unchanged, that bmo-job's answer reads as the board reads a stop, and
// that its refusal is said by the pack's sentence for its code.

const job = standIn("JOB", (call) => {
  const target = call.body?.target as { kind: string; id: string } | undefined;

  if (target?.id === "gone-unit") return answer({ error: "the target's run has already ended", code: "stop_target_ended" }, 404);
  if (call.body?.all === true) return answer({ code: "all_asked", asked: 1, confirmed: 0, targets: [{ target: { kind: "unit", id: "unit-a" }, state: "asked" }] });

  return answer({ code: "stop_confirmed", target, runs: 1 });
});
const handler = await routeOf<unknown>("../../apps/web/server/api/run-stop.post");

describe("stopping a run from the board", () => {
  test("a target of any kind reaches bmo-job as it was named", async () => {
    job.asked.length = 0;

    for (const target of [
      { kind: "execution", id: "execution-a" },
      { kind: "unit", id: "unit-a" },
      { kind: "item", id: "623" }
    ]) {
      expect(await asked(handler, { body: { target } })).toMatchObject({ status: 200, answer: { ok: true, target, state: "confirmed", runs: 1 } });
    }

    expect(job.asked.map((one) => one.path)).toEqual(["/run/stop", "/run/stop", "/run/stop"]);
    expect(job.asked.map((one) => one.body)).toEqual([
      { target: { kind: "execution", id: "execution-a" } },
      { target: { kind: "unit", id: "unit-a" } },
      { target: { kind: "item", id: "623" } }
    ]);
    // The owner's own reason rides along where one was given; an item is what carries it.
    expect(await asked(handler, { body: { target: { kind: "item", id: "623" }, why: "the direction was wrong" } })).toMatchObject({ status: 200 });
    expect(job.asked[3]?.body).toEqual({ target: { kind: "item", id: "623" }, why: "the direction was wrong" });
  });

  test("the unit body the board used to send is read as a unit target", async () => {
    job.asked.length = 0;

    expect(await asked(handler, { body: { unit: "  unit-a  " } })).toMatchObject({ status: 200 });
    expect(job.asked[0]?.body).toEqual({ target: { kind: "unit", id: "unit-a" } });
  });

  test("one press stops everything, and says what it asked", async () => {
    job.asked.length = 0;

    expect(await asked(handler, { body: { all: true } })).toMatchObject({
      status: 200,
      answer: { ok: true, stopped: [{ target: { kind: "unit", id: "unit-a" }, state: "asked", runs: 1 }], refused: [] }
    });
    expect(job.asked[0]?.body).toEqual({ all: true });
  });

  test("a body naming nothing is refused here, and bmo-job never hears it", async () => {
    job.asked.length = 0;

    expect(await asked(handler, { body: {} })).toMatchObject({ status: 400, message: wording("live.error.stop_target_needed") });
    expect(await asked(handler, { body: undefined })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { unit: "   " } })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { target: { kind: "runner", id: "unit-a" } } })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { target: { kind: "unit", id: "" } } })).toMatchObject({ status: 400 });
    expect(job.asked).toEqual([]);
  });

  test("bmo-job's refusal is said by the pack's sentence for its code", async () => {
    expect(await asked(handler, { body: { target: { kind: "unit", id: "gone-unit" } } })).toMatchObject({ status: 404, message: wording("live.error.stop_target_ended") });
  });
});
