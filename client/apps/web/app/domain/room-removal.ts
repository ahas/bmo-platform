import { toUnixSeconds } from "bmo-ui/app/domain/time";

import type { BotEvent, RoomRemovalCopy, RoomRemovalFolder, RoomRemovalPreview, RoomRemovalRows } from "#shared/types";

// What the registration removal dialog shows of bmo-server's preview (docs/web.md, the room screen's
// registration removal): what goes, group by group, what stays, and whether the room can be removed
// at all; and which room a removal event closes. Which tables, folders and warnings there are is
// bmo-server's to say, read off its own declarations; nothing here names one.

/** One group's rows: their sum, and each table with rows in it, the largest first. */
export interface RowsTally {
  total: number;
  tables: RoomRemovalRows[];
}

/** The preview as the dialog lists it. */
export interface RemovalGroups {
  // The reason the room is not removed, or null where it can be.
  refused: string | null;
  goes: RowsTally;
  pictures: number;
  settings: string[];
  // The data root's folders of the room, with how many files each holds.
  folders: RoomRemovalFolder[];
  // The brain's folders of the room, each with its documents by name.
  documents: RoomRemovalFolder[];
  copies: RoomRemovalCopy[];
  stays: RowsTally;
  warnings: string[];
  // The settings naming the room that no declaration matches, which stay.
  undeclared: string[];
}

export function rowsTally(rows: readonly RoomRemovalRows[]): RowsTally {
  const tables = rows.filter((one) => one.rows > 0).toSorted((a, b) => b.rows - a.rows || a.table.localeCompare(b.table));

  return { total: tables.reduce((sum, one) => sum + one.rows, 0), tables };
}

export function removalGroups(preview: RoomRemovalPreview): RemovalGroups {
  return {
    refused: preview.refused,
    goes: rowsTally(preview.goes),
    pictures: preview.pictures.length,
    settings: preview.settings.taken,
    folders: preview.folders.filter((folder) => folder.documents === undefined),
    documents: preview.folders.filter((folder) => folder.documents !== undefined),
    copies: preview.copies,
    stays: rowsTally(preview.stays),
    warnings: preview.warnings,
    undeclared: preview.settings.undeclared,
  };
}

/** Whether the dialog offers the confirm: only for a room no declaration refuses. */
export function removable(preview: RoomRemovalPreview | null): boolean {
  return preview !== null && preview.refused === null;
}

// When this tab opened, in the seconds a bot event is stamped in.
const TAB_OPENED = Math.floor(performance.timeOrigin / 1000);

/**
 * The room a `room-removed` event closes, or null. A tab's first stream is handed what the bot's ring
 * still holds, so a removal from before the tab opened — already in what it read, and perhaps of a
 * room registered again since — closes nothing.
 */
export function removedRoom(event: BotEvent, openedAt = TAB_OPENED): number | null {
  return event.type === "room-removed" && toUnixSeconds(event.at) >= openedAt ? event.chatId : null;
}
