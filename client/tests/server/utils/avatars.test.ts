import { afterAll, beforeEach, describe, expect, test } from "bun:test";

// A profile picture is the chat service's (docs/web.md, 「Profile photos」): the console reads a room's off the
// Telegram service's `GET /rooms/{roomId}/avatar` and a person's off `GET /people/{personId}/avatar`, with that
// service's token, and serves it under its own path. The pictures the service keeps are listed at `GET /avatars`,
// a page at a time, oldest received first.

const PICTURE = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3]);

// The pictures the stand-in keeps, as the provider lists them.
const KEPT = [
  { kind: "room", id: -100, takenAt: 1_000 },
  { kind: "person", id: 42, takenAt: 2_000 },
  { kind: "person", id: 43, takenAt: 3_000 },
  { kind: "room", id: -200, takenAt: 4_000 }
];

const heard: { path: string; after: string | null; token: string | null }[] = [];
let pageSize = 2;
let refusedFrom: number | null = null;
let listed = 0;

const telegram = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  fetch: (request) => {
    const url = new URL(request.url);

    heard.push({ path: url.pathname, after: url.searchParams.get("after"), token: request.headers.get("authorization") });

    if (url.pathname === "/avatars") {
      listed += 1;

      if (refusedFrom !== null && listed >= refusedFrom) {
        return Response.json({ code: "unavailable", error: "not now" }, { status: 503 });
      }

      const after = url.searchParams.has("after") ? Number(url.searchParams.get("after")) : Number.NEGATIVE_INFINITY;
      const later = KEPT.filter((row) => row.takenAt > after);
      const page = later.slice(0, pageSize);

      return Response.json({ items: page, more: later.length > page.length });
    }

    return url.pathname === "/rooms/-100/avatar"
      ? new Response(PICTURE, { headers: { "content-type": "image/jpeg" } })
      : Response.json({ error: "no picture is kept", code: "no-route" }, { status: 404 });
  }
});

beforeEach(() => {
  heard.length = 0;
  pageSize = 2;
  refusedFrom = null;
  listed = 0;
  process.env.TELEGRAM_ADDRESS = `http://127.0.0.1:${telegram.port}`;
  process.env.TELEGRAM_TOKEN = "the service's token";
});

afterAll(async () => {
  await telegram.stop(true);
});

const { avatarPicture, avatarRefs, isAvatarKind } = await import("../../../apps/web/server/utils/avatars");

describe("a profile picture", () => {
  test("is the one the chat service keeps, asked with its token", async () => {
    expect(await avatarPicture("room", -100)).toEqual(PICTURE);
    expect(heard.at(-1)).toEqual({ path: "/rooms/-100/avatar", after: null, token: "Bearer the service's token" });
  });

  test("is none where the service keeps none, or where no service is named", async () => {
    expect(await avatarPicture("person", 42)).toBeNull();
    expect(heard.at(-1)?.path).toBe("/people/42/avatar");

    delete process.env.TELEGRAM_ADDRESS;
    expect(await avatarPicture("room", -100)).toBeNull();
  });

  test("is asked of a room or a person and nothing else", () => {
    expect(isAvatarKind("room")).toBe(true);
    expect(isAvatarKind("person")).toBe(true);
    expect(isAvatarKind("machine")).toBe(false);
  });
});

describe("the pictures the chat service keeps", () => {
  test("are read page by page, oldest received first, until the service says there is no more", async () => {
    expect(await avatarRefs()).toEqual([
      { kind: "room", id: -100, url: "/api/avatars/room/-100?v=1000" },
      { kind: "person", id: 42, url: "/api/avatars/person/42?v=2000" },
      { kind: "person", id: 43, url: "/api/avatars/person/43?v=3000" },
      { kind: "room", id: -200, url: "/api/avatars/room/-200?v=4000" }
    ]);
    expect(heard.map((one) => [one.path, one.after])).toEqual([
      ["/avatars", null],
      ["/avatars", "2000"]
    ]);
    expect(heard.every((one) => one.token === "Bearer the service's token")).toBe(true);
  });

  test("stand as many as were read where a later page is refused", async () => {
    refusedFrom = 2;

    expect(await avatarRefs()).toEqual([
      { kind: "room", id: -100, url: "/api/avatars/room/-100?v=1000" },
      { kind: "person", id: 42, url: "/api/avatars/person/42?v=2000" }
    ]);
  });

  test("are none where no service is named", async () => {
    delete process.env.TELEGRAM_ADDRESS;

    expect(await avatarRefs()).toEqual([]);
  });
});
