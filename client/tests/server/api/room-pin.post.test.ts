import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";
import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

import type { PinnedRooms } from "../../../apps/web/shared/types";

// A room pinned or let go: the route hands the receiver the room on the road that says which, answers the order as
// the receiver now holds it, and says a refusal of the receiver's by the pack's sentence for its code.

let order: number[] = [5];

const receiver = standIn("RECEIVER", (one) => {
  const chat = one.body?.chat as number;

  if (chat === -404) return answer({ code: "not-found", error: "The record holds no such room." }, 404);
  if (one.path === "/console/pinned-rooms/add") order = order.includes(chat) ? order : [...order, chat];
  if (one.path === "/console/pinned-rooms/remove") order = order.filter((room) => room !== chat);

  return answer({ rooms: order });
});

const handler = await routeOf<PinnedRooms>("../../apps/web/server/api/rooms/[chatId]/pin.post");

describe("pinning a room", () => {
  test("adds the room to the receiver's order, last, and takes it out again", async () => {
    receiver.asked.length = 0;

    expect((await asked(handler, { params: { chatId: "-7" }, body: { pinned: true } })).answer).toEqual({ rooms: [5, -7] });
    expect((await asked(handler, { params: { chatId: "5" }, body: { pinned: false } })).answer).toEqual({ rooms: [-7] });
    expect(receiver.asked.map((one) => [one.method, one.path, one.body])).toEqual([
      ["POST", "/console/pinned-rooms/add", { chat: -7 }],
      ["POST", "/console/pinned-rooms/remove", { chat: 5 }]
    ]);
  });

  test("refuses a request that says neither, here and in words, and never reaches the receiver", async () => {
    receiver.asked.length = 0;

    for (const body of [{}, { pinned: "yes" }, null]) {
      expect(await asked(handler, { params: { chatId: "-7" }, body })).toMatchObject({ status: 400, message: wording("room.error.pin_direction") });
    }

    expect(receiver.asked).toEqual([]);
  });

  test("says the receiver's refusal by the pack's sentence for its code", async () => {
    expect(await asked(handler, { params: { chatId: "-404" }, body: { pinned: true } })).toMatchObject({ status: 404, message: wording("room.error.receiver_not_found") });
  });
});
