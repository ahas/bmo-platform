import { PAGING, roomLabel } from "#shared/config";
import { isSystemNotice } from "#shared/text-marks";
import { wording } from "#shared/wording";

import { ownerChat } from "./owner";
import { carriesFiles, messageMedia } from "./message-media";
import { receiverRead } from "./receiver";

import type { ArchivedMessage, JudgementPage, MessageAttachment, MessagePage, ReplyQuote, RoomSummary, TelegramExpression } from "#shared/types";

// A room as the receiver keeps it (the receiver's `docs/boundary-api.md`, 「For the web unit's console: reading a
// room」): the rooms, a room's name, and its line walked by a cursor the reader holds, read into the shapes the room
// screen draws. A page holds only what a person said themselves, as the receiver hands it. What the receiver's
// pages do not carry stands empty (docs/boundary.md, 「Gaps the console meets」): the judgements beside a page, which
// are the gate's; what a round cost and what its turn did; and a line's voice briefing. A message that carries files
// is read again for them and for the metadata it arrived with (`server/utils/message-media.ts`).

interface RoomLine {
  chatId: number;
  title: string | null;
  messageCount: number;
  lastMessageAt: number | null;
  pinned: boolean;
  companions?: string[];
}

// A file a message carried, as the receiver's store keeps it.
interface KeptFile {
  kind: string;
  source: string;
  storedPath: string | null;
  summary: string | null;
  collectedAt: number;
}

interface ReceivedMessage {
  chatId: number;
  messageId: number;
  senderId: number | null;
  senderName: string;
  sentAt: number;
  text: string;
  imagePath: string | null;
  replyToMessageId: number | null;
  answerSeconds: number | null;
  round: string | null;
  album: number | null;
  via: string | null;
  files: KeptFile[];
}

interface ReceivedPage {
  messages: ReceivedMessage[];
  hasMore: boolean;
}

const ATTACHED: readonly MessageAttachment["kind"][] = ["audio", "video", "document", "sticker"];

/** Every room, the pinned ones first in the owner's order, each under the name it is called by. */
export async function listRooms(): Promise<RoomSummary[]> {
  const [{ rooms }, owner] = await Promise.all([receiverRead<{ rooms: RoomLine[] }>("/console/rooms"), ownerChat()]);

  return rooms.map((room) => ({
    chatId: room.chatId,
    title: roomLabel(room.chatId, room.title, owner),
    messageCount: room.messageCount,
    lastMessageAt: room.lastMessageAt,
    pinned: room.pinned,
    ...(room.companions !== undefined ? { madeForCompanions: true as const, companions: room.companions } : {})
  }));
}

/** One room by the name it is called by, the same name the room list gives it. */
export async function roomTitleOf(chatId: number): Promise<string> {
  const [{ title }, owner] = await Promise.all([receiverRead<{ title: string | null }>(`/console/rooms/${chatId}/title`), ownerChat()]);

  return roomLabel(chatId, title, owner);
}

// The files a message carried as its page names them, each by its place among them and standing unavailable: what
// the line stands where the receiver's list of them could not be read.
function attachmentsOf(message: ReceivedMessage): MessageAttachment[] {
  return message.files.flatMap((file, at) => {
    const kind = file.kind as MessageAttachment["kind"];

    return ATTACHED.includes(kind)
      ? [{ id: at, kind, name: file.source.startsWith("bmo:") ? wording("room.label.voice_message") : file.source, bytes: null, type: null, available: false }]
      : [];
  });
}

// What a message carried and the expressions its words drew. Only a message whose page names a file is asked
// about, since every expression stands on one of its files; a list that cannot be read is no reason to fail the
// page, and the line names its files as unavailable instead.
async function mediaOf(chatId: number, message: ReceivedMessage): Promise<{ attachments: MessageAttachment[]; expressions: TelegramExpression[] }> {
  if (!carriesFiles(message.files)) return { attachments: [], expressions: [] };

  try {
    return await messageMedia(chatId, message.messageId, message.text);
  } catch {
    return { attachments: attachmentsOf(message), expressions: [] };
  }
}

