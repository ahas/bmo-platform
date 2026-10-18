import { afterEach, describe, expect, setSystemTime, test } from "bun:test";
import { createApp, eventHandler, toWebHandler, unsealSession, useSession } from "h3";

import { REMEMBER_LOGIN_DAYS, REMEMBER_LOGIN_SECONDS } from "../../../apps/web/shared/config";
import { SESSION_SLIDE_SECONDS, sessionLife, sessionRemembers, sessionWantsSlide } from "../../../apps/web/server/utils/session-life";

// The two lives a login can have, and what tells them apart. What they are held to: the switch
// off says nothing at all, which is what leaves the cookie the browser's own session; the switch
// on names a span from the moment of the write, which is both the cookie's expiry and the expiry
// the seal carries; and a kept login in use is resealed so the span counts from the last visit.

const DAY = 24 * 60 * 60 * 1000;
const PASSWORD = "a-session-password-long-enough-for-iron-seals";

afterEach(() => {
  setSystemTime();
});

describe("sessionLife", () => {
  test("a login not kept names no age, so the cookie dies with the browser", () => {
    expect(sessionLife(false)).toEqual({});
    expect("maxAge" in sessionLife(false)).toBe(false);
  });

  test("a kept login names the span, and the cookie's expiry a span from the write", () => {
    expect(sessionLife(true, 1000)).toEqual({ maxAge: REMEMBER_LOGIN_SECONDS, cookie: { expires: new Date(1000 + REMEMBER_LOGIN_SECONDS * 1000) } });
  });

  test("the span is seven days", () => {
    expect(REMEMBER_LOGIN_DAYS).toBe(7);
    expect(REMEMBER_LOGIN_SECONDS).toBe(7 * 24 * 60 * 60);
  });
});

describe("sessionRemembers", () => {
  test("only the switch having been on reads as kept", () => {
    expect(sessionRemembers({ remember: true })).toBe(true);
    expect(sessionRemembers({ remember: false })).toBe(false);
  });

  test("a session written before this existed is not a kept one", () => {
    expect(sessionRemembers({})).toBe(false);
    expect(sessionRemembers({ remember: "true" })).toBe(false);
  });
});

describe("sessionWantsSlide", () => {
  const user = { name: "owner" };
  const interval = SESSION_SLIDE_SECONDS * 1000;

  test("a kept login slides once the interval since the login has passed", () => {
    expect(sessionWantsSlide({ user, remember: true, loggedInAt: 0 }, interval)).toBe(true);
    expect(sessionWantsSlide({ user, remember: true, loggedInAt: 0 }, interval - 1)).toBe(false);
  });

  test("the last slide, not the login, is what the interval counts from", () => {
    expect(sessionWantsSlide({ user, remember: true, loggedInAt: 0, touchedAt: 5 * DAY }, 5 * DAY + interval - 1)).toBe(false);
    expect(sessionWantsSlide({ user, remember: true, loggedInAt: 0, touchedAt: 5 * DAY }, 5 * DAY + interval)).toBe(true);
  });

  test("a browser-session login and an empty session never slide", () => {
    expect(sessionWantsSlide({ user, remember: false, loggedInAt: 0 }, 10 * DAY)).toBe(false);
    expect(sessionWantsSlide({ remember: true, loggedInAt: 0 }, 10 * DAY)).toBe(false);
  });
});

// The life handed to the real session layer, so what is held is what the browser and the server
// actually get: the cookie's Expires and the seal's own expiry.
describe("a kept session written through the session layer", () => {
  async function write(): Promise<string> {
    const app = createApp();

    app.use(
      eventHandler(async (event) => {
        const session = await useSession(event, { password: PASSWORD, name: "s", ...sessionLife(true) });

        await session.update({ remember: true });
        return "ok";
      })
    );

    const response = await toWebHandler(app)(new Request("http://localhost/"));

    return response.headers.get("set-cookie") ?? "";
  }

  function seal(cookie: string): string {
    return decodeURIComponent(/s=([^;]+)/.exec(cookie)?.[1] ?? "");
  }

  function expires(cookie: string): number {
    return Date.parse(/Expires=([^;]+)/i.exec(cookie)?.[1] ?? "");
  }

  async function opens(sealed: string): Promise<boolean> {
    // The read side names no age, as every reader of the session in the console does; the seal
    // refuses on its own expiry.
    return unsealSession({} as never, { password: PASSWORD }, sealed).then(
      () => true,
      () => false
    );
  }

  test("a write sets the cookie to expire a span from that write, and again from the next", async () => {
    setSystemTime(new Date(Date.UTC(2026, 0, 1)));
    const first = await write();

    expect(expires(first)).toBe(Date.UTC(2026, 0, 1) + REMEMBER_LOGIN_SECONDS * 1000);

    setSystemTime(new Date(Date.UTC(2026, 0, 4)));
    const second = await write();

    expect(expires(second)).toBe(Date.UTC(2026, 0, 4) + REMEMBER_LOGIN_SECONDS * 1000);
  });

  test("the server refuses a seal once a span has passed since it was written, and not before", async () => {
    setSystemTime(new Date(Date.UTC(2026, 0, 1)));
    const sealed = seal(await write());

    setSystemTime(new Date(Date.UTC(2026, 0, 1) + REMEMBER_LOGIN_SECONDS * 1000 - 60_000));
    expect(await opens(sealed)).toBe(true);

    setSystemTime(new Date(Date.UTC(2026, 0, 1) + REMEMBER_LOGIN_SECONDS * 1000 + 60_000));
    expect(await opens(sealed)).toBe(false);
  });
});
