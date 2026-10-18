import { createReadStream } from "node:fs";
import { wording } from "#shared/wording";

// A kept file is served by the message it belongs to, never by a path the browser names, and
// only to somebody who has logged in, in a room their user may read. A player seeking through a
// sound asks for a stretch of it at a time, so a range is answered as one.
export default defineEventHandler(async (event) => {
  const kind = getRouterParam(event, "kind") ?? "";

  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);

  const messageId = Number(getRouterParam(event, "messageId") ?? "");

  if (!Number.isSafeInteger(messageId)) {
    throw fail(400, wording("room.error.bad_message_id"));
  }

  const query = getQuery(event);
  const file = await mediaFileFor(kind, chatId, messageId, { item: whole(query.item), part: whole(query.part), download: query.as === "download" });

  if (file === null) {
    throw fail(404, wording("room.error.file_not_found"));
  }

  const asked = rangeOf(getRequestHeader(event, "range"), file.bytes);
  // A saved page is somebody else's markup and never runs in this console: it leaves as a file
  // to keep, and so does anything the reader asked to keep rather than to open.
  const keep = servedAsFile(kind) || query.as === "download";

  setResponseHeaders(event, {
    "content-type": file.type,
    // Saying so is what lets a player seek at all; without it a browser must play from the
    // beginning or not at all.
    "accept-ranges": "bytes",
    // It is the owner's own room read behind their own login, so no cache but theirs may hold it.
    "cache-control": "private, max-age=86400",
    // The type is the one this app decided from the file's own name; a browser sniffing its way
    // to another is how a kept file becomes a running one.
    "x-content-type-options": "nosniff",
    ...(keep ? { "content-disposition": `attachment; filename*=UTF-8''${encodeURIComponent(file.name)}` } : {})
  });

  if (asked === null) {
    setResponseHeaders(event, { "content-length": file.bytes });

    return sendStream(event, createReadStream(file.path));
  }

  setResponseStatus(event, 206);
  setResponseHeaders(event, {
    "content-length": asked.to - asked.from + 1,
    "content-range": `bytes ${asked.from}-${asked.to}/${file.bytes}`
  });

  return sendStream(event, createReadStream(file.path, { start: asked.from, end: asked.to }));
});

// A whole number an address names, or nothing where it names none.
function whole(value: unknown): number | null {
  const asked = Number.parseInt(typeof value === "string" ? value : "", 10);

  return Number.isSafeInteger(asked) ? asked : null;
}
