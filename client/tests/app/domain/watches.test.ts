import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";

import { unheldWatches, watchLastCheck, watchOfRun, watchTitle, watchWait } from "../../../apps/web/app/domain/watches";

import type { WatchView } from "../../../apps/web/shared/types";
import { OWNER_ID } from "../../support/ids";
import KO from "../../fixtures/app/domain/watches.test.ko.json";

function watch(overrides: Partial<WatchView> = {}): WatchView {
  return {
    id: 1,
    kind: "condition",
    chatId: OWNER_ID,
    name: null,
    note: KO.line_16,
    condition: KO.line_17,
    dueAt: null,
    deadlineAt: 10_000,
    intervalSeconds: 60,
    lastCheckedAt: null,
    lastExit: null,
    unit: "bmo-watch-1-1",
    memoryId: null,
    ...overrides
  };
}

describe("a watch on the live board", () => {
  test("is joined to the run whose unit it names, and the rest stand apart", () => {
    const held = watch();
    const timer = watch({ id: 2, kind: "timer", condition: null, intervalSeconds: null, dueAt: 9_000, unit: null });
    const between = watch({ id: 3, unit: "bmo-watch-3-1" });
    const runs = [{ unit: "bmo-watch-1-1" }, { unit: null }];

    expect(watchOfRun([held, timer, between], runs[0]!)).toBe(held);
    expect(watchOfRun([held], runs[1]!)).toBeNull();
    expect(unheldWatches([held, timer, between], runs).map((row) => row.id)).toEqual([2, 3]);
  });

  test("is shown by its short name, and by the head of its note where it was given none", () => {
    expect(watchTitle(wording, watch({ name: KO.line_42 }))).toBe(KO.line_42_2);
    expect(watchTitle(wording, watch())).toBe(KO.line_43);
    expect(watchTitle(wording, watch({ note: `${KO.line_44.repeat(10)}` }))).toBe(`${KO.line_44_2}${KO.line_44.repeat(10).trim().slice(0, 39)}…`);
  });

  test("reads its number, its check, its pace and its time left", () => {
    expect(watchWait(wording, watch(), 9_940)).toBe(KO.line_48);
    expect(watchWait(wording, watch(), 10_001)).toContain(KO.line_49);
    expect(watchWait(wording, watch({ kind: "timer", dueAt: 9_400 }), 9_000)).toBe(wording("live.label.timer_left", { time: KO.line_50 }));
  });

  test("says how the last check went, and nothing for a timer", () => {
    expect(watchLastCheck(wording, watch(), 100)).toBe(KO.line_54);
    expect(watchLastCheck(wording, watch({ lastCheckedAt: 70, lastExit: 1 }), 100)).toBe(KO.line_55);
    expect(watchLastCheck(wording, watch({ lastCheckedAt: 70, lastExit: -1 }), 100)).toContain(KO.line_56);
    expect(watchLastCheck(wording, watch({ kind: "timer" }), 100)).toBeNull();
  });
});
