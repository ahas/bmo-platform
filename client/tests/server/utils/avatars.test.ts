import { afterAll, describe, expect, test } from "bun:test";

// A profile picture is the chat service's (docs/web.md, 「Profile photos」): the console reads a room's off the
// Telegram service's `GET /rooms/{roomId}/avatar` and a person's off `GET /people/{personId}/avatar`, with that
// service's token, and serves it under its own path.

const PICTURE = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3]);
const heard: { path: string; token: string | null }[] = [];

const telegram = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  fetch: (request) => {
    const path = new URL(request.url).pathname;

    heard.push({ path, token: request.headers.get("authorization") });

    return path === "/rooms/-100/avatar"
      ? new Response(PICTURE, { headers: { "content-type": "image/jpeg" } })
      : Response.json({ error: "no picture is kept", code: "no-route" }, { status: 404 });
  }
});

afterAll(async () => {
  await telegram.stop(true);
});

const { avatarPicture, avatarRefs, isAvatarKind } = await import("../../../apps/web/server/utils/avatars");

describe("a profile picture", () => {
  test("is the one the chat service keeps, asked with its token", async () => {
    process.env.TELEGRAM_ADDRESS = `http://127.0.0.1:${telegram.port}`;
    process.env.TELEGRAM_TOKEN = "the service's token";

    expect(await avatarPicture("room", -100)).toEqual(PICTURE);
    expect(heard.at(-1)).toEqual({ path: "/rooms/-100/avatar", token: "Bearer the service's token" });
  });

  test("is none where the service keeps none, or where no service is named", async () => {
    process.env.TELEGRAM_ADDRESS = `http://127.0.0.1:${telegram.port}`;
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

  test("stands in no list while no unit lists the pictures it keeps", async () => {
    expect(await avatarRefs()).toEqual([]);
  });
});
