import { createReadStream } from "node:fs";
import { extname } from "node:path";

import type { H3Event } from "h3";

// A file that came with a room's message, served by the message it belongs to. A browser never names
// a path: it names a room, a message and a kind, and the file comes off the receiver, which keeps it
// (docs/web.md, 「Message files」). The rules a served file is held to are this app's: the types each
// kind may be, the kinds that leave only as a file to keep, and the stretch a player asks for.
//
// Audio, video, ordinary downloads, and Telegram visual assets share this authenticated route.

export interface MediaFile {
  path: string;
  type: string;
  bytes: number;
  // What it is called on the disk, which is what a browser saves it as when it is kept.
  name: string;
}

// What each kind is allowed to be. A file whose name is not one of these is refused rather than
// served as something the browser has to guess at.
const TYPES: Record<string, Record<string, string>> = {
  document: {},
  sticker: { ".webp": "image/webp", ".jpg": "image/jpeg", ".png": "image/png", ".webm": "video/webm" },
  // What a room's video plays as. A QuickTime file from a phone is the MPEG-4 family inside, so it
  // is named as MP4, which is the type every browser that plays MP4 takes.
  video: {
    ".mp4": "video/mp4",
    ".m4v": "video/mp4",
    ".mov": "video/mp4",
    ".webm": "video/webm"
  },
  audio: {
    ".ogg": "audio/ogg",
    ".oga": "audio/ogg",
    ".opus": "audio/ogg",
    ".mp3": "audio/mpeg",
    ".m4a": "audio/mp4",
    ".aac": "audio/aac",
    ".wav": "audio/wav",
    ".flac": "audio/flac",
    ".webm": "audio/webm"
  },
  // The page a share was saved from, exactly as it came off the web. Down this road it only ever
  // leaves as a file to keep; drawing it is the saved-page road's business, and that one draws it
  // under an isolation this route has none of.
  page: {
    ".html": "text/html",
    ".htm": "text/html"
  },
  // What a saved page says, in words. This is the one part of a share that is read on screen,
  // and it is plain text, so there is nothing in it to run.
  text: {
    ".txt": "text/plain; charset=utf-8"
  },
  // What was embedded in a saved page and kept beside it.
  media: {
    ".mp4": "video/mp4",
    ".webm": "video/webm",
    ".mov": "video/quicktime",
    ".m4v": "video/mp4",
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".png": "image/png",
    ".gif": "image/gif",
    ".webp": "image/webp"
  }
};

// Which kinds never leave this road as anything but a file to keep, whatever their type says. A
// saved page is somebody else's HTML: handed over here it would be drawn in this console's own
// origin with nothing holding it back, so here it is a download and nothing else.
const KEEP_ONLY = new Set(["page", "document"]);

export function servedAsFile(kind: string): boolean {
  return KEEP_ONLY.has(kind);
}

// Which of a room's kept files a request is about. A message can carry more than one, so the item
// may be named outright; a captured page's embedded files are numbered by their part.
export interface MediaAsked {
  download?: boolean;
  item?: number | null;
  part?: number | null;
}

// The file one message points at. The receiver serves no road for a message's files, and no file is
// read from its folders (docs/boundary.md, 「Gaps the console meets」), so there is none to serve: a room's
// line names its file and its kind, and neither plays nor opens it.
export async function mediaFileFor(_kind: string, _chatId: number, _messageId: number, _asked: MediaAsked = {}): Promise<MediaFile | null> {
  return null;
}

// The kinds a browser plays where the file stands, in the order a name is judged by: a WebM is
// a video before it is a sound.
const PLAYED = ["video", "audio"] as const;

// Which played kind a kept file's name is, or null where a browser plays it as neither.
export function playedKind(name: string): (typeof PLAYED)[number] | null {
  const extension = extname(name).toLowerCase();

  return PLAYED.find((kind) => TYPES[kind]![extension] !== undefined) ?? null;
}

// The type a kind's file of this name is served as, or null where the kind takes no file of that name.
export function servedType(kind: string, name: string): string | null {
  return TYPES[kind]?.[extname(name).toLowerCase()] ?? null;
}

// What a browser asked for of a file it is playing. Seeking in a sound sends a range, and a
// player that is refused one cannot move through a briefing at all.
export interface MediaRange {
  from: number;
  to: number;
}

// The stretch a Range header names, or null where it names none and the whole file is the
// answer. A range nobody could satisfy — beyond the end, or backwards — is refused by returning
// the whole file rather than by failing: the sound still plays.
export function rangeOf(header: string | undefined, bytes: number): MediaRange | null {
  const match = /^bytes=(\d*)-(\d*)$/.exec(header?.trim() ?? "");

  if (match === null || bytes === 0) {
    return null;
  }

  const [, start, end] = match;
  // A range with no start counts back from the end: the last N bytes.
  const from = start === "" ? Math.max(0, bytes - Number(end || 0)) : Number(start);
  const to = start === "" || end === "" ? bytes - 1 : Math.min(Number(end), bytes - 1);

  return from > to || from < 0 || from >= bytes ? null : { from, to };
}

// A kept file put on the wire. Every road a kept file goes down answers a range, because a video is
// played from either and a player that cannot seek cannot move through it. Only the caller's own
// headers differ, so this is written once and they set theirs before calling it. A road whose answer
// must not outlive a link names its own caching.
export function sendKeptFile(event: H3Event, file: MediaFile, keep: boolean, cache = "private, max-age=86400"): Promise<void> {
  const asked = rangeOf(getRequestHeader(event, "range"), file.bytes);

  setResponseHeaders(event, {
    "content-type": file.type,
    // Saying so is what lets a player seek at all; without it a browser must play from the
    // beginning or not at all.
    "accept-ranges": "bytes",
    // It is the reader's own room read behind their own login, so no cache but theirs may hold it.
    "cache-control": cache,
    "content-disposition": keep
      ? `attachment; filename*=UTF-8''${encodeURIComponent(file.name)}`
      : "inline"
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
}
