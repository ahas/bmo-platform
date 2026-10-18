import { faultText, hasResearchClock, researchFieldFaults } from "#shared/research";
import { spanClock } from "bmo-ui/app/domain/time";

import type { ResearchField, ResearchKey, ResearchProfileValues } from "#shared/research";
import type { CoworkerTier } from "#shared/agency";
import type { ChildRoom, EngineView, PoolPingView, PoolReservationView, PoolTierView, ResearchProfileView, ResearchSettingsView } from "#shared/research";
import type { Translate } from "#shared/wording";

// What the agent-management screen says about the common pool, the research profiles and the
// engines, and how one profile field is checked before it is sent (docs/web.md).

export const profileFields: readonly ResearchField[] = [
  "capMs", "synthesisReserveMs", "childRole", "maxChildren", "waitLimitMs", "strategy", "hostIntervalMs", "engineIntervalMs"
];

// Each profile's name and each fetching strategy's, by the key of the words they are said in.
export const PROFILE_LABELS: Record<ResearchKey, string> = {
  "search:basic": "research.label.profile_search_basic",
  "search:thorough": "research.label.profile_search_thorough",
  "search:exhaustive": "research.label.profile_search_exhaustive",
  browse: "research.label.profile_browse"
};

const STRATEGY_LABELS: Record<string, string> = {
  "direct-then-browser": "research.label.strategy_direct_then_browser",
  "browser-first": "research.label.strategy_browser_first",
  "direct-only": "research.label.strategy_direct_only",
  visible: "research.label.strategy_visible",
  headless: "research.label.strategy_headless"
};

// A research key this screen has no words for stands under the key the bot gave it.
export function profileLabel(t: Translate, key: ResearchKey): string {
  const label = PROFILE_LABELS[key];

  return label === undefined ? key : t(label);
}

// A strategy this screen has no words for stands under the name the bot gave it.
export function strategyLabel(t: Translate, strategy: string): string {
  const key = STRATEGY_LABELS[strategy];

  return key === undefined ? strategy : t(key);
}

// Saved values read after this long without a successful reread are called stale on screen.
export const STALE_AFTER_MS = 60_000;

export function profileValues(profile: ResearchProfileValues): ResearchProfileValues {
  return {
    capMs: profile.capMs,
    synthesisReserveMs: profile.synthesisReserveMs,
    childRole: profile.childRole,
    maxChildren: profile.maxChildren,
    waitLimitMs: profile.waitLimitMs,
    strategy: profile.strategy,
    hostIntervalMs: profile.hostIntervalMs,
    engineIntervalMs: profile.engineIntervalMs
  };
}

export function sameValues(one: ResearchProfileValues, other: ResearchProfileValues): boolean {
  return profileFields.every((field) => one[field] === other[field]);
}

// The roles a profile may open branches with: enabled and lent out by the pool.
export function pooledRoles(settings: ResearchSettingsView): string[] {
  return settings.pool.roles.filter((role) => role.enabled && role.pooled).map((role) => role.name);
}

// A profile with one field changed, and what bmo-artifact would say is wrong with that field, checked the way it
// checks the whole profile.
export function withField(profile: ResearchProfileValues, field: ResearchField, value: ResearchProfileValues[ResearchField]): ResearchProfileValues {
  return { ...profileValues(profile), [field]: value };
}

export function fieldFault(t: Translate, settings: ResearchSettingsView, profile: ResearchProfileView, field: ResearchField, value: ResearchProfileValues[ResearchField]): string | null {
  const values = withField(profile, field, value);
  const fault = researchFieldFaults(profile, values, settings, pooledRoles(settings)).find((one) => one.field === field);

  return fault === undefined ? null : faultText(t, fault, settings.limits, values);
}

// The form edits whole seconds; the bot keeps milliseconds. An empty or broken field stays a NaN,
// which the field checks refuse with that field's own sentence.
export function secondsOf(ms: number): number {
  return ms / 1000;
}

export function msOf(seconds: number | string | null | undefined): number {
  const value = typeof seconds === "string" ? (seconds.trim() === "" ? Number.NaN : Number(seconds)) : seconds ?? Number.NaN;

  return Number.isFinite(value) ? Math.round(value * 1000) : Number.NaN;
}

export function wholeOf(value: number | string | null | undefined): number {
  const held = typeof value === "string" ? (value.trim() === "" ? Number.NaN : Number(value)) : value ?? Number.NaN;

  return Number.isFinite(held) ? held : Number.NaN;
}

export function durationText(t: Translate, ms: number): string {
  if (!Number.isFinite(ms)) return "—";
  const seconds = Math.round(ms / 1000);

  if (seconds < 60) return t("common.label.seconds", { n: seconds });
  if (seconds >= 3600) return spanClock(t, seconds);
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;

  return rest === 0 ? t("common.label.minutes", { m: minutes }) : t("common.label.minutes_seconds", { m: minutes, s: rest });
}

