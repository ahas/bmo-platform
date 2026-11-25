import { describe, expect, test } from "bun:test";

import { holdRelay, openRelayCount, STOPPING_FRAME, stopRelays } from "../../../apps/web/server/utils/relays";

describe("the relays this app holds open", () => {
  test("are each told the app is stopping, then ended, and none is held after", () => {
    const said: string[] = [];
    const ended: string[] = [];

    holdRelay({ write: (text) => said.push(`a:${text}`), stop: () => ended.push("a") });
    holdRelay({ write: (text) => said.push(`b:${text}`), stop: () => ended.push("b") });

    stopRelays();

    expect(said).toEqual([`a:${STOPPING_FRAME}`, `b:${STOPPING_FRAME}`]);
    expect(ended).toEqual(["a", "b"]);
    expect(openRelayCount()).toBe(0);
  });

  test("the word is the one the bot sends, with no id to move a cursor", () => {
    expect(STOPPING_FRAME).toBe("event: stopping\ndata: {}\n\n");
  });

  test("a relay that already let go is not told", () => {
    const said: string[] = [];
    const release = holdRelay({ write: (text) => said.push(text), stop: () => undefined });

    release();
    stopRelays();

    expect(said).toEqual([]);
  });

  test("one whose browser has gone does not keep the others from hearing", () => {
    const ended: string[] = [];

    holdRelay({
      write: () => {
        throw new Error("closed");
      },
      stop: () => ended.push("gone")
    });
    holdRelay({ write: () => undefined, stop: () => ended.push("here") });

    stopRelays();

    expect(ended).toEqual(["gone", "here"]);
    expect(openRelayCount()).toBe(0);
  });
});
