import { FALLBACK_ROUTE_MODES, RUNNER_AGENTS, SIREN_MAX, SIREN_PAGE_SIZE } from "#shared/config";
import { agentProviderNames } from "#shared/types";

import { unitFetch } from "./units";

import type { AgentMode, SirenEvent, SirenQuota, SirenWatch } from "#shared/types";

// The cache siren as the console reads it (the cache siren's `docs/api.md`, `GET /stats` and `GET /events`): its own
// tally and the last requests it saw, at the address `CACHE_SIREN_ADDRESS` names. Which agent holds a session and
// what moved a cache prefix are the agency's records, which no route serves the console (docs/boundary.md, 「Gaps
// the console meets」), so a request is named only where the siren's row names it and no prefix loss carries a
// cause.

// The siren reads local files on this machine; it answers at once or it is not there.
const CALL_TIMEOUT_MS = 3_000;

interface SirenQuotaStats {
  five_h?: number | null;
  five_h_resets?: number | null;
  seven_d?: number | null;
  seven_d_resets?: number | null;
  tier?: string | null;
  provider?: string | null;
  accounts?: number | null;
}

// The quota the siren read from its source, in the console's words. The name it carries is the account's the
// source marked active — the one runs go out on now — and an empty one is no name.
export function watchQuota(stats: SirenQuotaStats | undefined): SirenQuota {
  return {
    fiveHour: stats?.five_h ?? null,
    fiveHourResetsAt: stats?.five_h_resets ?? null,
    weekly: stats?.seven_d ?? null,
    weeklyResetsAt: stats?.seven_d_resets ?? null,
    account: stats?.tier || null,
    provider: isAgentMode(stats?.provider) ? stats.provider : null,
    accounts: stats?.accounts ?? null
  };
}

function isAgentMode(value: unknown): value is AgentMode {
  return value !== "auto" && (FALLBACK_ROUTE_MODES as readonly unknown[]).includes(value);
}

interface SirenStats {
  per_min?: number[];
  new_60s?: number;
  sessions_30m?: number;
  uptime_s?: number;
  files?: number;
  last_event?: string | null;
  cfg?: { yellow_new_tokens?: number; red_new_tokens?: number; burst_red_tokens?: number };
  quota?: SirenQuotaStats;
}

interface SirenRow {
  seq?: number;
  ts_kst?: string;
  epoch?: number;
  project?: string;
  // The first characters of the session's number, and the last part of the directory the session worked in.
  session?: string;
  cwd?: string;
  // A transcript row says whether it is a subagent's; a row a caller posted carries the caller's agent name.
  agent?: string | boolean;
  mode?: string;
  m?: string;
  model?: string;
  kind?: string;
  level?: string;
  note?: string;
  new?: number;
  read?: number;
}

async function ask<T>(path: string): Promise<T> {
  return await unitFetch<T>("CACHE_SIREN", path, undefined, CALL_TIMEOUT_MS);
}

function unreachable(): SirenWatch {
  return {
    reachable: false,
    files: 0,
    lastEvent: null,
    uptimeSeconds: 0,
    newLastMinute: 0,
    perMinute: [],
    sessions30m: 0,
    thresholds: { yellow: 0, red: 0, burst: 0 },
    events: [],
    hasMore: false,
    quota: watchQuota(undefined)
  };
}

// What the screen stands on: the siren's own tally and the last requests it saw. A siren that does not answer is
// not this app's failure — the screen says so and shows nothing else.
export async function readSiren(limit: number = SIREN_PAGE_SIZE): Promise<SirenWatch> {
  const asked = Math.min(Math.max(limit, 1), SIREN_MAX);

  try {
    const [stats, feed] = await Promise.all([ask<SirenStats>("/stats"), ask<{ events?: SirenRow[] }>(`/events?limit=${asked}`)]);
    const rows = feed.events ?? [];

    return {
      reachable: true,
      files: stats.files ?? 0,
      lastEvent: stats.last_event ?? null,
      uptimeSeconds: stats.uptime_s ?? 0,
      newLastMinute: stats.new_60s ?? 0,
      perMinute: stats.per_min ?? [],
      sessions30m: stats.sessions_30m ?? 0,
      thresholds: {
        yellow: stats.cfg?.yellow_new_tokens ?? 0,
        red: stats.cfg?.red_new_tokens ?? 0,
        burst: stats.cfg?.burst_red_tokens ?? 0
      },
      events: rows.map(asEvent).reverse(),
      // It handed back as many as it was asked for, so there may be more of its ring behind them. Fewer than
      // asked is the ring itself ending, and there is nothing further to ask for.
      hasMore: rows.length >= asked && asked < SIREN_MAX,
      quota: watchQuota(stats.quota)
    };
  } catch {
    return unreachable();
  }
}

function asEvent(row: SirenRow): SirenEvent {
  return {
    seq: row.seq ?? 0,
    at: row.ts_kst ?? "",
    atSeconds: typeof row.epoch === "number" ? Math.floor(row.epoch) : null,
    project: row.project ?? "",
    session: row.session ?? "",
    agent: sirenAgentOf(row),
    // A row names the provider it came from; one that names none, or one this console does not know, is read
    // as Claude Code's.
    mode: agentProviderNames.find((name) => name === row.mode) ?? "claude-code",
    // Rows carry the model's identifier in `model`, and `m` is its short display name; the identifier wins.
    model: row.model ?? row.m ?? "",
    kind: row.kind ?? "",
    level: row.level ?? "info",
    note: row.note ?? "",
    cause: null,
    newTokens: row.new ?? 0,
    readTokens: row.read ?? 0
  };
}

// Which agent made a request. Two grounds, and no third: a row a caller posted names its agent, and a runner
// works in a directory of its own name outside the repository. Everything else on this machine is left unnamed
// rather than guessed at.
export function sirenAgentOf(row: Pick<SirenRow, "agent" | "project" | "cwd">): string | null {
  if (typeof row.agent === "string" && row.agent !== "") {
    return row.agent;
  }

  const project = row.project ?? "";
  const cwd = row.cwd ?? "";

  return RUNNER_AGENTS.find((name) => cwd === name && project.endsWith(`-${name}`)) ?? null;
}
