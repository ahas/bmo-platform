import { describe, expect, test } from "bun:test";

import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

import type { MemoriesView, MemoryChanged, MemoryRecord } from "../../../apps/web/shared/types";

// The memories screen's rows: what the stood-in brain answers, read by what the stood-in agency calls each agent
// and by what the stood-in receiver calls each room and person and keeps of each message. What it is held to is
// that a unit that does not know a name leaves the row standing by its number, that taking a row down goes to the
// brain with its reason, and that the brain's refusal is said by the pack's sentence for its code.

const OWNER_CHAT = 5;
const ROOM = -7;
const PERSON = 77;

const row = (id: number, fields: Partial<MemoryRecord>): MemoryRecord => ({
  id,
  agent: "agent-a",
  slug: `m${id}`,
  type: "promise",
  chatId: OWNER_CHAT,
  reach: "all",
  condition: "",
  event: null,
  eventMetAt: null,
  action: "tell when the shell ends",
  forUser: null,
  active: true,
  lastHitAt: null,
  hits: 0,
  setBy: null,
  setAt: 100,
  messageId: null,
  todoId: null,
  doneAt: null,
  doneText: null,
  downReason: null,
  downAt: null,
  candidateWhy: null,
  candidateAt: null,
  movedAt: null,
  ...fields
});

const rows = [
  row(1, {}),
  row(2, { type: "followup", chatId: ROOM, action: "ask again tomorrow", forUser: PERSON, setBy: 1, messageId: 1 }),
  row(3, { type: "followup", chatId: ROOM, action: "a stranger", forUser: 999, messageId: 99 }),
  row(4, { type: "situation", chatId: null, reach: "group", condition: "when asked about lunch", action: "briefly", messageId: 1 }),
  row(5, { agent: "agent-b", chatId: ROOM })
];

const brain = standIn("BRAIN", (one) => {
  const path = one.path.split("?")[0]!;

  if (one.method === "GET" && path === "/memories") return answer({ memories: rows });
  if (one.method === "POST" && path === "/memories/active") {
    if (one.body?.slug === "m2") return answer({ code: "conflict", error: "the row is already down", refusal: { code: "already-down", message: "the row is already down" } }, 409);

    return answer({ ok: true, memory: { ...rows[0]!, active: false, downReason: one.body?.why ?? null, downAt: 200 } });
  }

  return answer({ code: "no-route", error: "no route" }, 404);
});

standIn("AGENCY", (one) => (one.path === "/agents" ? answer({ agents: [{ name: "agent-a", label: "Agent A" }, { name: "agent-b", label: null }] }) : answer({ code: "not-found" }, 404)));

standIn("RECEIVER", (one) => {
  const path = one.path.split("?")[0]!;

  if (path === "/console/rooms") return answer({ rooms: [{ chatId: OWNER_CHAT, title: null, messageCount: 1, lastMessageAt: 1, pinned: false }, { chatId: ROOM, title: "Room A", messageCount: 1, lastMessageAt: 1, pinned: false }] });
  if (path === "/owner") return answer({ chat: OWNER_CHAT });
  if (path === "/people/names") return answer({ names: (one.body?.people as number[]).map((person) => ({ person, name: person === PERSON ? "Person A" : null })) });
  if (path === `/console/rooms/${ROOM}/messages/1`) {
    return answer({ messages: [{ chatId: ROOM, messageId: 1, senderId: PERSON, senderName: "Person A", sentAt: 100, text: "hello", imagePath: null, replyToMessageId: null, answerSeconds: null, round: null, album: null, via: null, files: [] }], hasMore: false });
  }
  if (path === `/console/rooms/${ROOM}/messages/99`) return answer({ messages: [], hasMore: false });

  return answer({ code: "not-found", error: "no such message" }, 404);
});

const { registerChatAgent } = await import("../../../apps/web/shared/agent-names");

registerChatAgent({ name: "agent-a", label: "Agent A" });

const read = await routeOf<MemoriesView>("../../apps/web/server/api/memories.get.ts");
const change = await routeOf<MemoryChanged>("../../apps/web/server/api/memories.post.ts");
const { memories } = await read({});
const [ownerRow, personRow, strangerRow, globalRow, otherRow] = memories;

describe("the memories as the screen reads them", () => {
  test("are the brain's rows, asked for a room where the screen names one", async () => {
    expect(memories.map((one) => one.id)).toEqual([1, 2, 3, 4, 5]);

    await read({ query: { chat: String(ROOM) } });

    expect(brain.asked.at(-1)?.path).toBe(`/memories?chat=${ROOM}`);
  });

  test("a room is read by its name, the owner's own by the chat agent's, and a global row by none", () => {
    expect(ownerRow!.room).toBe("Agent A");
    expect(personRow!.room).toBe("Room A");
    expect(globalRow!.room).toBeNull();
  });

  test("whose a row is is read by what the agency calls the agent, and by its name where it gives no label", () => {
    expect(ownerRow!.agentLabel).toBe("Agent A");
    expect(otherRow!.agentLabel).toBe("agent-b");
  });

  test("a person is read by the name they last spoke under, and by their number where none is known", () => {
    expect(personRow!.forName).toBe("Person A");
    expect(strangerRow!.forName).toBe("999");
    expect(ownerRow!.forName).toBeNull();
    expect(personRow!.setByName).toBe("1");
  });

  test("the message a row was set from is read in its own room, and a global row or a message not kept gives none", () => {
    expect(personRow!.message).toEqual({ senderName: "Person A", sentAt: 100, text: "hello" });
    expect(strangerRow!.message).toBeNull();
    expect(globalRow!.message).toBeNull();
  });

  test("no watch stands beside a row, since no route serves one", () => {
    expect(memories.every((one) => one.watch === null)).toBe(true);
  });
});

describe("a row taken down or put back", () => {
  test("goes to the brain with its reason, and the row comes back as the screen reads it", async () => {
    const changed = await change({ body: { slug: "m1", active: false, why: "no longer needed" } });

    expect(brain.asked.at(-1)).toMatchObject({ method: "POST", path: "/memories/active", body: { slug: "m1", active: false, why: "no longer needed" } });
    expect(changed.memory).toMatchObject({ id: 1, active: false, downReason: "no longer needed", room: "Agent A" });
  });

  test("put back goes without a reason", async () => {
    await change({ body: { slug: "m1", active: true } });

    expect(brain.asked.at(-1)?.body).toEqual({ slug: "m1", active: true });
  });

  test("is refused here for a missing slug, direction or reason, and by the brain's code where the row's state refuses it", async () => {
    const { wording } = await import("../../../apps/web/shared/wording");

    expect(await asked(change, { body: { active: false, why: "x" } })).toMatchObject({ status: 400, message: wording("memories.error.pick_memory") });
    expect(await asked(change, { body: { slug: "m1" } })).toMatchObject({ status: 400, message: wording("memories.error.direction") });
    expect(await asked(change, { body: { slug: "m1", active: false, why: " " } })).toMatchObject({ status: 400, message: wording("memories.error.reason_empty") });
    expect(await asked(change, { body: { slug: "m2", active: false, why: "x" } })).toMatchObject({ status: 409, message: wording("memories.error.row_already_down"), data: { code: "already-down" } });
  });
});
