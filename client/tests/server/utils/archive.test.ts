import { describe, expect, test } from "bun:test";

import "../../support/nitro";
import marks from "../../../apps/web/i18n/data/ko.yaml";
import { wording } from "../../../apps/web/shared/wording";
import { answer, standIn } from "../../support/units";

// A room as the receiver keeps it, read from a stand-in receiver into the shapes the room screen draws: the rooms
// with the owner's own named by the chat agent, a page with each sender asked once whether it is an agent, the files
// a message carried with whether each can be received and the expressions its words drew, asked only of a message
// that carries files, a reply carrying what it answers, and no judgements beside a page.

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

// A room whose messages carry a custom emoji, a sticker and a video, and one whose files cannot be listed.
const EMOJI = "\u{1F600}";
const drawn = [
  message(6, {
    chatId: -8,
    text: `hi ${EMOJI} there`,
    files: [
      { kind: "sticker", source: "emoji-123.webp", storedPath: "/kept/e.webp", summary: null, collectedAt: 1 },
      { kind: "sticker", source: "sticker.webp", storedPath: "/kept/s.webp", summary: null, collectedAt: 2 },
      { kind: "video", source: "clip.mp4", storedPath: "/kept/c.mp4", summary: null, collectedAt: 3 }
    ]
  }),
  message(7, { chatId: -8, files: [{ kind: "document", source: "lost.pdf", storedPath: null, summary: null, collectedAt: 4 }] })
];
const keptFile = (place: number, kind: string, name: string, type: string | null, size: number | null, kept = true) => ({ place, kind, name, type, size, kept });

const receiver = standIn("RECEIVER", (one) => {
  const path = one.path.split("?")[0]!;

  if (path === "/console/rooms/-7/messages/4/files") {
    return answer({ files: [keptFile(0, "audio", "bmo:voice", "audio/ogg", 1200), keptFile(1, "document", "notes.pdf", "application/pdf", null, false), keptFile(2, "photo", "p.jpg", "image/jpeg", 50)] });
  }
  if (path === "/console/rooms/-7/messages/4/metadata") return answer({ metadata: null });
  if (path === "/console/rooms/-8/messages") return answer({ messages: drawn, hasMore: false });
  if (path === "/console/rooms/-8/messages/6/files") {
    return answer({ files: [keptFile(0, "sticker", "emoji-123.webp", "image/webp", 40), keptFile(1, "sticker", "sticker.webp", "image/webp", 90), keptFile(2, "video", "clip.mp4", "video/mp4", 5000)] });
  }
  if (path === "/console/rooms/-8/messages/6/metadata") {
    return answer({
      metadata: {
        text: `hi ${EMOJI} there`,
        entities: [{ type: "custom_emoji", offset: 3, length: 2, custom_emoji_id: "123" }, { type: "bold", offset: 0, length: 2 }],
        sticker: { set: "set-a" },
        attachment: { width: 640, height: 360 }
      }
    });
  }
  if (path === "/console/rooms/-8/messages/7/files") return answer({ code: "unit-failed", error: "the store did not answer" }, 503);

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

  test("carries each file a message kept, by its place, with its size and type and whether it can be received, and what a reply answers", async () => {
    receiver.asked.length = 0;

    const reply = (await readMessagePage(-7, null, 100)).messages[3]!;

    expect(reply.attachments).toEqual([
      { id: 0, kind: "audio", name: wording("room.label.voice_message"), bytes: 1200, type: "audio/ogg", available: true },
      { id: 1, kind: "document", name: "notes.pdf", bytes: null, type: null, available: false }
    ]);
    expect(reply.expressions).toEqual([]);
    expect(reply.replyQuote).toEqual({ messageId: 1, senderName: "person-a", text: "words 1", pictured: false, attachment: null });
    // Only the message that carries files is asked about them.
    expect(receiver.asked.filter((one) => /\/(files|metadata)$/.test(one.path)).map((one) => one.path).sort()).toEqual([
      "/console/rooms/-7/messages/4/files",
      "/console/rooms/-7/messages/4/metadata"
    ]);
  });

  test("draws a custom emoji over the words it stands for, the latest sticker alone, and a video in the shape it arrived in", async () => {
    const [shown, unlisted] = (await readMessagePage(-8, null, 100)).messages;
    const emoji = { id: 0, kind: "sticker" as const, name: "emoji-123.webp", bytes: 40, type: "image/webp", available: true };

    expect(shown!.expressions).toEqual([{ offset: 3, length: 2, attachment: emoji }]);
    expect(shown!.attachments).toEqual([
      { id: 1, kind: "sticker", name: "sticker.webp", bytes: 90, type: "image/webp", available: true },
      { id: 2, kind: "video", name: "clip.mp4", bytes: 5000, type: "video/mp4", available: true, shape: { width: 640, height: 360 } }
    ]);
    // A list the receiver could not give leaves the files named and unavailable, and the page stands.
    expect(unlisted!.attachments).toEqual([{ id: 0, kind: "document", name: "lost.pdf", bytes: null, type: null, available: false }]);
  });

  test("past a message, and where a day begins", async () => {
    expect((await readMessagesAfter(-7, 4, 20)).messages.map((one) => one.messageId)).toEqual([5]);
    expect(await readDayStart(-7, 1_790_000_000)).toEqual({ messageId: 2 });
    expect(readJudgementPage()).toEqual({ judgements: [], hasMore: false });
  });
});
