import { describe, expect, test } from "bun:test";

import { pinnedFirst, readPinnedRooms } from "../../apps/web/shared/pinned-rooms";
import KO from "../fixtures/shared/pinned-rooms.test.ko.json";

const rooms = [
  { chatId: 1, title: KO.line_6 },
  { chatId: 2, title: KO.line_7 },
  { chatId: 3, title: KO.line_8 },
  { chatId: 4, title: "Test Lab" },
  { chatId: 5, title: KO.line_10 }
];

describe("the rooms pinned over the rest", () => {
  test("stand first in the setting's order, and every other room keeps the order it stood in", () => {
    expect(pinnedFirst(rooms, [4, 5, 3]).map((room) => room.chatId)).toEqual([4, 5, 3, 1, 2]);
  });

  test("a number the archive no longer has stands for nothing", () => {
    expect(pinnedFirst(rooms, [9, 2]).map((room) => room.chatId)).toEqual([2, 1, 3, 4, 5]);
  });

  test("the setting pins only a list of room numbers, each once, where it was first named", () => {
    expect(readPinnedRooms("[4, 5, 3, 4]")).toEqual([4, 5, 3]);
    expect(readPinnedRooms('[4, "5", 1.5, null]')).toEqual([4]);
    expect(readPinnedRooms('{"4": true}')).toEqual([]);
    expect(readPinnedRooms("not json")).toEqual([]);
    expect(readPinnedRooms(null)).toEqual([]);
  });
});
