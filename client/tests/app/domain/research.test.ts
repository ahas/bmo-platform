import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";

import {
  clockText,
  defaultsText,
  defaultText,
  demandText,
  durationText,
  engineStateLabel,
  fieldFault,
  isStale,
  msOf,
  pingText,
  pooledRoles,
  profileLabel,
  reservationOwnerText,
  reservationText,
  roomText,
  sameValues,
  STALE_AFTER_MS,
  tierModelText,
  tierRunText,
  usageText,
  wholeOf,
  withField
} from "../../../apps/web/app/domain/research";

import type { ResearchProfileValues } from "../../../apps/web/shared/research";
import type { ChildRoom, EngineView, PoolTierView, ResearchProfileView, ResearchSettingsView } from "../../../apps/web/shared/research";

// A search profile, which carries no clock: both clock fields are empty on every search key.
const values: ResearchProfileValues = {
  capMs: null,
  synthesisReserveMs: null,
  childRole: "scout",
  maxChildren: 4,
  waitLimitMs: 600_000,
  strategy: "direct-then-browser",
  hostIntervalMs: 1_000,
  engineIntervalMs: 2_000
};

// Where a run of a key that opens branches opens them: a key whose room is none opens none.
const branches: ChildRoom = { state: "free", provider: "claude-code", tier: "Light", model: "claude-haiku-4-5", capacity: 12, active: 0, waiting: 0, free: 12, error: null };

function profile(over: Partial<ResearchProfileView> = {}): ResearchProfileView {
  return {
    ...values,
    key: "search:thorough",
    setAt: 0,
    seed: values,
    strategies: ["direct-then-browser", "browser-first", "direct-only"],
    tier: "Frontier",
    provider: "claude-code",
    model: "claude-opus-5",
    effort: "high",
    error: null,
    room: branches,
    ...over
  };
}

function settings(): ResearchSettingsView {
  return {
    readAt: 0,
    pool: {
      roles: [
        { name: "builder", description: "", enabled: true, pooled: false, control: true, tier: "Frontier", usedBy: [] },
        { name: "scout", description: "", enabled: true, pooled: true, control: false, tier: "Light", usedBy: [] },
        { name: "digest", description: "", enabled: false, pooled: true, control: false, tier: "Fine", usedBy: [] }
      ],
      tiers: [],
      capacityLimit: 128,
      defaults: { Reliable: 6, Fine: 6, Light: 12, Simple: 12 },
      coworkerTiers: ["Reliable", "Fine", "Light", "Simple"]
    },
    profiles: [profile()],
    limits: { capMs: { min: 30_000, max: 7_200_000 }, synthesisReserveMs: { min: 10_000 }, researchMs: { min: 10_000 }, maxChildren: { max: 16 }, waitLimitMs: { max: 3_600_000 }, intervalMs: { max: 60_000 } },
    engines: [],
    cooldowns: []
  };
}

describe("what the form checks before the bot does", () => {
  test("turns typed seconds into milliseconds and an empty field into a refused one", () => {
    expect(msOf("90")).toBe(90_000);
    expect(msOf("")).toBeNaN();
    expect(wholeOf("4")).toBe(4);
    expect(wholeOf("  ")).toBeNaN();
    expect(sameValues(values, { ...values })).toBe(true);
  });

  test("checks one field the way the bot checks the whole profile", () => {
    expect(withField(values, "maxChildren", 2)).toEqual({ ...values, maxChildren: 2 });
    // A clock on a key that has none, a role the pool does not lend out and a branch on basic are each refused on their own field.
    expect(fieldFault(wording, settings(), profile(), "capMs", 180_000)).toBe(wording("research.error.cap_no_clock"));
    expect(fieldFault(wording, settings(), profile(), "childRole", "digest")).toBe(wording("research.error.role_not_pooled", { role: "digest" }));
    expect(fieldFault(wording, settings(), profile({ key: "search:basic", room: null }), "maxChildren", 2)).toBe(wording("research.error.branchless"));
    expect(fieldFault(wording, settings(), profile(), "maxChildren", 2)).toBeNull();
    expect(fieldFault(wording, settings(), profile(), "hostIntervalMs", Number.NaN)).toBe(
      wording("research.error.host_interval_out_of_range", { max: wording("common.label.minutes", { m: 1 }) })
    );
    expect(pooledRoles(settings())).toEqual(["scout"]);
  });

  test("says what a profile's tier runs on, or why it cannot run", () => {
    expect(tierRunText(wording, profile())).toBe("claude-opus-5 · high");
    expect(tierRunText(wording, profile({ model: null, effort: null, error: "No enabled provider carries the search line" }))).toBe(wording("research.label.cannot_run", { error: "No enabled provider carries the search line" }));
  });
});

