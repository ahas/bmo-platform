import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";
import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

// The console carries a change to bmo-artifact and names nothing else: an unknown action stops here, and
// bmo-artifact's refusal comes back said by the pack's sentence for its code, with its status.

const artifact = standIn("ARTIFACT", (call) =>
  call.body?.capacity === 999 ? answer({ error: "The capacity is not a whole number from 0 to 128.", code: "bad_capacity" }, 400) : answer({ ok: true })
);
const handler = await routeOf<{ ok: true }>("../../apps/web/server/api/research-settings.post");

describe("changing the pool and the research profiles", () => {
  test("carries each known action to bmo-artifact's one path", async () => {
    artifact.asked.length = 0;

    for (const action of ["set-pool-limit", "reset-pool-limits", "set-profile", "reset-profile"]) {
      expect(await asked(handler, { body: { action } })).toMatchObject({ status: 200 });
    }

    expect(artifact.asked.map(({ path }) => path)).toEqual(Array(4).fill("/research-settings"));
    expect(artifact.asked.map(({ body }) => body?.action)).toEqual(["set-pool-limit", "reset-pool-limits", "set-profile", "reset-profile"]);
  });

  test("refuses an unknown action locally and says bmo-artifact's refusal by its code", async () => {
    artifact.asked.length = 0;

    for (const body of [{ action: "set-mapping" }, { action: "set-role-enabled" }, { action: "" }, {}, null, "set-profile"]) {
      expect(await asked(handler, { body })).toMatchObject({ status: 400 });
    }

    expect(artifact.asked).toHaveLength(0);
    expect(await asked(handler, { body: { action: "set-pool-limit", capacity: 999 } })).toMatchObject({ status: 400, message: wording("research.error.bad_capacity") });
  });
});
