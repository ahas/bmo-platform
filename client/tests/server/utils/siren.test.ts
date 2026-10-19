import { afterEach, describe, expect, test } from "bun:test";

import { SIREN_MAX, SIREN_PAGE_SIZE } from "../../../apps/web/shared/config";
import { readSiren, sirenAgentOf, watchQuota } from "../../../apps/web/server/utils/siren";

const held = globalThis.fetch;

// The siren is reached at the address the setting names; every call is answered here instead.
process.env.CACHE_SIREN_ADDRESS = "http://siren.test";

afterEach(() => {
  globalThis.fetch = held;
});

// The watch's own answers, in the shape it really writes them: a tally under short names and a
// ring of requests newest last. Every address it was asked at is kept, since how far back the
// screen asked is part of what is checked here.
const asked: string[] = [];

function answering(stats: unknown, events: unknown): void {
  asked.length = 0;
  globalThis.fetch = ((input: string | URL | Request) => {
    const url = String(input);

    asked.push(url);

    const body = url.includes("/stats") ? stats : events;

    return Promise.resolve(new Response(JSON.stringify(body), { headers: { "content-type": "application/json" } }));
  }) as typeof fetch;
}

// A ring of n requests, oldest first, as the watch hands one back.
function ring(count: number): { events: { seq: number; kind: string }[] } {
  return { events: Array.from({ length: count }, (_, at) => ({ seq: at + 1, kind: "warm" })) };
}

const stats = {
  per_min: [0, 0, 1200, 4100],
  new_60s: 4100,
  sessions_30m: 3,
  uptime_s: 205,
  files: 45,
  last_event: "22:28:49",
  cfg: { yellow_new_tokens: 20000, red_new_tokens: 100000, burst_red_tokens: 500000 },
};

const events = {
  events: [
    {
      seq: 1,
      ts_kst: "22:24:45",
      epoch: 1_788_600_285.42,
      project: "home-user-checkouts-a",
      session: "f62a18a8",
      m: "O5",
      model: "claude-opus-5",
      kind: "warm",
      level: "info",
      note: "",
      new: 648,
      read: 814029,
    },
    { seq: 2, ts_kst: "22:28:49", kind: "cold", level: "red", note: "ttl_expired", new: 240000, read: 0 },
  ],
};

describe("where the watch is asked", () => {
  test("at the address the setting names, and nowhere where it names none", async () => {
    answering(stats, events);
    await readSiren();

    expect(asked.map((url) => url.split("?")[0])).toEqual(["http://siren.test/stats", "http://siren.test/events"]);

    delete process.env.CACHE_SIREN_ADDRESS;
    answering(stats, events);

    try {
      expect((await readSiren()).reachable).toBe(false);
      expect(asked).toEqual([]);
    } finally {
      process.env.CACHE_SIREN_ADDRESS = "http://siren.test";
    }
  });
});

describe("what the watch's answer becomes", () => {
  test("the tally the screen stands on, under the names the screen uses", async () => {
    answering(stats, events);

    const watch = await readSiren();

    expect(watch).toMatchObject({
      reachable: true,
      files: 45,
      lastEvent: "22:28:49",
      uptimeSeconds: 205,
      newLastMinute: 4100,
      sessions30m: 3,
      perMinute: [0, 0, 1200, 4100],
      thresholds: { yellow: 20000, red: 100000, burst: 500000 },
    });
  });

  test("one request, with the short model name and the tokens either way", async () => {
    answering(stats, events);

    const [newest, older] = (await readSiren()).events;

    // Newest first: the watch hands its ring oldest last, and the screen reads downward.
    expect(newest).toEqual({
      seq: 2,
      at: "22:28:49",
      // The watch sent no second for this one, so the screen has none to carry.
      atSeconds: null,
      project: "",
      session: "",
      agent: null,
      mode: "claude-code",
      model: "",
      kind: "cold",
      level: "red",
      note: "ttl_expired",
      // What moved a prefix is the agency's record, served by no route, so no request carries a cause.
      cause: null,
      newTokens: 240000,
      readTokens: 0,
    });
    expect(older).toMatchObject({
      seq: 1,
      mode: "claude-code",
      model: "claude-opus-5",
      kind: "warm",
      newTokens: 648,
      readTokens: 814029,
    });
    // The second the request was made, whole: what carries a press on the row into the log.
    expect(older?.atSeconds).toBe(1_788_600_285);
  });

  test("a field the watch did not send says nothing rather than breaking the screen", async () => {
    answering({}, { events: [{ seq: 3 }] });

    const watch = await readSiren();

    expect(watch.reachable).toBe(true);
    expect(watch.thresholds).toEqual({ yellow: 0, red: 0, burst: 0 });
    expect(watch.events[0]).toMatchObject({ seq: 3, at: "", kind: "", newTokens: 0 });
  });

  test("a watch that is not there leaves the screen readable and empty", async () => {
    globalThis.fetch = (() => Promise.reject(new Error("ECONNREFUSED"))) as unknown as typeof fetch;

    expect(await readSiren()).toMatchObject({ reachable: false, events: [], files: 0 });
  });

  test("and so does one that answers with an error", async () => {
    globalThis.fetch = (() => Promise.resolve(new Response("nope", { status: 500 }))) as unknown as typeof fetch;

    expect((await readSiren()).reachable).toBe(false);
  });
});

