import type { Translate } from "./wording";
import type { CoworkerTier, TierProvider } from "./agency";

// The research settings as bmo-artifact answers them (bmo-artifact's `docs/api.md`, 「Research settings」): the
// common co-worker pool, the research profiles and the search engines the agents screen shows, and the checks
// its form runs on a profile before it is sent. The form's checks read the bounds, the strategies and the
// clocks the answer carries; bmo-artifact's own refusal stays the verdict, and its faults are said in the same
// words (「How a fault is said」 below).

export type ResearchKey = string;

export interface ResearchProfileValues {
  // The clock, which only a key that has one carries. Null on the rest: a search runs until it has written
  // its result and is never cut off at a time.
  capMs: number | null;
  synthesisReserveMs: number | null;
  childRole: string;
  maxChildren: number;
  waitLimitMs: number;
  strategy: string;
  hostIntervalMs: number;
  engineIntervalMs: number;
}

export type ResearchField = keyof ResearchProfileValues;

// A role as the screen names it: which tier it is borrowed at and who opens branches with it.
export interface PoolRoleView {
  name: string;
  description: string;
  enabled: boolean;
  pooled: boolean;
  // The worker's own control role: the worker itself, never borrowed from the pool.
  control: boolean;
  tier: string | null;
  // Research profiles that open branches with this role.
  usedBy: ResearchKey[];
}

export interface PoolPingView {
  place: string;
  at: number;
  outcome: string;
  answeredAt: number | null;
}

// One held reservation as the screen shows it: who holds it, what it asked for, what it has.
export interface PoolReservationView {
  owner: string;
  source: string;
  role: string;
  wanted: number;
  // Seats let in and held, and seats still in the line.
  held: number;
  waiting: number;
  createdAt: number;
  lastPing: PoolPingView | null;
}

export interface PoolTierView {
  provider: TierProvider;
  // The provider's own name for itself, as its column is headed.
  label: string;
  tier: string;
  // The tier's current mapping on that provider, shown only; the limit does not depend on it.
  model: string | null;
  effort: string | null;
  error: string | null;
  capacity: number;
  defaultCapacity: number;
  snapshotCapacities: number[];
  snapshotModels: { model: string; effort: string | null }[];
  active: number;
  reserved: number;
  waiting: number;
  // Of the running and waiting ones, the requests that yield their place.
  yielding?: { active: number; waiting: number };
  // Enabled side roles assigned this tier.
  roles: string[];
  // The reservations standing in this tier, and the latest ping any of them sent.
  reservations: PoolReservationView[];
  lastPing: PoolPingView | null;
}

export type ChildRoomState = "free" | "full" | "no-capacity" | "role-off" | "unresolved";

export interface ChildRoom {
  state: ChildRoomState;
  provider: TierProvider | null;
  tier: string | null;
  model: string | null;
  capacity: number | null;
  active: number;
  waiting: number;
  free: number;
  error: string | null;
}

export interface ResearchProfileView extends ResearchProfileValues {
  key: ResearchKey;
  setAt: number;
  seed: ResearchProfileValues;
  strategies: readonly string[];
  // The key's tier, and what it launches with on the provider the run would start on.
  tier: string | null;
  provider: TierProvider | null;
  model: string | null;
  effort: string | null;
  error: string | null;
  // Where a run of it opens branches; null for a key that opens none.
  room: ChildRoom | null;
}

export type WebEngineState = "healthy" | "degraded" | "suspended" | "unknown";

export interface EngineView {
  engine: string;
  state: WebEngineState;
  reason: string | null;
  observedAt: number | null;
  suspendedUntil: number | null;
  elapsedMs: number | null;
  resultCount: number | null;
  category: string;
  korean: boolean;
  // A suspension whose time has passed is not a rest any more, whatever the row still says.
  resting: boolean;
}

// The bounds the form checks a profile against.
export interface ResearchLimits {
  capMs: { min: number; max: number };
  synthesisReserveMs: { min: number };
  researchMs: { min: number };
  maxChildren: { max: number };
  waitLimitMs: { max: number };
  intervalMs: { max: number };
}

export interface ResearchSettingsView {
  readAt: number;
  pool: {
    roles: PoolRoleView[];
    tiers: PoolTierView[];
    capacityLimit: number;
    defaults: Readonly<Record<CoworkerTier, number>>;
    coworkerTiers: readonly CoworkerTier[];
  };
  profiles: ResearchProfileView[];
  limits: ResearchLimits;
  engines: EngineView[];
  cooldowns: { reason: string; ms: number }[];
  // Where the search engines could not be read: the sentence why, with the engines and cooldowns empty.
  enginesError?: string;
}

// ## How a fault is said
//
// A fault is a field and its code, the codes bmo-artifact answers a profile's refusal with; each is said by
// the one sentence the pack holds for it, filled with the bound it was held to.

export type ResearchFaultCode =
  | "no_clock"
  | "cap_out_of_range"
  | "reserve_too_short"
  | "research_too_short"
  | "children_out_of_range"
  | "branchless"
  | "bad_role"
  | "role_not_pooled"
  | "wait_out_of_range"
  | "bad_strategy"
  | "interval_out_of_range";

export interface FieldFault {
  field: ResearchField;
  code: ResearchFaultCode;
}

