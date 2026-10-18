import { expect, test } from "bun:test";

import { shownRoom, watchReport } from "../../../apps/web/app/domain/watching";

// What a tab tells the server it shows (docs/web.md, 「Notifications」).

test("a room's screen names its room, and every other screen none", () => {
  expect(shownRoom({ name: "rooms-chatId", params: { chatId: "-1003" } })).toBe(-1003);
  expect(shownRoom({ name: "rooms-chatId", params: { chatId: "abc" } })).toBeNull();
  expect(shownRoom({ name: "index", params: {} })).toBeNull();
});

test("a hidden tab keeps its room, and a tab going away names none", () => {
  expect(watchReport("t", -1003, "hidden")).toEqual({ tab: "t", chatId: -1003, visible: false });
  expect(watchReport("t", -1003, "gone")).toEqual({ tab: "t", chatId: null, visible: false });
  expect(watchReport("t", -1003, "visible")).toEqual({ tab: "t", chatId: -1003, visible: true });
});
