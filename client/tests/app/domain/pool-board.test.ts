import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";

import { bucketHead, tileCaption, tileLines, tileStop } from "../../../apps/web/app/domain/pool-board";

import type { PoolBoardBucket, PoolBoardTile } from "../../../apps/web/shared/agency";
import KO from "../../fixtures/app/domain/pool-board.test.ko.json";

const working: PoolBoardTile = {
  state: "working",
  work: {
    kind: "cowork", role: "scout", tier: "Light", provider: "claude-code", model: "claude-haiku-4-5-20251001", effort: "low",
    title: KO.line_13, detail: KO.line_13_2, todoId: 471, since: 1_000, log: { agent: "work-order-471-stage-0-0-1", run: "bmo-work-471-stage-0-0-1" },
    // A co-worker has no stop of its own: the tile names the run it works inside.
    stop: { kind: "unit", id: "bmo-work-471-stage-0-0-1" }
  }
};

describe("the pool's tiles", () => {
  test("a row's head names where it is, how many places are taken and how many wait", () => {
    const bucket: PoolBoardBucket = { provider: "claude-code", tier: "Light", capacity: 12, tiles: [working, { state: "held", work: null }, { state: "idle", work: null }], waiting: [working.work!] };

    expect(bucketHead(wording, bucket)).toBe(KO.line_23);
    expect(bucketHead(wording, { ...bucket, waiting: [] })).toBe("Claude Code · Light 2/12");
  });

  test("a tile's hover says its state and what it is assigned to; an idle one says only that it rests", () => {
    const lines = tileLines(wording, working, 66_000);

    expect(lines[0]).toBe(KO.line_30);
    expect(lines).toContain(KO.line_31);
    expect(lines).toContain(KO.line_32);
    expect(lines).toContain("scout · Light");
    expect(lines.at(-1)).toBe(KO.line_34);
    expect(tileLines(wording, { state: "idle", work: null }, 0)).toEqual([KO.line_35]);
    expect(tileCaption(working.work!)).toBe(KO.line_36);
  });

  // A co-worker is not stopped on its own account, so a tile carries the target of the run it
  // works inside; a place with nobody in it carries none.
  test("a tile names the stop of the run it works inside, and an empty place names none", () => {
    expect(tileStop(working)).toEqual({ kind: "unit", id: "bmo-work-471-stage-0-0-1" });
    expect(tileStop({ state: "idle", work: null })).toBeNull();
    expect(tileStop({ state: "held", work: { ...working.work!, stop: null } })).toBeNull();
  });
});
