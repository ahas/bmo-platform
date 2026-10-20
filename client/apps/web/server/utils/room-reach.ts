import { checkRoom, userOf } from "./account-service";

import { signedInUser } from "./accounts";
import { receiver, receiverCall } from "./receiver";

import type { H3Event } from "h3";

// The rooms any of the senders has stood as a sender in, as the receiver's room list names each room's senders
// (the receiver's `docs/api.md`, 「The rooms」).
async function roomsSpokenIn(senders: readonly number[]): Promise<number[]> {
  const page = await receiverCall(receiver().GET("/rooms"));
  const ours = new Set(senders);

  return page.items.filter((room) => room.senders.some((sender) => ours.has(sender))).map((room) => room.id);
}

// The rooms a signed-in user reads on the console (docs/web.md, 「Access」): every
// room for the administrator; for anyone else the rooms they are a member of — where one of their
// identities has a message, and their own private rooms — in which they hold the console grant. A
// room outside them answers as if it did not stand.

/** Every room, or the ones a user may read. */
export type ReadableRooms = "all" | ReadonlySet<number>;

export async function readableRooms(event: H3Event): Promise<ReadableRooms> {
  const held = event.context.readableRooms as ReadableRooms | undefined;

  if (held !== undefined) return held;

  const signed = await signedInUser(event);
  let rooms: ReadableRooms = new Set<number>();

  if (signed.administrator) {
    rooms = "all";
  } else if (signed.id !== null) {
    const senders = [...((await userOf(signed.id))?.senders ?? [])].sort((one, other) => one - other);

    if (senders.length > 0) {
      const spoken = await roomsSpokenIn(senders);
      const readable = new Set<number>();

      for (const room of new Set([...spoken, ...senders])) {
        if ((await checkRoom(room, senders[0]!, "console")).allowed) readable.add(room);
      }

      rooms = readable;
    }
  }

  event.context.readableRooms = rooms;

  return rooms;
}

/** A room the signed-in user may read, or a refusal answering as if it did not stand. */
export async function requireReadableRoom(event: H3Event, chatId: number): Promise<void> {
  const rooms = await readableRooms(event);

  if (rooms !== "all" && !rooms.has(chatId)) {
    throw createError({ statusCode: 404, statusMessage: "Not Found" });
  }
}
