import { describe, expect, test } from "bun:test";

import "../../support/nitro";
import { answer, standIn } from "../../support/units";
import { scratchWebDatabase } from "../../support/web-database";

// How far an account has read a room: the marks are the console's own table, counted against a stand-in
// receiver's line. What is pinned here is that a room met for the first time is read as it stands, that what lands
// past the mark is counted from its first, that the account's own word reads the room up to it, and that a mark
// only moves forward.

await scratchWebDatabase();

const OWN = 42;
let line: { messageId: number; senderId: number | null; sentAt: number }[] = [{ messageId: 1, senderId: 7, sentAt: 100 }];

const after = (id: number) => line.slice(line.findIndex((one) => one.messageId === id) + 1);

standIn("RECEIVER", (one) => {
  const url = new URL(one.path, "http://receiver");
  const path = url.pathname;

  if (path === "/console/rooms/-7/messages") return answer({ messages: line.slice(-1), hasMore: false });
  if (path === "/console/rooms/-7/messages/after") return answer({ messages: after(Number(url.searchParams.get("after"))), hasMore: false });

  const single = /^\/console\/rooms\/-7\/messages\/(-?\d+)$/.exec(path);

  if (single !== null) return answer({ messages: line.filter((one) => one.messageId === Number(single[1])), hasMore: false });

  return answer({ code: "no-route", error: "no route" }, 404);
});

const { markRead, unreadCounts } = await import("../../../apps/web/server/utils/room-reads");

const signed = { name: "member-a", id: null, administrator: true };
const room = () => [{ chatId: -7, lastMessageAt: line.at(-1)!.sentAt }];

describe("what a room holds unread", () => {
  test("a room met for the first time is read as it stands", async () => {
    expect((await unreadCounts(signed, [OWN], room(), 1_000)).get(-7)).toEqual({ count: 0, first: null, from: { sentAt: 100, order: 0 } });
  });

  test("what lands past the mark is counted from the first of it", async () => {
    line = [...line, { messageId: 2, senderId: 7, sentAt: 110 }, { messageId: 3, senderId: 8, sentAt: 120 }];

    expect((await unreadCounts(signed, [OWN], room(), 1_000)).get(-7)).toEqual({ count: 2, first: 2, from: { sentAt: 100, order: 0 } });
  });

  test("the account's own word reads the room up to it", async () => {
    line = [...line, { messageId: 4, senderId: OWN, sentAt: 130 }, { messageId: 5, senderId: 7, sentAt: 140 }];

    expect((await unreadCounts(signed, [OWN], room(), 1_000)).get(-7)).toEqual({ count: 1, first: 5, from: { sentAt: 130, order: 0 } });
  });

  test("a room read up to a message counts only past it, and a mark behind it moves nothing", async () => {
    expect(await markRead(signed, [OWN], -7, 5, 1_000)).toEqual({ count: 0, first: null, from: { sentAt: 140, order: 0 } });
    expect(await markRead(signed, [OWN], -7, 2, 1_001)).toEqual({ count: 0, first: null, from: { sentAt: 140, order: 0 } });
  });
});
