import type { RoomRemovalAnswer } from "#shared/types";

// This room's registration removed with its whole record, carried to the receiver under the administrator's name.
// It waits out the room's turns being cut and every holder's part, so it is given long.
const REMOVE_WAIT_MS = 180_000;

// What the receiver answers a removal with; the fields the console's screen does not show stand aside.
interface RemovalRead {
  taken?: RoomRemovalAnswer["taken"];
  cleared?: RoomRemovalAnswer["cleared"];
  pictures?: string[];
  folders?: RoomRemovalAnswer["folders"];
  couldNot?: string[];
}

export default defineEventHandler(async (event): Promise<RoomRemovalAnswer> => {
  const signed = await requireAdministrator(event);
  const chatId = requireChatId(event);
  const answer = (await receiverCall(
    receiver().DELETE("/rooms/{roomId}", {
      params: { path: { roomId: chatId }, query: { by: signed.name } },
      signal: within(REMOVE_WAIT_MS)
    })
  )) as RemovalRead;

  return {
    chat: chatId,
    taken: answer.taken ?? [],
    cleared: answer.cleared ?? [],
    pictures: answer.pictures ?? [],
    settings: [],
    folders: answer.folders ?? [],
    copies: [],
    left: answer.couldNot ?? []
  };
});