// What the clock of a profile that has one comes to. A profile with none says so instead: the
// run goes until it has written its answer and nothing closes it at a time.
export function researchWindowText(t: Translate, values: Pick<ResearchProfileValues, "capMs" | "synthesisReserveMs">): string {
  const { capMs, synthesisReserveMs } = values;

  if (capMs === null || synthesisReserveMs === null) return t("research.label.no_clock");
  const research = capMs - synthesisReserveMs;

  return Number.isFinite(research) && research > 0
    ? t("research.label.window", { research: durationText(t, research), synthesis: durationText(t, synthesisReserveMs) })
    : "—";
}

// The clock line the card's head carries: the whole time and what it is split into, or the one
// sentence that says there is no clock at all.
export function profileClockText(t: Translate, values: Pick<ResearchProfileValues, "capMs" | "synthesisReserveMs">): string {
  return values.capMs === null
    ? researchWindowText(t, values)
    : `${durationText(t, values.capMs)} (${researchWindowText(t, values)})`;
}

export { hasResearchClock };

// What a profile's tier launches with on the provider its run starts on, or why it cannot run.
export function tierRunText(t: Translate, profile: Pick<ResearchProfileView, "model" | "effort" | "provider" | "error">): string {
  if (profile.error !== null) return t("research.label.cannot_run", { error: profile.error });

  return modelText(t, profile.model ?? t("research.label.no_model"), profile.effort);
}

// What an optional request and an insisted one do for this profile, in the owner's words.
export function demandText(t: Translate, values: Pick<ResearchProfileValues, "maxChildren" | "waitLimitMs">, key: ResearchKey): string {
  if (values.maxChildren === 0) return t("research.message.demand_no_children");
  if (key === "browse") return t("research.message.demand_browse", { time: durationText(t, values.waitLimitMs) });

  return t("research.message.demand_search", { time: durationText(t, values.waitLimitMs) });
}

export function roomText(t: Translate, room: ChildRoom | null, key: ResearchKey): string {
  if (room === null) return "";

  const usage =
    room.capacity === null ? "" : ` ${t("research.label.room_usage", { active: room.active, capacity: room.capacity, waiting: room.waiting })}`;
  const optional = key === "browse" ? t("research.message.optional_browse") : t("research.message.optional_search");

  const place = room.tier === null ? String(room.model) : `${room.tier}(${room.model})`;

  switch (room.state) {
    case "free":
      return t("research.message.room_free", { place, free: room.free, usage });
    case "full":
      return t("research.message.room_full", { place, usage, optional });
    case "no-capacity":
      return t("research.message.room_no_capacity", { tier: room.tier ?? t("research.label.branch_tier"), optional });
    case "role-off":
      return t("research.message.room_role_off", { optional });
    case "unresolved":
      return t("research.message.room_unresolved", { error: room.error ?? t("common.label.no_reason") });
  }
}

function modelText(t: Translate, model: string, effort: string | null): string {
  return t("research.label.model_effort", { model, effort: effort ?? t("common.label.no_effort") });
}

// The model a tier resolves to now, which the limit does not depend on.
export function tierModelText(t: Translate, place: Pick<PoolTierView, "model" | "effort" | "error">): string {
  return place.model === null
    ? t("research.label.model_unresolved", { error: place.error ?? t("common.label.no_reason") })
    : modelText(t, place.model, place.effort);
}

// How a tier's places stand, one figure after another, with where the tier stood when the pool
// started where that differs from now.
export function usageText(t: Translate, place: PoolTierView): string {
  const capacities = place.snapshotCapacities.filter((capacity) => capacity !== place.capacity);
  const models = place.snapshotModels.filter((one) => one.model !== place.model || one.effort !== place.effort);

  // Among the running and the waiting, the requests that yield their place are said apart, where
  // there are any.
  const running = place.yielding?.active ?? 0;
  const waiting = place.yielding?.waiting ?? 0;

  return [
    running > 0 ? t("research.label.usage_running_yielding", { n: place.active, yielding: running }) : t("research.label.usage_running", { n: place.active }),
    t("research.label.usage_reserved", { n: place.reserved }),
    waiting > 0 ? t("research.label.usage_waiting_yielding", { n: place.waiting, yielding: waiting }) : t("research.label.usage_waiting", { n: place.waiting }),
    place.capacity === 0 ? t("research.label.usage_blocked") : t("research.label.usage_limit", { n: place.capacity }),
    capacities.length > 0 ? t("research.label.usage_start_limit", { list: capacities.join(", ") }) : null,
    models.length > 0 ? t("research.label.usage_start_model", { list: models.map((one) => modelText(t, one.model, one.effort)).join(", ") }) : null
  ]
    .filter((part): part is string => part !== null)
    .join(" · ");
}

