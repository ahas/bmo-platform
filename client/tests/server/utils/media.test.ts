import { describe, expect, test } from "bun:test";

// The rules a message's file is held to on its way out (docs/web.md, the room screen's message files): which of a
// message's files a request names, the types each kind may be, the kinds that leave only as a file to keep, and
// which names play where they stand.

const { fileAsked, fileOut, playedKind, servedAsFile, servedType } = await import("../../../apps/web/server/utils/media");

const file = (place: number, kind: string, name: string, over: Partial<{ type: string | null; size: number | null; kept: boolean }> = {}) => ({
  place, kind, name, type: null, size: 10, kept: true, ...over
});

describe("the file a request names", () => {
  const files = [file(0, "photo", "a.jpg"), file(1, "document", "take.mp3"), file(2, "audio", "voice.ogg"), file(3, "document", "report.pdf")];

  test("is the one at its place, where the kind asked for may stand there", () => {
    expect(fileAsked("audio", files, 2)?.place).toBe(2);
    // A sound kept as a document is asked for as a sound.
    expect(fileAsked("audio", files, 1)?.place).toBe(1);
    expect(fileAsked("video", files, 2)).toBeNull();
    expect(fileAsked("audio", files, 9)).toBeNull();
  });

  test("is the kind's first where no place is named", () => {
    expect(fileAsked("audio", files, null)?.place).toBe(1);
    expect(fileAsked("sticker", files, null)).toBeNull();
  });
});

describe("how a kept file goes out", () => {
  test("as the type its kind takes for its name, a MOV as MP4", () => {
    expect(fileOut("audio", file(0, "audio", "line.mp3"), false)).toEqual({ type: "audio/mpeg", keep: false });
    expect(fileOut("video", file(0, "video", "IMG_1.MOV"), false)).toEqual({ type: "video/mp4", keep: false });
  });

  test("as the receiver's type where the name says none and the kind takes that type", () => {
    expect(fileOut("audio", file(0, "audio", "voice", { type: "audio/ogg" }), false)).toEqual({ type: "audio/ogg", keep: false });
    expect(fileOut("audio", file(0, "audio", "voice", { type: "text/html" }), false)).toBeNull();
  });

  test("a document, and a sound or a video asked for to keep, leave as a file to keep", () => {
    expect(fileOut("document", file(0, "document", "page.html"), false)).toEqual({ type: "application/octet-stream", keep: true });
    expect(fileOut("audio", file(0, "audio", "line.mp3"), true)).toEqual({ type: "application/octet-stream", keep: true });
  });

  test("never where its bytes are not kept, or for a kind no message's file is asked for by", () => {
    expect(fileOut("audio", file(0, "audio", "line.mp3", { kept: false }), false)).toBeNull();
    expect(fileOut("page", file(0, "page", "saved.html"), false)).toBeNull();
  });

  test("is served with the type its name says, and a name that is not its kind's is refused", () => {
    expect(servedType("audio", "line.mp3")).toBe("audio/mpeg");
    expect(servedType("video", "IMG_1.MOV")).toBe("video/mp4");
    expect(servedType("audio", "line.txt")).toBeNull();
    expect(servedType("nothing", "line.mp3")).toBeNull();
  });

  test("a saved page and a document leave only as a file to keep", () => {
    expect(servedAsFile("page")).toBe(true);
    expect(servedAsFile("document")).toBe(true);
    expect(servedAsFile("audio")).toBe(false);
  });

  test("a name is a video before it is a sound, and neither where no browser plays it", () => {
    expect(playedKind("clip.webm")).toBe("video");
    expect(playedKind("clip.MOV")).toBe("video");
    expect(playedKind("voice.opus")).toBe("audio");
    expect(playedKind("take.m4a")).toBe("audio");
    expect(playedKind("film.mkv")).toBeNull();
    expect(playedKind("report.html")).toBeNull();
  });
});
