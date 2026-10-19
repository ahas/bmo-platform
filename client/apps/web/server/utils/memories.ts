import { agencyRead } from "./agency";
import { listRooms, readOneMessage } from "./archive";
import { receiverPost } from "./receiver";

import type { AgentSummary, MemoryRecord, MemoryRowView, MemorySourceView } from "#shared/types";

// The memories as the memories screen shows them: the rows the brain answers (the brain's `docs/boundary-api.md`,
// 「Memory rows」), each with what its agent is called, the name its room is known by, the names of the person it
// is for and of who set it, and the message it was set from. The brain answers the rows alone; the agent's label
// is the agency's to say, and the room, the people and the message the receiver's. A unit that does not answer
// leaves the row standing by its numbers. The watch serving a row is served by no route, so none stands beside
// it (docs/boundary.md, 「Gaps the console meets」).

// The names the given rooms are known by, as the room list names them; a room the list does not hold stands as
// its number.
async function roomNames(chatIds: readonly number[]): Promise<Map<number, string>> {
  if (chatIds.length === 0) return new Map();

  const rooms = await listRooms().catch(() => []);
  const known = new Map(rooms.map((room) => [room.chatId, room.title]));

  return new Map(chatIds.map((chatId) => [chatId, known.get(chatId) ?? String(chatId)]));
}

// What the owner calls each agent, as the agency labels it, and its own name where it gives none.
async function agentLabels(): Promise<Map<string, string>> {
  try {
    const { agents } = await agencyRead<{ agents?: AgentSummary[] }>("/agents");

    return new Map((agents ?? []).map((agent) => [agent.name, agent.label ?? agent.name]));
  } catch {
    return new Map();
  }
}

// The name each given person last spoke under, as the receiver reads it (`POST /people/names`).
async function personNames(people: readonly number[]): Promise<Map<number, string>> {
  if (people.length === 0) return new Map();

  try {
    const { names } = await receiverPost<{ names: { person: number; name: string | null }[] }>("/people/names", { people });

    return new Map(names.flatMap((one) => (one.name === null ? [] : [[Number(one.person), one.name] as const])));
  } catch {
    return new Map();
  }
}

// The message each row was set from, looked up in the row's own room, since a message's number is the room's
// own. A global row keeps no room, so its message cannot be told from another room's.
async function sourceMessages(records: readonly MemoryRecord[]): Promise<Map<number, MemorySourceView>> {
  const found = await Promise.all(
    records.map(async (record): Promise<[number, MemorySourceView] | null> => {
      if (record.chatId === null || record.messageId === null) return null;

      const message = (await readOneMessage(record.chatId, record.messageId).catch(() => null))?.messages[0];

      return message === undefined ? null : [record.id, { senderName: message.senderName, sentAt: message.sentAt, text: message.text }];
    })
  );

  return new Map(found.filter((one) => one !== null));
}

export async function memoryViews(records: readonly MemoryRecord[]): Promise<MemoryRowView[]> {
  const distinct = (values: (number | null)[]): number[] => [...new Set(values.filter((value): value is number => value !== null))];
  const [rooms, people, agents, messages] = await Promise.all([
    roomNames(distinct(records.map((record) => record.chatId))),
    personNames(distinct(records.flatMap((record) => [record.forUser, record.setBy]))),
    agentLabels(),
    sourceMessages(records)
  ]);
  const nameOf = (userId: number | null): string | null => (userId === null ? null : (people.get(userId) ?? String(userId)));

  return records.map((record) => ({
    ...record,
    agentLabel: agents.get(record.agent) ?? record.agent,
    room: record.chatId === null ? null : (rooms.get(record.chatId) ?? String(record.chatId)),
    forName: nameOf(record.forUser),
    setByName: nameOf(record.setBy),
    message: messages.get(record.id) ?? null,
    watch: null
  }));
}
