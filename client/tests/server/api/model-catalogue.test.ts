import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";
import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

// The model files as the console asks them of the agency (the agency's docs/api.md, 「providers, models,
// pin-checks, tiers and fallback」): the catalogue at `/model-catalogue`, and one save, removal or choice
// carried there under the signed-in user. The entries are not models and no model route lists them.

const catalogue = { entries: [], uses: [] };

const agency = standIn("AGENCY", () => answer(catalogue));
const calls = agency.asked;

const read = await routeOf<unknown>("../../apps/web/server/api/model-catalogue.get");
const change = await routeOf<unknown>("../../apps/web/server/api/model-catalogue.post");

describe("listing the model files", () => {
  test("reads the agency's catalogue resource and answers it as it came", async () => {
    calls.length = 0;

    expect(await asked(read, {})).toMatchObject({ status: 200, answer: catalogue });
    expect(calls).toEqual([{ method: "GET", path: "/model-catalogue", body: null }]);
  });
});

describe("changing a model file", () => {
  test("carries a save to the catalogue under the signed-in user", async () => {
    calls.length = 0;

    expect(await asked(change, { body: { action: "save", name: "entry-a", file: "model-a.gguf" } })).toMatchObject({ status: 200, answer: catalogue });
    expect(calls).toEqual([{ method: "POST", path: "/model-catalogue", body: { action: "save", name: "entry-a", file: "model-a.gguf", by: "tester" } }]);
  });

  test("carries a removal and a choice by the entry's name", async () => {
    calls.length = 0;

    expect(await asked(change, { body: { action: "remove", name: "entry-a" } })).toMatchObject({ status: 200 });
    expect(await asked(change, { body: { action: "use", name: "entry-a" } })).toMatchObject({ status: 200 });
    expect(calls.map(({ body }) => body)).toEqual([
      { action: "remove", name: "entry-a", by: "tester" },
      { action: "use", name: "entry-a", by: "tester" }
    ]);
  });

  test("refuses a change that names no action without calling the agency", async () => {
    calls.length = 0;

    expect(await asked(change, { body: {} })).toMatchObject({ status: 400, message: wording("agents.error.catalogue_action") });
    expect(await asked(change, { body: { action: 1 } })).toMatchObject({ status: 400 });
    expect(calls).toEqual([]);
  });
});
