import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";
import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

// What the route does before the agency is ever called: it reads a target out of the body, still takes the unit
// the board used to send, and passes the owner's one press for everything through. A job is stopped by its item's
// job, and a run by its own close (the agency's `docs/api.md`, 「assignments and jobs」), so what is checked here is
// that what a press would send arrives at the right route, and that the agency's refusal is said by the pack's
// sentence for its code.

function jobOf(id: number, itemId: number): Record<string, unknown> {
  return {
    id,
    itemId,
    leaseId: 1,
    holderName: "owner",
    assignmentId: 1,
    workerName: "worker-a",
    sessionId: null,
    runId: id,
    runName: `run-${id}`,
    state: "running",
    outcome: null,
    report: null,
    tokenCount: 0,
    startedAt: 0,
    endedAt: null
  };
}

let running = [jobOf(9, 623)];
let refuse = false;

const agency = standIn("AGENCY", (call) => {
  if (one(call.path) === "/jobs") return answer({ items: running, more: false });
  if (refuse) return answer({ code: "not-found", error: "no such job" }, 404);
  if (call.method === "PATCH" && one(call.path).startsWith("/jobs/")) return answer(jobOf(9, 623));

  return answer({});
});

function one(path: string): string {
  return path.split("?")[0] ?? path;
}

const handler = await routeOf<unknown>("../../apps/web/server/api/run-stop.post");

describe("stopping a run from the board", () => {
  test("an item's job is found and stopped, and the target answers as it was named", async () => {
    agency.asked.length = 0;

    expect(await asked(handler, { body: { target: { kind: "item", id: "623" } } })).toMatchObject({
      status: 200,
      answer: { ok: true, target: { kind: "item", id: "623" }, state: "asked", runs: 1 }
    });
    expect(agency.asked.map((call) => `${call.method} ${one(call.path)}`)).toEqual(["GET /jobs", "PATCH /jobs/9"]);
    expect(agency.asked[1]?.body).toMatchObject({ state: "stopped", outcome: "stopped" });
  });

  test("a run of any other kind is closed by its number", async () => {
    agency.asked.length = 0;

    for (const kind of ["execution", "unit"]) {
      expect(await asked(handler, { body: { target: { kind, id: "12" } } })).toMatchObject({
        status: 200,
        answer: { ok: true, target: { kind, id: "12" }, state: "asked", runs: 1 }
      });
    }

    expect(agency.asked.map((call) => `${call.method} ${one(call.path)}`)).toEqual(["PATCH /runs/12", "PATCH /runs/12"]);
  });

  test("the unit body the board used to send is read as a unit target", async () => {
    agency.asked.length = 0;

    expect(await asked(handler, { body: { unit: "  12  " } })).toMatchObject({ status: 200, answer: { target: { kind: "unit", id: "12" } } });
    expect(one(agency.asked[0]?.path ?? "")).toBe("/runs/12");
  });

  test("one press stops everything, and says what it asked", async () => {
    agency.asked.length = 0;

    expect(await asked(handler, { body: { all: true } })).toMatchObject({
      status: 200,
      answer: { ok: true, stopped: [{ target: { kind: "item", id: "623" }, state: "asked", runs: 1 }], refused: [] }
    });
    expect(agency.asked[0]?.path).toBe("/jobs?state=running%2Cstarting");
    expect(agency.asked[1]?.path).toBe("/jobs/9");
  });

  test("a body naming nothing is refused here, and the agency never hears it", async () => {
    agency.asked.length = 0;

    expect(await asked(handler, { body: {} })).toMatchObject({ status: 400, message: wording("live.error.stop_target_needed") });
    expect(await asked(handler, { body: undefined })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { unit: "   " } })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { target: { kind: "runner", id: "unit-a" } } })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { target: { kind: "unit", id: "" } } })).toMatchObject({ status: 400 });
    expect(await asked(handler, { body: { target: { kind: "unit", id: "unit-a" } } })).toMatchObject({ status: 400 });
    expect(agency.asked).toEqual([]);
  });

  test("the agency's refusal is said by the pack's sentence for its code", async () => {
    refuse = true;

    try {
      expect(await asked(handler, { body: { target: { kind: "item", id: "623" } } })).toMatchObject({ status: 404, message: wording("agents.error.agency_not_found") });
    } finally {
      refuse = false;
      running = [jobOf(9, 623)];
    }
  });
});