// Who holds a reservation, in the words the owner knows the holder by. The owner is the pool's own
// record of the caller, so it is read here the way the pool wrote it.
export function reservationOwnerText(t: Translate, owner: string): string {
  const order = /^work-order:(\d+)$/.exec(owner);
  if (order) return t("research.label.owner_work_order", { id: order[1]! });
  const run = /^(search|browse)-run:(\d+)$/.exec(owner);
  if (run) return run[1] === "search" ? t("research.label.owner_search_run", { id: run[2]! }) : t("research.label.owner_browse_run", { id: run[2]! });
  const cli = /^native-cli:(\d+):/.exec(owner);
  if (cli) return t("research.label.owner_native_cli", { pid: cli[1]! });
  return owner;
}

export function reservationText(t: Translate, reservation: PoolReservationView): string {
  const figures = {
    owner: reservationOwnerText(t, reservation.owner),
    role: reservation.role,
    wanted: reservation.wanted,
    held: reservation.held,
    waiting: reservation.waiting
  };

  return reservation.waiting > 0 ? t("research.label.reservation_queued", figures) : t("research.label.reservation", figures);
}

// What the last ping to a reserved place came to, with the reason where it did not warm anything.
export function pingText(t: Translate, ping: PoolPingView | null): string {
  if (ping === null) return t("research.label.no_ping");
  const reason = ping.outcome.slice(ping.outcome.indexOf(": ") + 2);
  const outcome = ping.outcome === "answered"
    ? t("research.label.ping_answered")
    : ping.outcome === "busy"
      ? t("research.label.ping_busy")
      : ping.outcome.startsWith("failed: ")
        ? t("research.label.ping_failed", { reason })
        : ping.outcome.startsWith("refused: ")
          ? t("research.label.ping_refused", { reason })
          : ping.outcome;
  return t("research.label.last_ping", { at: clockText(ping.at), outcome });
}

export function defaultText(t: Translate, place: Pick<PoolTierView, "defaultCapacity">): string {
  return t("research.label.default_capacity", { n: place.defaultCapacity });
}

// The starting values as one sentence, tiers of the same value together in the tier order.
export function defaultsText(defaults: Readonly<Record<CoworkerTier, number>>): string {
  const groups = new Map<number, CoworkerTier[]>();

  for (const [tier, capacity] of Object.entries(defaults) as [CoworkerTier, number][]) {
    groups.set(capacity, [...(groups.get(capacity) ?? []), tier]);
  }

  return [...groups].map(([capacity, tiers]) => `${tiers.join("·")} ${capacity}`).join(", ");
}

const CATEGORY_LABELS: Record<string, string> = {
  general: "research.label.category_general",
  news: "research.label.category_news",
  images: "research.label.category_images",
  it: "research.label.category_it"
};

// A category this screen has no words for stands under the name the bot gave it.
export function categoryLabel(t: Translate, category: string): string {
  const key = CATEGORY_LABELS[category];

  return key === undefined ? category : t(key);
}

const ENGINE_STATE_LABELS: Record<EngineView["state"], string> = {
  healthy: "research.label.engine_healthy",
  degraded: "research.label.engine_degraded",
  suspended: "research.label.engine_suspended",
  unknown: "research.label.engine_unknown"
};

// A suspension whose time has passed says so rather than still reading as a rest.
export function engineStateLabel(t: Translate, engine: EngineView): string {
  if (engine.state === "suspended" && !engine.resting) return t("research.label.engine_rest_over");

  return t(ENGINE_STATE_LABELS[engine.state]);
}

export function engineIcon(engine: EngineView): string {
  if (engine.state === "healthy") return "i-material-symbols-check-circle-outline";
  if (engine.state === "suspended" && engine.resting) return "i-material-symbols-pause-circle-outline";
  if (engine.state === "degraded") return "i-material-symbols-warning-outline";

  return "i-material-symbols-help-outline";
}

export function clockText(at: number | null): string {
  if (at === null) return "—";

  return new Date(at).toLocaleString("ko-KR", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

export function engineDetail(t: Translate, engine: EngineView): string {
  if (engine.resting) {
    return t("research.label.engine_resting", { until: clockText(engine.suspendedUntil), reason: engine.reason ?? t("common.label.no_reason") });
  }

  if (engine.observedAt === null) return t("research.label.engine_never_checked");

  const parts = [
    t("research.label.engine_last_check", { at: clockText(engine.observedAt) }),
    engine.elapsedMs === null ? null : durationText(t, engine.elapsedMs),
    engine.resultCount === null ? null : t("research.label.engine_results", { n: engine.resultCount }),
    engine.reason
  ];

  return parts.filter((part): part is string => part !== null && part !== "").join(" · ");
}

export function isStale(readAt: number | null, failedSince: boolean, now: number): boolean {
  return readAt !== null && (failedSince || now - readAt > STALE_AFTER_MS);
}
