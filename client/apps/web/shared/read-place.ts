import type { ReadPlace } from "./types";

// Places in a room's line, as the unread counts are reckoned from them (docs/web.md, 「Sidebar」),
// compared the same way on the server that keeps the marks and on the screen that shows the counts.

/** Whether one place stands past another in the room's line; nothing stands past no place. */
export function isPast(one: ReadPlace | null, other: ReadPlace | null): boolean {
  if (one === null) return false;
  if (other === null) return true;

  return one.sentAt > other.sentAt || (one.sentAt === other.sentAt && one.order > other.order);
}
