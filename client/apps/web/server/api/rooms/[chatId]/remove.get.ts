import type { RoomRemovalPreview } from "#shared/types";

// What removing this room's registration would take and leave, read by the receiver with nothing written, for the
// administrator alone. It asks every holder and counts the room's files, which takes seconds and more under load.
const PREVIEW_WAIT_MS = 60_000;

// The answer the receiver gives a removal, preview or carried out; the fields the console's screen does not show
// stand aside.
interface RemovalRead {
  goes?: RoomRemovalPreview["goes"];
  stays?: RoomRemovalPreview["stays"];
  pictures?: string[];
  settings?: RoomRemovalPreview["settings"];
  folders?: RoomRemovalPreview["folders"];
  warned?: string[];
}

export default defineEventHandler(async (event): Promise<RoomRemovalPreview> => {
  const signed = await requireAdministrator(event);
  const chatId = requireChatId(event);
  const answer = (await receiverCall(
    receiver().DELETE("/rooms/{roomId}", {
      params: { path: { roomId: chatId }, query: { by: signed.name, dryRun: true } },
      signal: within(PREVIEW_WAIT_MS)
    })
  )) as RemovalRead;

  return {
    chat: chatId,
    refused: null,
    warnings: answer.warned ?? [],
    goes: answer.goes ?? [],
    stays: answer.stays ?? [],
    pictures: answer.pictures ?? [],
    settings: answer.settings ?? { taken: [], undeclared: [] },
    folders: answer.folders ?? [],
    copies: []
  };
});
