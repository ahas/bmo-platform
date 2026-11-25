// Which room a tab shows, as it tells the server (docs/web.md, 「Notifications」): a room's
// notification is not sent to an account while one of its visible tabs shows that room.

export interface WatchReport {
  tab: string;
  chatId: number | null;
  visible: boolean;
}

// The room a route stands for, or null for any screen that is not one room's.
export function shownRoom(route: { name?: unknown; params: Record<string, unknown> }): number | null {
  if (route.name !== "rooms-chatId") {
    return null;
  }

  const chatId = Number(route.params.chatId);

  return Number.isSafeInteger(chatId) ? chatId : null;
}

// What a tab says as it stands now. A hidden tab still names its room, so a hidden one that comes
// back is the same tab; a tab going away names nothing.
export function watchReport(tab: string, chatId: number | null, state: "visible" | "hidden" | "gone"): WatchReport {
  return { tab, chatId: state === "gone" ? null : chatId, visible: state === "visible" };
}