describe("the screen's sentences", () => {
  test("names a research key in the screen's words, and one it has no words for by the key itself", () => {
    expect(profileLabel(wording, "search:basic")).not.toBe("search:basic");
    expect(profileLabel(wording, "search:deeper")).toBe("search:deeper");
  });

  test("tells an optional search from an insisted one, and browse waits", () => {
    const full: ChildRoom = { state: "full", provider: "claude-code", tier: "Light", model: "claude-haiku", capacity: 12, active: 12, waiting: 1, free: 0, error: null };

    const usage = ` ${wording("research.label.room_usage", { active: "12", capacity: "12", waiting: "1" })}`;

    expect(roomText(wording, full, "search:thorough")).toBe(wording("research.message.room_full", { place: "Light(claude-haiku)", usage, optional: wording("research.message.optional_search") }));
    expect(roomText(wording, { ...full, state: "no-capacity" }, "search:thorough")).toBe(wording("research.message.room_no_capacity", { tier: "Light", optional: wording("research.message.optional_search") }));
    expect(roomText(wording, full, "browse")).toBe(wording("research.message.room_full", { place: "Light(claude-haiku)", usage, optional: wording("research.message.optional_browse") }));
    expect(roomText(wording, null, "search:basic")).toBe("");
    expect(demandText(wording, { maxChildren: 0, waitLimitMs: 600_000 }, "search:basic")).toBe(wording("research.message.demand_no_children"));
    expect(demandText(wording, values, "search:thorough")).toBe(wording("research.message.demand_search", { time: durationText(wording, values.waitLimitMs) }));
  });

  test("shows the launch-time capacity and model an admitted request still holds", () => {
    const place: PoolTierView = {
      provider: "claude-code", label: "Claude Code", tier: "Light", model: "claude-haiku", effort: "medium", error: null, capacity: 2, defaultCapacity: 12,
      snapshotCapacities: [2, 12], snapshotModels: [{ model: "claude-haiku", effort: "medium" }, { model: "claude-haiku", effort: "low" }], active: 1, reserved: 0, waiting: 3, roles: ["scout"],
      reservations: [], lastPing: null
    };

    // The figures one after another, each in its own entry.
    const figures = (...parts: [string, Record<string, string>?][]) => parts.map(([key, marks]) => wording(key, marks ?? {})).join(" · ");
    const startModel = wording("research.label.model_effort", { model: "claude-haiku", effort: "low" });

    expect(usageText(wording, place)).toBe(
      figures(["research.label.usage_running", { n: "1" }], ["research.label.usage_reserved", { n: "0" }], ["research.label.usage_waiting", { n: "3" }], ["research.label.usage_limit", { n: "2" }], ["research.label.usage_start_limit", { list: "12" }], ["research.label.usage_start_model", { list: startModel }]),
    );
    expect(usageText(wording, { ...place, capacity: 0, reserved: 2, snapshotCapacities: [], snapshotModels: [] }))
      .toBe(figures(["research.label.usage_running", { n: "1" }], ["research.label.usage_reserved", { n: "2" }], ["research.label.usage_waiting", { n: "3" }], ["research.label.usage_blocked"]));
    // The early work that yields its place is said apart among the running and the waiting.
    expect(usageText(wording, { ...place, active: 3, waiting: 2, snapshotCapacities: [], snapshotModels: [], yielding: { active: 1, waiting: 2 } }))
      .toBe(figures(["research.label.usage_running_yielding", { n: "3", yielding: "1" }], ["research.label.usage_reserved", { n: "0" }], ["research.label.usage_waiting_yielding", { n: "2", yielding: "2" }], ["research.label.usage_limit", { n: "2" }]));
    expect(usageText(wording, { ...place, snapshotCapacities: [], snapshotModels: [], yielding: { active: 0, waiting: 0 } }))
      .toBe(figures(["research.label.usage_running", { n: "1" }], ["research.label.usage_reserved", { n: "0" }], ["research.label.usage_waiting", { n: "3" }], ["research.label.usage_limit", { n: "2" }]));
    expect(defaultText(wording, place)).toBe(wording("research.label.default_capacity", { n: "12" }));
    expect(tierModelText(wording, { ...place, model: null, error: "Missing tier mapping: claude-code/Light" })).toContain("Missing tier mapping");
  });

  test("names who holds each reservation and says what its last ping came to", () => {
    const ping = { place: "cowork:repo:1", at: Date.UTC(2026, 8, 16, 3, 4, 5), outcome: "answered", answeredAt: null };

    const order = wording("research.label.owner_work_order", { id: "411" });
    const pinged = (outcome: string) => wording("research.label.last_ping", { at: clockText(ping.at), outcome });

    expect(reservationOwnerText(wording, "work-order:411")).toBe(order);
    expect(reservationOwnerText(wording, "search-run:12")).toBe(wording("research.label.owner_search_run", { id: "12" }));
    expect(reservationOwnerText(wording, "browse-run:3")).toBe(wording("research.label.owner_browse_run", { id: "3" }));
    expect(reservationOwnerText(wording, "native-cli:4242:991")).toBe(wording("research.label.owner_native_cli", { pid: "4242" }));
    expect(reservationText(wording, { owner: "work-order:411", source: "cowork", role: "scout", wanted: 3, held: 2, waiting: 1, createdAt: 1, lastPing: null }))
      .toBe(wording("research.label.reservation_queued", { owner: order, role: "scout", wanted: "3", held: "2", waiting: "1" }));
    expect(reservationText(wording, { owner: "search-run:12", source: "search", role: "scout", wanted: 4, held: 4, waiting: 0, createdAt: 1, lastPing: null }))
      .toBe(wording("research.label.reservation", { owner: wording("research.label.owner_search_run", { id: "12" }), role: "scout", wanted: "4", held: "4", waiting: "0" }));
    expect(pingText(wording, null)).toBe(wording("research.label.no_ping"));
    expect(pingText(wording, ping)).toBe(pinged(wording("research.label.ping_answered")));
    expect(pingText(wording, { ...ping, outcome: "busy" })).toBe(pinged(wording("research.label.ping_busy")));
    expect(pingText(wording, { ...ping, outcome: "failed: overran" })).toBe(pinged(wording("research.label.ping_failed", { reason: "overran" })));
    expect(pingText(wording, { ...ping, outcome: "refused: The local model publishes no cache lifetime." })).toBe(pinged(wording("research.label.ping_refused", { reason: "The local model publishes no cache lifetime." })));
  });

  test("names the starting values by tier", () => {
    expect(defaultsText({ Reliable: 6, Fine: 6, Light: 12, Simple: 12 })).toBe("Reliable·Fine 6, Light·Simple 12");
  });

  test("a rest whose time has passed does not read as a rest", () => {
    const engine: EngineView = { engine: "zapmeta", state: "suspended", reason: "CAPTCHA", observedAt: 1, suspendedUntil: 2, elapsedMs: 10, resultCount: 0, category: "general", korean: false, resting: false };

    expect(engineStateLabel(wording, engine)).toBe(wording("research.label.engine_rest_over"));
    expect(engineStateLabel(wording, { ...engine, resting: true })).toBe(wording("research.label.engine_suspended"));
  });

  test("calls saved values stale after a failed reread or a long silence", () => {
    expect(isStale(null, true, 10)).toBe(false);
    expect(isStale(1_000, true, 1_001)).toBe(true);
    expect(isStale(1_000, false, 1_000 + STALE_AFTER_MS + 1)).toBe(true);
    expect(isStale(1_000, false, 2_000)).toBe(false);
  });
});
