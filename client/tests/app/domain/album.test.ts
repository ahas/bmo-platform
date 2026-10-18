import { describe, expect, test } from "bun:test";

import { albumLayout, albumRows, rowSizes } from "../../../apps/web/app/domain/album";

// The mosaic an album stands as: rows split by how many pictures there are, each filling the width.
describe("an album's mosaic", () => {
  test("splits its rows so none is left half full", () => {
    expect(rowSizes(2)).toEqual([2]);
    expect(rowSizes(3)).toEqual([1, 2]);
    expect(rowSizes(4)).toEqual([1, 3]);
    expect(rowSizes(5)).toEqual([2, 3]);
    expect(rowSizes(6)).toEqual([3, 3]);
    expect(rowSizes(7)).toEqual([2, 2, 3]);
    expect(rowSizes(10)).toEqual([2, 2, 3, 3]);

    for (const count of [2, 3, 4, 5, 6, 7, 8, 9, 10]) {
      expect(rowSizes(count).reduce((total, size) => total + size, 0)).toBe(count);
    }
  });

  test("stands each row at the height its pictures fill the width at, within what reads as a row", () => {
    // A wide and a tall picture side by side come to their shapes' sum; a picture whose shape is not
    // known yet is a square.
    expect(albumRows([1.5, 0.5]).map((row) => row.aspect)).toEqual([2]);
    expect(albumRows([null, null, null, null, null]).map((row) => row.aspect)).toEqual([2, 3]);
    // One tall picture alone in its row stands no taller than the bubble is wide.
    expect(albumRows([0.5, 1, 1])[0]!.aspect).toBe(1);
  });

  test("stands three or four whose first is not wide as one large beside the rest, the rest over one width", () => {
    const side = albumLayout([0.8, 1, 1, 1]);

    expect(side.kind).toBe("side");

    if (side.kind === "side") {
      expect(side.main).toBe(0.8);
      expect(side.column).toBeCloseTo(1 / 3, 5);
      expect(side.aspect).toBeCloseTo(0.8 + 1 / 3, 5);
      expect(side.rest.map((one) => one.picture)).toEqual([1, 2, 3]);
    }

    expect(albumLayout([1.8, 1, 1, 1]).kind).toBe("rows");
    expect(albumLayout([0.8, 1]).kind).toBe("rows");
  });
});