// Whether each sender of a page is an agent, asked of the receiver once a sender on one of its messages: the
// receiver answers a message an agent filed and refuses one a person said.
async function agentSenders(chatId: number, messages: readonly ReceivedMessage[]): Promise<Map<string, boolean>> {
  const firstOf = new Map<string, number>();

  for (const message of messages) {
    const sender = senderKey(message);

    if (!firstOf.has(sender)) firstOf.set(sender, message.messageId);
  }

  const asked = await Promise.all(
    [...firstOf].map(async ([sender, messageId]) => {
      try {
        await receiverRead(`/console/rooms/${chatId}/agent-message/${messageId}`);

        return [sender, true] as const;
      } catch {
        return [sender, false] as const;
      }
    })
  );

  return new Map(asked);
}

function senderKey(message: ReceivedMessage): string {
  return `${message.senderId ?? ""}:${message.senderName}`;
}

// The message a reply answers, as the reply shows it; one the receiver does not hand over stands with nothing.
async function replyQuoteOf(chatId: number, messageId: number): Promise<ReplyQuote> {
  try {
    const answered = (await receiverRead<ReceivedPage>(`/console/rooms/${chatId}/messages/${messageId}`)).messages[0];

    if (answered !== undefined) {
      return {
        messageId,
        senderName: answered.senderName,
        text: answered.text,
        pictured: answered.imagePath !== null,
        attachment: (await mediaOf(chatId, answered)).attachments[0] ?? null
      };
    }
  } catch {
    // A quote is no reason to fail the page.
  }

  return { messageId, senderName: null, text: "", pictured: false, attachment: null };
}

// A page of the receiver's read into the messages the screen draws.
async function messagesOf(chatId: number, received: readonly ReceivedMessage[]): Promise<ArchivedMessage[]> {
  const agents = await agentSenders(chatId, received);
  // Several replies of a page often answer one message; what it said and carried is read once.
  const quoted = new Map<number, Promise<ReplyQuote>>();
  const quote = (messageId: number): Promise<ReplyQuote> => {
    let held = quoted.get(messageId);

    if (held === undefined) {
      held = replyQuoteOf(chatId, messageId);
      quoted.set(messageId, held);
    }

    return held;
  };

  return await Promise.all(
    received.map(async (message) => {
      const agent = agents.get(senderKey(message)) === true;
      const media = await mediaOf(chatId, message);

      return {
        chatId: message.chatId,
        messageId: message.messageId,
        senderId: message.senderId,
        senderName: message.senderName,
        sentAt: message.sentAt,
        text: message.text,
        imagePath: message.imagePath,
        replyToMessageId: message.replyToMessageId,
        answerSeconds: message.answerSeconds,
        round: message.round,
        album: message.album,
        agent,
        notice: agent && isSystemNotice(message.text),
        attachments: media.attachments,
        expressions: media.expressions,
        replyQuote: message.replyToMessageId === null ? null : await quote(message.replyToMessageId),
        spend: null,
        actions: null
      };
    })
  );
}

async function pageOf(chatId: number, path: string): Promise<MessagePage> {
  const page = await receiverRead<ReceivedPage>(path);

  return { messages: await messagesOf(chatId, page.messages), judgements: [], hasMore: page.hasMore };
}

/** One page of a room, oldest first, ending just before `before` when it is given. */
export async function readMessagePage(chatId: number, before: number | null, limit: number): Promise<MessagePage> {
  const asked = new URLSearchParams({ limit: String(Math.min(limit, PAGING.maxSize)) });

  if (before !== null) asked.set("before", String(before));

  return await pageOf(chatId, `/console/rooms/${chatId}/messages?${asked}`);
}

/** The messages of a room past a message the reader already holds, oldest first. */
export async function readMessagesAfter(chatId: number, after: number, limit: number): Promise<MessagePage> {
  const asked = new URLSearchParams({ after: String(after), limit: String(Math.min(limit, PAGING.maxSize)) });

  return await pageOf(chatId, `/console/rooms/${chatId}/messages/after?${asked}`);
}

/** One message of a room by its own number, as a page of one, or an empty page where the screen stands none. */
export async function readOneMessage(chatId: number, messageId: number): Promise<MessagePage> {
  return await pageOf(chatId, `/console/rooms/${chatId}/messages/${messageId}`);
}

/** Where a day begins in a room: the first message a person said at or after a moment, or null. */
export async function readDayStart(chatId: number, from: number): Promise<{ messageId: number } | null> {
  const { messageId } = await receiverRead<{ messageId: number | null }>(`/console/rooms/${chatId}/day-start?from=${from}`);

  return messageId === null ? null : { messageId };
}

/** A room's judgements newest first. They are the gate's, which serves the console none yet. */
export function readJudgementPage(): JudgementPage {
  return { judgements: [], hasMore: false };
}
