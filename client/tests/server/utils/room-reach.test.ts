import { afterAll, describe, expect, test } from "bun:test";

import { standInAccountService } from "../../support/account-service";
import { ROOM_ID } from "../../support/ids";

// The rooms a user other than the administrator reads on the console (docs/web.md, 「Access」): those they are a
// member of, where one of their senders has stood as a sender, and hold the console grant in, and nothing
// else. The account service answers the user's senders and the grant; the receiver answers the rooms with the
// senders that have stood in each.

const member = 77;

const service = standInAccountService({
  users: [{ id: 2, name: "member", senders: [member] }],
  grants: [
    { object: String(ROOM_ID), relation: "console", subject: "user:2" },
    { object: String(member), relation: "console", subject: "user:2" }
  ]
});

// The receiver's room titles: the member has spoken in the room and in -300, someone else in -200, and the
// member's own private room is theirs.
const receiver = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  fetch: (request) =>
    new URL(request.url).pathname === "/rooms/titles" && request.method === "POST"
      ? Response.json({
          rooms: [
            { chatId: ROOM_ID, title: "a room", senders: [member] },
            { chatId: -300, title: "another room", senders: [member] },
            { chatId: -200, title: "a third room", senders: [5] },
            { chatId: member, title: null, senders: [member] }
          ]
        })
      : Response.json({ error: "no such route", code: "not-found" }, { status: 404 })
});

process.env.WEB_SERVER_ADDRESS = service.address;
process.env.WEB_SERVER_TOKEN = service.token;
process.env.RECEIVER_ADDRESS = `http://127.0.0.1:${receiver.port}`;

const globals = globalThis as unknown as Record<string, unknown>;
let session: Record<string, unknown> = {};

globals.requireUserSession = async () => session;
globals.createError = (error: { statusCode: number; statusMessage?: string; message?: string }) =>
  Object.assign(new Error(error.statusMessage ?? error.message), error);
globals.fail = (statusCode: number, message: string) => Object.assign(new Error(message), { statusCode });

const { readableRooms, requireReadableRoom } = await import("../../../apps/web/server/utils/room-reach");

afterAll(async () => {
  service.stop();
  await receiver.stop(true);
});

const event = () => ({ context: {} }) as never;

describe("the rooms a user reads on the console", () => {
  test("are the rooms they are a member of and hold the console grant in", async () => {
    session = { user: { name: "member", id: 2 }, sid: "bmo.sid=member", administrator: false };

    expect([...(await readableRooms(event())) as Set<number>].sort((a, b) => a - b)).toEqual([ROOM_ID, member]);

    for (const absent of [-200, -300]) {
      expect(await requireReadableRoom(event(), absent).then(() => 200, (error: { statusCode: number }) => error.statusCode)).toBe(404);
    }
  });

  test("are every room for the administrator", async () => {
    session = { user: { name: "boss", id: 1 }, sid: "bmo.sid=boss", administrator: true };

    expect(await readableRooms(event())).toBe("all");
  });
});
