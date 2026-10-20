import { agency, agencyCall } from "./agency";
import { listRooms, readOneMessage } from "./archive";
import { receiver, receiverCall } from "./receiver";

import type { Memory as BrainMemory } from "bmo-brain-client";
import type { AgentSummary, MemoryRecord, MemoryRowView, MemorySourceView } from "#shared/types";

// The memories as the memories screen shows them: the rows the brain answers (the brain's `docs/boundary-api.md`,
// 「Memory rows」), each with what its agent is called, the name its room is known by, the names of the person it
// is for and of who set it, and the message it was set from. The brain answers the rows alone; the agent's label
// is the agency's to say, and the room, the people and the message the receiver's. A unit that does not answer
// leaves the row standing by its numbers. The watch serving a row is served by no route, so none stands beside
// it (docs/boundary.md, 「Gaps the console meets」).

// A label the caller wrote on a row, as a number.
function numberLabel(labels: Record<string, string>, key: string): number | null {
  const value = Number(labels[key]);

  return labels[key] !== undefined && Number.isSafeInteger(value) ? value : null;
}

/**
 * A memory row as the memories screen reads it. The brain's fixed API keeps one generic row — its scope, kind,
 * text, state, labels and conditions — and the console's own row was a thing of its kind: its agent, its room, the
 * event it waited on and what it was for. Whatever the caller wrote of those into the row's labels is read back;
 * what is not there stands at its default (docs/boundary.md, 「Gaps the console meets」).
 */
export function memoryRecordOf(memory: BrainMemory): MemoryRecord {
  const labels = memory.labels;
  const type = labels.type;

  return {
    id: memory.id,
    agent: labels.agent ?? memory.subjectKey ?? "",
    slug: labels.slug ?? String(memory.id),
    type: type === "promise" || type === "check" || type === "followup" || type === "situation" || type === "state" ? type : "check",
    chatId: numberLabel(labels, "chatId"),
    reach: labels.reach === "owner" || labels.reach === "group" ? labels.reach : "all",
    condition: labels.condition ?? "",
    event: null,
    eventMetAt: null,
    action: labels.action ?? memory.text,
    forUser: numberLabel(labels, "forUser"),
    active: memory.state !== "retired" && memory.state !== "off",
    lastHitAt: memory.lastMatchedAt === null ? null : Math.floor(memory.lastMatchedAt / 1000),
    hits: memory.matchCount,
    setBy: numberLabel(labels, "setBy"),
    setAt: Math.floor(memory.createdAt / 1000),
    messageId: numberLabel(labels, "messageId"),
    todoId: numberLabel(labels, "todoId"),
    doneAt: memory.state === "done" ? Math.floor(memory.changedAt / 1000) : null,
    doneText: labels.doneText ?? null,
    downReason: labels.downReason ?? null,
    downAt: numberLabel(labels, "downAt"),
    candidateWhy: labels.candidateWhy ?? null,
    candidateAt: numberLabel(labels, "candidateAt"),
    movedAt: Math.floor(memory.changedAt / 1000)
  };
}

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
    const [agents, workers] = await Promise.all([agencyCall(agency().GET("/agents")), agencyCall(agency().GET("/workers"))]);
    const rows: AgentSummary[] = agentSummaries(agents.items, workers.items);

    return new Map(rows.map((agent) => [agent.name, agent.label ?? agent.name]));
  } catch {
    return new Map();
  }
}

// The name each given person last spoke under, as the receiver reads it (`POST /people/names`).
async function personNames(people: readonly number[]): Promise<Map<number, string>> {
  if (people.length === 0) return new Map();

  try {
    const { items } = await receiverCall(receiver().GET("/people", { params: { query: { ids: people.join(",") } } }));

    return new Map(items.flatMap((one) => (one.name === null ? [] : [[one.id, one.name] as const])));
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
