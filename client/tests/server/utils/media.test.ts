import { describe, expect, test } from "bun:test";

// The rules a message's file is held to on its way out (docs/web.md, 「Message files」): the types each kind
// may be, the kinds that leave only as a file to keep, which names play where they stand, and the stretch a
// player asked for. While the receiver serves no road for a message's files, none is served.

const { mediaFileFor, playedKind, rangeOf, servedAsFile, servedType } = await import("../../../apps/web/server/utils/media");

describe("the file a message points at", () => {
  test("is none while the receiver serves no road for it", async () => {
    expect(await mediaFileFor("audio", -100, 9)).toBeNull();
    expect(await mediaFileFor("video", -100, 9, { item: 3 })).toBeNull();
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

describe("the stretch a player asked for", () => {
  test("a plain range is the bytes it names", () => {
    expect(rangeOf("bytes=0-99", 500)).toEqual({ from: 0, to: 99 });
    expect(rangeOf("bytes=100-199", 500)).toEqual({ from: 100, to: 199 });
  });

  test("no end named means to the end of the file", () => {
    expect(rangeOf("bytes=400-", 500)).toEqual({ from: 400, to: 499 });
  });

  test("no start named counts back from the end", () => {
    expect(rangeOf("bytes=-100", 500)).toEqual({ from: 400, to: 499 });
  });

  test("an end past the file is the end of the file", () => {
    expect(rangeOf("bytes=400-9999", 500)).toEqual({ from: 400, to: 499 });
  });

  test("nothing asked for is the whole file", () => {
    expect(rangeOf(undefined, 500)).toBeNull();
    expect(rangeOf("", 500)).toBeNull();
    expect(rangeOf("bytes=abc", 500)).toBeNull();
  });

  test("a range nobody could satisfy takes the whole file rather than failing", () => {
    // Backwards, past the end, and a file with nothing in it.
    expect(rangeOf("bytes=300-100", 500)).toBeNull();
    expect(rangeOf("bytes=900-999", 500)).toBeNull();
    expect(rangeOf("bytes=0-10", 0)).toBeNull();
  });
});
