import { wording } from "#shared/wording";

import type { CompanionAdded, CompanionView } from "#shared/agency";

// A companion added with a room of the console's own, carried to the receiver for the administrator
// (the agency's docs/companions.md, 「Where a companion and its rooms come from」). The receiver checks every
// field, asks the agency for the agent's half, and answers every companion as it then stands with the room the
// added one answers and whether it took that room back.
const FIELDS = ["name", "label", "provider", "model", "worker", "room"] as const;

// The receiver checks a name against the agency's rows and writes both halves, which takes longer than a save.
const ADD_WAIT_MS = 30_000;

interface Added {
  companions: CompanionView[];
  room: { chatId: number; takenBack: boolean };
}

export default defineEventHandler(async (event): Promise<CompanionAdded> => {
  await requireAdministrator(event);
  const body = await readBody<Record<string, unknown>>(event);

  if (typeof body !== "object" || body === null || FIELDS.some((field) => typeof body[field] !== "string")) throw fail(400, wording("agents.error.companion_add"));

  const added = await companionCall<Added>("/companions/add", Object.fromEntries(FIELDS.map((field) => [field, body[field]])), ADD_WAIT_MS);

  return {
    companions: added.companions,
    room: { ...added.room, said: added.room.takenBack ? wording("agents.message.companion_room_taken_back", { room: String(body.room) }) : null }
  };
});