describe("which agent a request belongs to", () => {
  test("a row a caller posted names its agent", () => {
    expect(sirenAgentOf({ agent: "agent-a", project: "p" })).toBe("agent-a");
    expect(sirenAgentOf({ agent: true, project: "p" })).toBeNull();
  });

  test("a runner is known by the directory of its own name it works in", () => {
    expect(sirenAgentOf({ project: "home-user-agents-search", cwd: "search" })).toBe("search");
    expect(sirenAgentOf({ project: "home-user-agents-browse", cwd: "browse" })).toBe("browse");
  });

  test("a directory of that name inside a checkout is not a runner", () => {
    expect(sirenAgentOf({ project: "home-user-checkouts-a", cwd: "search" })).toBeNull();
  });

  test("anything else is left unnamed rather than guessed at, since which agent holds a session is served by no route", () => {
    expect(sirenAgentOf({})).toBeNull();
    expect(sirenAgentOf({ project: "home-user-checkouts-a", cwd: "a" })).toBeNull();
  });
});

// Scrolling to the foot of the list is what asks for more, and asking for more is asking the watch
// for a bigger slice of its ring: it keeps a ring rather than a history and hands back the newest
// of it, so there is no cursor to read from. What is held here is that the ask really grows, that
// the answer says whether there is more behind it, and that the end of the ring is an end.
describe("how far back the feed is read", () => {
  test("a page at a time, and the first ask is one page", async () => {
    answering(stats, ring(SIREN_PAGE_SIZE));

    await readSiren();

    expect(asked.some((url) => url.includes(`/events?limit=${SIREN_PAGE_SIZE}`))).toBe(true);
  });

  test("and the screen may ask for more than one", async () => {
    answering(stats, ring(10));

    await readSiren(SIREN_PAGE_SIZE * 3);

    expect(asked.some((url) => url.includes(`/events?limit=${SIREN_PAGE_SIZE * 3}`))).toBe(true);
  });

  // The watch handed back as many as it was asked for, so there is more of its ring behind them.
  test("says there is more where it gave back as many as it was asked for", async () => {
    answering(stats, ring(SIREN_PAGE_SIZE));

    expect((await readSiren(SIREN_PAGE_SIZE)).hasMore).toBe(true);
  });

  // Fewer than asked is the ring itself ending, and there is nothing further to ask for.
  test("and says there is none where it gave back fewer", async () => {
    answering(stats, ring(SIREN_PAGE_SIZE - 1));

    expect((await readSiren(SIREN_PAGE_SIZE)).hasMore).toBe(false);
  });

  // The ring the watch keeps is the ceiling. An ask past it is cut back to it, and standing at it
  // is standing at the end however full the answer was.
  test("never asks for more of the ring than the watch keeps", async () => {
    answering(stats, ring(SIREN_MAX));

    const watch = await readSiren(SIREN_MAX * 4);

    expect(asked.some((url) => url.includes(`/events?limit=${SIREN_MAX}`))).toBe(true);
    expect(watch.hasMore).toBe(false);
  });

  test("a watch that does not answer has nothing more to give", async () => {
    globalThis.fetch = (() => Promise.reject(new Error("not there"))) as unknown as typeof fetch;

    const watch = await readSiren();

    expect(watch.reachable).toBe(false);
    expect(watch.hasMore).toBe(false);
    expect(watch.events).toEqual([]);
  });

  // Every row keeps the watch's own sequence number and the list is drawn by it, so a feed read
  // again at a bigger size is the same rows with older ones behind them rather than a second copy.
  test("what comes back is one feed, newest first, with no row twice", async () => {
    answering(stats, ring(SIREN_PAGE_SIZE * 2));

    const watch = await readSiren(SIREN_PAGE_SIZE * 2);
    const seqs = watch.events.map((event) => event.seq);

    expect(seqs).toHaveLength(SIREN_PAGE_SIZE * 2);
    expect(new Set(seqs).size).toBe(seqs.length);
    expect(seqs[0]).toBeGreaterThan(seqs[seqs.length - 1] as number);
  });
});

// The watch reads the slot its source marked active, and the screen says whose figures they are.
describe("the quota the watch read", () => {
  test("carries the active login's name, its provider and how many logins it holds", () => {
    expect(watchQuota({ five_h: 30, five_h_resets: 100, seven_d: 58, seven_d_resets: 200, tier: "main", provider: "claude-code", accounts: 2 })).toEqual({
      fiveHour: 30,
      fiveHourResetsAt: 100,
      weekly: 58,
      weeklyResetsAt: 200,
      account: "main",
      provider: "claude-code",
      accounts: 2,
    });
  });

  test("a source that names nobody leaves the name blank rather than inventing one", () => {
    expect(watchQuota({ five_h: 12, tier: "", provider: "somewhere" })).toMatchObject({ fiveHour: 12, account: null, provider: null, accounts: null });
    expect(watchQuota(undefined)).toEqual({
      fiveHour: null,
      fiveHourResetsAt: null,
      weekly: null,
      weeklyResetsAt: null,
      account: null,
      provider: null,
      accounts: null,
    });
  });

  test("the screen's watch carries it", async () => {
    answering({ quota: { five_h: 30, tier: "main", provider: "claude-code", accounts: 2 } }, ring(0));

    expect((await readSiren()).quota).toMatchObject({ fiveHour: 30, account: "main", accounts: 2 });
  });
});
