import { describe, expect, test } from "bun:test";

import { GATE_ORDER } from "../../support/gate-order";
import { packEntries } from "../../../apps/web/shared/wording";

import { GATE_CONDITION_NOTES, GATE_PART_LABELS, gateOrderKeys, gatePlacements } from "../../../apps/web/app/domain/gate-order";
import KO from "../../fixtures/app/domain/gate-order.test.ko.json";

// An order as the gate unit serves one.
const order = GATE_ORDER;
const pack = new Map(packEntries());

describe("where each gate stands", () => {
  test("is every place of the gate order, part by part and step by step", () => {
    const placements = gatePlacements(order);

    expect(placements.map((one) => `${one.part}:${one.step ?? "-"}:${one.key}`)).toEqual(
      order.flatMap((part) =>
        part.steps.flatMap((step, index) => step.places.map((place) => `${part.part}:${part.inTurn ? index + 1 : "-"}:${place.key}`))
      )
    );
  });

  test("numbers the steps within each turn part, and none outside a turn", () => {
    const placements = gatePlacements(order);

    for (const part of ["pre", "post"] as const) {
      const steps = placements.filter((one) => one.part === part).map((one) => one.step);

      expect(steps[0]).toBe(1);
      expect(steps.every((step) => step !== null)).toBe(true);
    }

    expect(placements.filter((one) => one.part === "apart").every((one) => one.step === null)).toBe(true);
  });

  test("marks the gates of one step as starting together, and gives them one number", () => {
    const together = gatePlacements(order).filter((one) => one.together);
    const step = order.find((part) => part.steps.some((one) => one.places.length > 1))!;
    const shared = step.steps.find((one) => one.places.length > 1)!;

    expect(together.map((one) => one.key)).toEqual(shared.places.map((place) => place.key));
    expect(new Set(together.map((one) => one.step)).size).toBe(1);
  });

  test("stands a gate that runs in both turn parts in each", () => {
    const placements = gatePlacements(order);
    const twice = [...new Set(placements.map((one) => one.key))].filter((key) => placements.filter((one) => one.key === key).length > 1);

    for (const key of twice) {
      expect(placements.filter((one) => one.key === key).map((one) => one.part)).toEqual(["pre", "post"]);
    }
  });

  test("gives each gate once in the order where it first stands", () => {
    const keys = gateOrderKeys(order);

    expect(new Set(keys).size).toBe(keys.length);
    expect(keys[0]).toBe(gatePlacements(order)[0]?.key);
  });

  test("is nothing at all before the order has been read", () => {
    expect(gatePlacements([])).toEqual([]);
    expect(gateOrderKeys([])).toEqual([]);
  });
});

describe("the words each part and condition is shown in", () => {
  test("head the two turn parts with the owner's own names", () => {
    expect(pack.get(GATE_PART_LABELS.pre)).toBe(KO.line_69);
    expect(pack.get(GATE_PART_LABELS.post)).toBe(KO.line_70);
    expect(pack.get(GATE_PART_LABELS.apart)).toBe(KO.line_71);
  });

  test("stand in the language pack for every part and every condition the order can carry", () => {
    for (const key of [...Object.values(GATE_PART_LABELS), ...Object.values(GATE_CONDITION_NOTES)]) {
      expect(pack.get(key)).toBeString();
    }

    for (const placement of gatePlacements(order).filter((one) => one.condition !== null)) {
      expect(pack.get(GATE_CONDITION_NOTES[placement.condition!])).toBeString();
    }
  });
});
