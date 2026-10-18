import { describe, expect, test } from "bun:test";
import { mkdirSync, readdirSync, utimesSync } from "node:fs";
import { join } from "node:path";

import { scratchDir } from "../../support/scratch";

import { fail } from "../../../apps/web/server/utils/fail";
import { unitSlow } from "../../../apps/web/server/utils/units";
import { answerSend, claimSend, failSend, readSend, releaseSend, SEND_KEY_SECONDS, sendKeyOf, sendOnce, sweepSends } from "../../../apps/web/server/utils/send-keys";

// What Nitro would import into the module by itself.
const globals = globalThis as Record<string, unknown>;

globals.createError = (options: { statusCode: number; message: string }) => Object.assign(new Error(options.message), options);
globals.fail = fail;
globals.unitSlow = unitSlow;

const root = scratchDir("bmo-send-keys-");
let drawn = 0;

// A key of the shape the screen draws, new for every test.
function key(): string {
  drawn += 1;

  return `key-${String(drawn).padStart(16, "0")}`;
}

describe("a key under its claim", () => {
  test("is fresh once, then going until its send answers, then that answer", async () => {
    const one = key();

    expect(await claimSend(7, one, root)).toEqual({ kind: "fresh" });
    expect(await claimSend(7, one, root)).toEqual({ kind: "going" });

    await answerSend(7, one, { messageId: 42 }, root);

    expect(await claimSend(7, one, root)).toEqual({ kind: "answered", answer: { messageId: 42 } });
  });

  test("of two tries at once, only one is fresh", async () => {
    const one = key();
    const claims = await Promise.all([claimSend(7, one, root), claimSend(7, one, root)]);

    expect(claims.map((claim) => claim.kind).sort()).toEqual(["fresh", "going"]);
  });

  test("is a room's own: the same key in another room is another send", async () => {
    const one = key();

    await claimSend(7, one, root);

    expect(await claimSend(8, one, root)).toEqual({ kind: "fresh" });
  });

  test("released, is fresh again", async () => {
    const one = key();

    await claimSend(7, one, root);
    await releaseSend(7, one, root);

    expect(await claimSend(7, one, root)).toEqual({ kind: "fresh" });
  });

  test("is swept once it is older than it is kept, and not before", async () => {
    const swept = join(root, "swept");

    mkdirSync(swept);
    const [old, young] = [key(), key()];
    const now = Date.now();

    await claimSend(7, old, swept, now);
    await claimSend(7, young, swept, now);
    utimesSync(join(swept, `7_${old}.json`), new Date(now - SEND_KEY_SECONDS * 1000), new Date(now - SEND_KEY_SECONDS * 1000));
    await sweepSends(swept, now);

    expect(readdirSync(swept)).toEqual([`7_${young}.json`]);
  });
});

describe("what a send carries as its key", () => {
  test("nothing, or a key of the screen's shape", () => {
    expect(sendKeyOf(undefined)).toBeNull();
    expect(sendKeyOf("")).toBeNull();
    expect(sendKeyOf("0123456789abcdef0123456789abcdef")).toBe("0123456789abcdef0123456789abcdef");
  });

  test("anything else is refused, rather than sent unguarded", () => {
    for (const bad of ["short", "../../etc/passwd-0000000000", 42, "a key with spaces in it"]) {
      expect(() => sendKeyOf(bad)).toThrow();
    }
  });
});

