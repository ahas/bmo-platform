// Which rooms the console stands above the rest, and in what order (docs/web.md, 「Sidebar」). It is one
// setting in the archive holding a list of room numbers: a number keeps its room through a rename,
// and a number the archive no longer has stands for nothing.
export const PINNED_ROOMS_SETTING = "console_pinned_rooms";

// The list as it was stored, out of whatever the setting holds. Anything that is not a list of room
// numbers pins nothing, and a number named twice stands once, where it was first named.
export function readPinnedRooms(stored: string | null): number[] {
  if (stored === null) {
    return [];
  }

  let value: unknown;

  try {
    value = JSON.parse(stored);
  } catch {
    return [];
  }

  if (!Array.isArray(value)) {
    return [];
  }

  return [...new Set(value.filter((one): one is number => Number.isSafeInteger(one)))];
}

// The rooms with the pinned ones first, in the setting's order, and every other room after them in
// the order it already stood in.
export function pinnedFirst<Room extends { chatId: number }>(rooms: readonly Room[], pinned: readonly number[]): Room[] {
  const byId = new Map(rooms.map((room) => [room.chatId, room]));
  const first = pinned.flatMap((chatId) => byId.get(chatId) ?? []);
  const standing = new Set(first.map((room) => room.chatId));

  return [...first, ...rooms.filter((room) => !standing.has(room.chatId))];
}
