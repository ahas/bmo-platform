import { describe, expect, test } from "bun:test";

import "../../support/nitro";
import marks from "../../../apps/web/i18n/data/ko.yaml";
import { wording } from "../../../apps/web/shared/wording";
import { answer, standIn } from "../../support/units";

// A room as the receiver keeps it, read from a stand-in receiver into the shapes the room screen draws: the rooms
// with the owner's own named by the chat agent, a page with each sender asked once whether it is an agent, the files
// a message carried named and not available, a reply carrying what it answers, and no judgements beside a page.

const message = (messageId: number, over: Record<string, unknown> = {}) => ({
  chatId: -7,
  messageId,
  senderId: 11,
  senderName: "person-a",
  sentAt: 1_790_000_000 + messageId,
  text: `words ${messageId}`,
  imagePath: null,
  replyToMessageId: null,
  answerSeconds: null,
  round: null,
  album: null,
  via: null,
  files: [],
  ...over
});

const page = [
  message(1),
  message(2, { senderId: 22, senderName: "agent-a", answerSeconds: 4, round: "round:-7:1:1" }),
  message(3, { senderId: 22, senderName: "agent-a", text: `${(marks as { marks: { notice: string } }).marks.notice} restarting` }),
  message(4, {
    replyToMessageId: 1,
    files: [
      { kind: "audio", source: "bmo:voice", storedPath: "/kept/a.ogg", summary: null, collectedAt: 1 },
      { kind: "document", source: "notes.pdf", storedPath: null, summary: null, collectedAt: 2 },
      { kind: "photo", source: "p.jpg", storedPath: null, summary: null, collectedAt: 3 }
    ]
  })
];

const receiver = standIn("RECEIVER", (one) => {
  const path = one.path.split("?")[0]!;

  if (path === "/console/rooms") {
    return answer({
      rooms: [
        { chatId: 5, title: null, messageCount: 3, lastMessageAt: 100, pinned: true },
        { chatId: -7, title: "Room A", messageCount: 4, lastMessageAt: 200, pinned: false, companions: ["companion-a"] }
      ]
    });
  }

  if (path === "/owner") return answer({ chat: 5 });
  if (path === "/console/rooms/-7/title") return answer({ title: "Room A" });
  if (path === "/console/rooms/-7/messages") return answer({ messages: page, hasMore: true });
  if (path === "/console/rooms/-7/messages/after") return answer({ messages: [message(5)], hasMore: false });
  if (path === "/console/rooms/-7/messages/1") return answer({ messages: [page[0]], hasMore: false });
  if (path === "/console/rooms/-7/day-start") return answer({ messageId: 2 });
  if (path.startsWith("/console/rooms/-7/agent-message/")) {
    const id = Number(path.split("/").at(-1));

    return id === 2 ? answer(page[1]) : answer({ code: "not-found", error: "no such message an agent filed" }, 404);
  }

  return answer({ code: "no-route", error: "no route" }, 404);
});

const { listRooms, readDayStart, readMessagePage, readMessagesAfter, readJudgementPage, roomTitleOf } = await import("../../../apps/web/server/utils/archive");
const { registerChatAgent } = await import("../../../apps/web/shared/agent-names");

registerChatAgent({ name: "agent-a", label: "Agent A" });

describe("the rooms", () => {
  test("stand in the receiver's order, the owner's own named by the chat agent, each pinned or not, and a room made for companions so marked", async () => {
    expect(await listRooms()).toEqual([
      { chatId: 5, title: "Agent A", messageCount: 3, lastMessageAt: 100, pinned: true },
      { chatId: -7, title: "Room A", messageCount: 4, lastMessageAt: 200, pinned: false, madeForCompanions: true, companions: ["companion-a"] }
    ]);
    expect(await roomTitleOf(-7)).toBe("Room A");
  });
});

describe("a page of a room", () => {
  test("is asked by its cursor, and each sender is asked once whether it is an agent", async () => {
    receiver.asked.length = 0;

    const read = await readMessagePage(-7, 9, 50);

    expect(receiver.asked.filter((one) => one.path.startsWith("/console/rooms/-7/messages?")).map((one) => one.path)).toEqual(["/console/rooms/-7/messages?limit=50&before=9"]);
    expect(receiver.asked.filter((one) => one.path.includes("agent-message")).map((one) => one.path).sort()).toEqual([
      "/console/rooms/-7/agent-message/1",
      "/console/rooms/-7/agent-message/2"
    ]);
    expect(read.hasMore).toBe(true);
    expect(read.judgements).toEqual([]);
    expect(read.messages.map((one) => [one.messageId, one.agent, one.notice])).toEqual([
      [1, false, false],
      [2, true, false],
      [3, true, true],
      [4, false, false]
    ]);
    expect(read.messages[1]).toMatchObject({ answerSeconds: 4, round: "round:-7:1:1", spend: null, actions: null });
  });

  test("names the files a message carried, none of them available, and carries what a reply answers", async () => {
    const reply = (await readMessagePage(-7, null, 100)).messages[3]!;

    expect(reply.attachments).toEqual([
      { id: 0, kind: "audio", name: wording("room.label.voice_message"), bytes: null, type: null, available: false },
      { id: 1, kind: "document", name: "notes.pdf", bytes: null, type: null, available: false }
    ]);
    expect(reply.replyQuote).toEqual({ messageId: 1, senderName: "person-a", text: "words 1", pictured: false, attachment: null });
  });

  test("past a message, and where a day begins", async () => {
    expect((await readMessagesAfter(-7, 4, 20)).messages.map((one) => one.messageId)).toEqual([5]);
    expect(await readDayStart(-7, 1_790_000_000)).toEqual({ messageId: 2 });
    expect(readJudgementPage()).toEqual({ judgements: [], hasMore: false });
  });
});