export const roleNameShape = /^[a-z][a-z0-9_-]{0,63}$/;

const MINUTE_MS = 60_000;

// A bound said in whole minutes where it is one, and in seconds where it is not.
function spanText(t: Translate, ms: number): string {
  return ms % MINUTE_MS === 0 ? t("common.label.minutes", { m: ms / MINUTE_MS }) : t("common.label.seconds", { n: ms / 1000 });
}

/** The sentence a fault is said in, filled with the bound the profile was held to. */
export function faultText(t: Translate, fault: FieldFault, limits: ResearchLimits, values: ResearchProfileValues): string {
  switch (fault.code) {
    case "no_clock":
      return fault.field === "capMs" ? t("research.error.cap_no_clock") : t("research.error.reserve_no_clock");
    case "cap_out_of_range":
      return t("research.error.cap_out_of_range", { min: spanText(t, limits.capMs.min), max: spanText(t, limits.capMs.max) });
    case "reserve_too_short":
      return t("research.error.reserve_too_short", { min: spanText(t, limits.synthesisReserveMs.min) });
    case "research_too_short":
      return t("research.error.research_too_short", { min: spanText(t, limits.researchMs.min) });
    case "children_out_of_range":
      return t("research.error.children_out_of_range", { max: limits.maxChildren.max });
    case "branchless":
      return t("research.error.branchless");
    case "bad_role":
      return t("research.error.bad_role");
    case "role_not_pooled":
      return t("research.error.role_not_pooled", { role: values.childRole });
    case "wait_out_of_range":
      return t("research.error.wait_out_of_range", { max: spanText(t, limits.waitLimitMs.max) });
    case "bad_strategy":
      return t("research.error.bad_strategy");
    case "interval_out_of_range":
      return fault.field === "hostIntervalMs"
        ? t("research.error.host_interval_out_of_range", { max: spanText(t, limits.intervalMs.max) })
        : t("research.error.engine_interval_out_of_range", { max: spanText(t, limits.intervalMs.max) });
  }
}

function whole(value: unknown, min: number, max: number): boolean {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= min && value <= max;
}

/** Whether a profile's runs are held to a ceiling and a research close at all: its seed carries a clock. */
export function hasResearchClock(profile: Pick<ResearchProfileView, "seed">): boolean {
  return profile.seed.capMs !== null;
}

// Every field that is wrong, in the order the form shows them. Empty when the profile holds.
export function researchFieldFaults(profile: ResearchProfileView, values: ResearchProfileValues, settings: ResearchSettingsView, pooledRoles: readonly string[]): FieldFault[] {
  const faults: FieldFault[] = [];
  const limits = settings.limits;

  if (!hasResearchClock(profile)) {
    // The two clock fields are not shown for such a key, so anything in them came from somewhere that still
    // believes the run has a ceiling.
    if (values.capMs !== null) faults.push({ field: "capMs", code: "no_clock" });
    if (values.synthesisReserveMs !== null) faults.push({ field: "synthesisReserveMs", code: "no_clock" });
  } else {
    const capOk = whole(values.capMs, limits.capMs.min, limits.capMs.max);

    if (!capOk) faults.push({ field: "capMs", code: "cap_out_of_range" });

    if (!whole(values.synthesisReserveMs, limits.synthesisReserveMs.min, limits.capMs.max)) {
      faults.push({ field: "synthesisReserveMs", code: "reserve_too_short" });
    } else if (capOk && (values.capMs as number) - (values.synthesisReserveMs as number) < limits.researchMs.min) {
      faults.push({ field: "synthesisReserveMs", code: "research_too_short" });
    }
  }

  if (!whole(values.maxChildren, 0, limits.maxChildren.max)) {
    faults.push({ field: "maxChildren", code: "children_out_of_range" });
  } else if (profile.room === null && values.maxChildren !== 0) {
    faults.push({ field: "maxChildren", code: "branchless" });
  }

  if (typeof values.childRole !== "string" || !roleNameShape.test(values.childRole)) {
    faults.push({ field: "childRole", code: "bad_role" });
  } else if (whole(values.maxChildren, 1, limits.maxChildren.max) && !pooledRoles.includes(values.childRole)) {
    faults.push({ field: "childRole", code: "role_not_pooled" });
  }

  if (!whole(values.waitLimitMs, 0, limits.waitLimitMs.max)) faults.push({ field: "waitLimitMs", code: "wait_out_of_range" });
  if (typeof values.strategy !== "string" || !profile.strategies.includes(values.strategy)) faults.push({ field: "strategy", code: "bad_strategy" });
  if (!whole(values.hostIntervalMs, 0, limits.intervalMs.max)) faults.push({ field: "hostIntervalMs", code: "interval_out_of_range" });
  if (!whole(values.engineIntervalMs, 0, limits.intervalMs.max)) faults.push({ field: "engineIntervalMs", code: "interval_out_of_range" });

  return faults;
}

/** Why a pool limit cannot stand, or null where it can: a whole number from 0 to the most a limit may hold. */
export function poolCapacityFault(t: Translate, value: unknown, most: number): string | null {
  return whole(value, 0, most) ? null : t("research.error.pool_capacity", { max: most });
}
