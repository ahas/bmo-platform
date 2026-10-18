import { PAGING } from "#shared/config";
import { wording } from "#shared/wording";

import type { H3Event } from "h3";

export function requireChatId(event: H3Event): number {
  const raw = getRouterParam(event, "chatId") ?? "";
  const chatId = Number(raw);

  if (!Number.isSafeInteger(chatId)) {
    throw fail(400, wording("room.error.bad_chat_id"));
  }

  return chatId;
}

export function requireMessageId(event: H3Event): number {
  const messageId = Number(getRouterParam(event, "messageId") ?? "");

  if (!Number.isSafeInteger(messageId)) {
    throw fail(400, wording("room.error.bad_message_id"));
  }

  return messageId;
}

export function requireTodoId(event: H3Event): number {
  const raw = getRouterParam(event, "id") ?? "";
  const id = Number(raw);

  if (!Number.isSafeInteger(id)) {
    throw fail(400, wording("todo.error.bad_item_id"));
  }

  return id;
}

export function requireSearchId(event: H3Event): number {
  const id = Number(getRouterParam(event, "id") ?? "");

  if (!Number.isSafeInteger(id) || id < 1) {
    throw fail(400, wording("archive.error.bad_id"));
  }

  return id;
}

// The number of one attachment of a document, as its road names it.
export function requireAttachmentId(event: H3Event): number {
  const id = Number(getRouterParam(event, "attachment") ?? "");

  if (!Number.isSafeInteger(id) || id < 1) {
    throw fail(400, wording("archive.error.bad_attachment_id"));
  }

  return id;
}

// The clock every road reads, in the unit this console stores: unix seconds. A road that took
// milliseconds here would write a link ending a thousand times further off than the span it was
// given.
export function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

// A share link's token as the road carries it. Anything not shaped like one issued here opens
// nothing, and it is refused as not found rather than as malformed: to whoever holds the address,
// a token that was never a token and one that names no link are the same thing.
const SHARE_TOKEN = /^[A-Za-z0-9_-]{22,128}$/;

export function isShareToken(token: string): boolean {
  return SHARE_TOKEN.test(token);
}

export function requireShareToken(event: H3Event): string {
  const token = getRouterParam(event, "token") ?? "";

  if (!isShareToken(token)) {
    throw fail(404, wording("common.error.share_not_found"));
  }

  return token;
}

// Which picture of a page is wanted: its own, its representative image, or its icon. Anything
// else is the page's own, since a request never chooses more than one of the three.
export function pictureOf(value: unknown): "self" | "thumb" | "favicon" {
  return value === "thumb" || value === "favicon" ? value : "self";
}

// A cursor is the id of the row the caller already has; the page ends just before it.
export function readCursor(value: unknown): number | null {
  if (value === undefined || value === null || value === "") {
    return null;
  }

  const cursor = Number(value);

  if (!Number.isSafeInteger(cursor)) {
    throw fail(400, wording("common.error.bad_cursor"));
  }

  return cursor;
}

export function readLimit(value: unknown): number {
  if (value === undefined || value === null || value === "") {
    return PAGING.defaultSize;
  }

  const limit = Number(value);

  if (!Number.isSafeInteger(limit) || limit < 1) {
    throw fail(400, wording("common.error.bad_limit"));
  }

  return Math.min(limit, PAGING.maxSize);
}

// The message a send answers, as a body names it: a message number, or nothing. Anything else is a
// request that was never going to be a reply.
export function replyToOf(value: unknown): number | null {
  if (value === undefined || value === null || value === "") {
    return null;
  }

  const id = typeof value === "number" ? value : Number(value);

  if (!Number.isSafeInteger(id) || id === 0) {
    throw fail(400, wording("room.error.bad_reply_id"));
  }

  return id;
}
