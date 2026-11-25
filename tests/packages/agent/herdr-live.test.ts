import "@agency/test-support/setup";

import { afterEach, describe, expect, test } from "bun:test";

import { activeUnits, installSecondSupervisor } from "@agency/agent/units";

// The picture is what makes liveness answerable without waiting, so what is held here is the
// part that decides wrongly in silence: which event means a run has gone, and what an empty
// answer from a herdr that has just come back is taken to mean.

describe("joining herdr's word to systemd's", () => {
  afterEach(() => {
    installSecondSupervisor(null);
  });

  test("a name only herdr holds is alive, though no unit of that name exists", async () => {
    installSecondSupervisor(() => new Set(["bmo-herdr-only-run"]));

    expect((await activeUnits(["bmo-herdr-only-run"])).has("bmo-herdr-only-run")).toBe(true);
  });

  test("a name neither holds is over", async () => {
    installSecondSupervisor(() => new Set(["bmo-herdr-only-run"]));

    expect((await activeUnits(["bmo-nothing-holds-this"])).size).toBe(0);
  });

  test("only what was asked about comes back, so a picture full of other runs adds nothing", async () => {
    installSecondSupervisor(() => new Set(["bmo-one", "bmo-two", "bmo-three"]));

    expect([...await activeUnits(["bmo-two"])]).toEqual(["bmo-two"]);
  });

  test("with nothing else standing the answer is systemd's alone", async () => {
    installSecondSupervisor(null);

    expect((await activeUnits(["bmo-nothing-holds-this"])).size).toBe(0);
  });

  test("nothing to ask about asks nobody", async () => {
    installSecondSupervisor(() => {
      throw new Error("asked about an empty list");
    });

    expect((await activeUnits([])).size).toBe(0);
  });
});

