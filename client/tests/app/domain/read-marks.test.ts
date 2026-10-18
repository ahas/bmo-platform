import { expect, test } from "bun:test";

import { readThrough } from "../../../apps/web/app/domain/read-marks";

// How far a room is read once a row is seen (docs/web.md, 「Sidebar」).

const line = [10, 11, 12, 13, 14].map((messageId) => ({ messageId }));

test("a seen row reads on over what stands as no row of its own, up to the next row", () => {
  // 11 is a picture of 10's album and 14 the voice of 13's line.
  const rows = new Set([10, 12, 13]);

  expect(readThrough(line, rows, 10)).toBe(11);
  expect(readThrough(line, rows, 12)).toBe(12);
  expect(readThrough(line, rows, 13)).toBe(14);
  expect(readThrough(line, rows, 99)).toBeNull();
});