describe("a send under its key", () => {
  const going = () => "going" as const;

  test("reaches the bot once however often it is tried, and every try hands back its answer", async () => {
    const one = key();
    let sent = 0;
    const send = async () => {
      sent += 1;

      return `message ${sent}`;
    };

    expect(await sendOnce(7, one, send, going, root)).toBe("message 1");
    expect(await sendOnce(7, one, send, going, root)).toBe("message 1");
    expect(sent).toBe(1);
  });

  test("tried while the first try is still going answers as going, and sends nothing", async () => {
    const one = key();
    let finish: (value: string) => void = () => undefined;
    let sent = 0;
    let started: () => void = () => undefined;
    const sending = new Promise<void>((resolve) => (started = resolve));
    const first = sendOnce(7, one, () => {
      sent += 1;
      started();

      return new Promise<string>((resolve) => (finish = resolve));
    }, going, root);

    // The first try's claim is made before its send starts, however long its files take.
    await sending;

    expect(await sendOnce(7, one, async () => "twice", going, root)).toBe("going");
    finish("once");
    expect(await first).toBe("once");
    expect(sent).toBe(1);
  });

  test("that the bot did not take frees its key, so the next try sends", async () => {
    const one = key();

    await expect(sendOnce(7, one, () => Promise.reject(fail(503, "no bot")), going, root)).rejects.toThrow("no bot");
    expect(await sendOnce(7, one, async () => "sent", going, root)).toBe("sent");
  });

  test("that ran late keeps its key claimed: it may well have gone", async () => {
    const one = key();

    await expect(sendOnce(7, one, () => Promise.reject(fail(504, "too late")), going, root)).rejects.toThrow("too late");
    expect(await sendOnce(7, one, async () => "twice", going, root)).toBe("going");
  });

  test("with no key is simply sent, every time", async () => {
    let sent = 0;
    const send = async () => (sent += 1);

    await sendOnce(7, null, send, () => 0, root);
    await sendOnce(7, null, send, () => 0, root);

    expect(sent).toBe(2);
  });
});

// A key's record as it settles once a send listened for late has come back, asked a bounded number of times.
async function settledRecord(chatId: number, one: string): Promise<Awaited<ReturnType<typeof readSend>>> {
  for (let tries = 0; tries < 200; tries += 1) {
    const record = await readSend(chatId, one, root);

    if (record.state !== "going") {
      return record;
    }

    await Bun.sleep(1);
  }

  return readSend(chatId, one, root);
}

describe("what a key came to, read without claiming it", () => {
  test("nothing for a key never handed over, going while claimed, then its answer", async () => {
    const one = key();

    expect(await readSend(7, one, root)).toEqual({ state: "unknown" });
    await claimSend(7, one, root);
    expect(await readSend(7, one, root)).toEqual({ state: "going" });
    await answerSend(7, one, { messageId: 42 }, root);
    expect(await readSend(7, one, root)).toEqual({ state: "answered", answer: { messageId: 42 } });
  });

  test("a refusal written under it reads as failed, and frees the key for the next try", async () => {
    const one = key();

    await claimSend(7, one, root);
    await failSend(7, one, "refused", root);

    expect(await readSend(7, one, root)).toEqual({ state: "failed", reason: "refused" });
    expect(await claimSend(7, one, root)).toEqual({ kind: "fresh" });
  });
});

describe("a send the screen stopped waiting for", () => {
  const going = () => "going" as const;

  // A send held until the test lets it answer, past a screen given no time at all.
  function held(): { send: () => Promise<string>; finish: (answer: string) => void; refuse: (error: Error) => void } {
    let finish: (answer: string) => void = () => undefined;
    let refuse: (error: Error) => void = () => undefined;
    const answer = new Promise<string>((resolve, reject) => {
      finish = resolve;
      refuse = reject;
    });

    return { send: () => answer, finish: (one) => finish(one), refuse: (error) => refuse(error) };
  }

  test("answers as going, and writes what it went out as once it comes back", async () => {
    const one = key();
    const send = held();

    expect(await sendOnce(7, one, send.send, going, root, 0)).toBe("going");
    expect(await readSend(7, one, root)).toEqual({ state: "going" });

    send.finish("message 9");

    expect(await settledRecord(7, one)).toEqual({ state: "answered", answer: "message 9" });
    expect(await sendOnce(7, one, async () => "twice", going, root, 0)).toBe("message 9");
  });

  test("writes the refusal that came instead", async () => {
    const one = key();
    const send = held();

    expect(await sendOnce(7, one, send.send, going, root, 0)).toBe("going");
    send.refuse(fail(409, "refused"));

    expect(await settledRecord(7, one)).toEqual({ state: "failed", reason: "refused" });
  });

  test("that ran past even the app's own wait stays claimed: it may well have gone", async () => {
    const one = key();
    const send = held();

    expect(await sendOnce(7, one, send.send, going, root, 0)).toBe("going");
    send.refuse(fail(504, "late"));
    await Bun.sleep(5);

    expect(await readSend(7, one, root)).toEqual({ state: "going" });
  });

  test("answered within the screen's time is answered as ever", async () => {
    expect(await sendOnce(7, key(), async () => "message 1", going, root, 60_000)).toBe("message 1");
  });
});
