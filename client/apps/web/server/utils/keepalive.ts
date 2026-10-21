import { unitFetch } from "./units";

import type { AgentSummary } from "#shared/types";

// The keepalive as the cache warmer holds it (the cache warmer's `docs/api.md`): the round as `intervalMs`,
// `nextAt` and `running`; each agent as the subject of its name under the consumer the deployment names in
// `KEEPALIVE_CONSUMER`, with the switch thrown and the pause set by a merge patch; and the targets those
// subjects warm, page by page. Every route takes the bearer `CACHE_WARMER_TOKEN` carries. With no consumer
// named, an agent's own switch is not offered and the round still is.

// The warmer's codes, each with the pack's key for its sentence.
const WARMER_CODES: Readonly<Record<string, string>> = {
  "bad-request": "agents.error.keepalive_refused",
  "invalid-clock": "agents.error.keepalive_refused",
  "invalid-kind": "agents.error.keepalive_refused",
  "not-found": "agents.error.keepalive_no_consumer",
  unauthorized: "agents.error.keepalive_failed",
  failed: "agents.error.keepalive_failed"
};

// A pause ends at the moment it names or at a use whose work began after it, whichever comes first. The
// console's pause is the worker's until its next job, a moment no one knows ahead of time, so it names the
// furthest moment a number carries and lets that job's use lift it.
const PAUSE_UNTIL_MS = Number.MAX_SAFE_INTEGER;

const CALL = { codes: WARMER_CODES };

// The most one page of a list the warmer keeps holds; a longer list is read a page at a time.
const PAGE = "limit=1000";

/** The consumer an agent's keepalive stands under, or null where the deployment names none. */
export function keepaliveConsumer(environment: Record<string, string | undefined> = process.env): string | null {
  const named = environment.KEEPALIVE_CONSUMER?.trim() ?? "";

  return named === "" ? null : named;
}

/** The round as the warmer holds it. */
interface Round {
  intervalMs: number;
  nextAt: number;
  running: boolean;
}

/** Whether the owner has stopped every warming at once. */
export async function keepaliveRound(): Promise<{ stopped: boolean }> {
  const round = await unitFetch<Round>("CACHE_WARMER", "/round", undefined, CALL);

  return { stopped: !round.running };
}

/** The round stopped or let run. */
export async function setKeepaliveRound(stopped: boolean): Promise<void> {
  await unitFetch("CACHE_WARMER", "/round", { method: "PATCH", headers: { "content-type": "application/json" }, body: JSON.stringify({ running: !stopped }) }, CALL);
}

/** One agent's warming switched off or on again, or its switch returned to the registration's default. */
export async function switchAgentKeepalive(consumer: string, agent: string, stopped: boolean): Promise<void> {
  await unitFetch("CACHE_WARMER", subjectPath(consumer, agent), { method: "PATCH", headers: { "content-type": "application/json" }, body: JSON.stringify({ on: !stopped }) }, CALL);
}

/** One agent's warming turned off until its next use. */
export async function pauseAgentKeepalive(consumer: string, agent: string): Promise<void> {
  await unitFetch(
    "CACHE_WARMER",
    subjectPath(consumer, agent),
    { method: "PATCH", headers: { "content-type": "application/json" }, body: JSON.stringify({ pausedUntil: PAUSE_UNTIL_MS }) },
    CALL
  );
}

/** A page of a list the warmer keeps, in id order. */
interface Page<T> {
  items: T[];
  more: boolean;
}

interface Subject {
  id: number;
  name: string;
  on: boolean | null;
  pausedAt?: number | null;
}

interface Target {
  id: number;
  subjectName: string;
  warmedAt?: number | null;
}

/** Every item of one list the warmer keeps, read a page at a time past the last id read. */
async function pages<T extends { id: number }>(path: string): Promise<T[]> {
  const items: T[] = [];
  let after = 0;

  for (;;) {
    const next = after === 0 ? "" : `&after=${after}`;
    const page = await unitFetch<Page<T>>("CACHE_WARMER", `${path}${path.includes("?") ? "&" : "?"}${PAGE}${next}`, undefined, CALL);

    items.push(...page.items);

    const last = page.items.at(-1);

    if (!page.more || last === undefined) return items;

    after = last.id;
  }
}

/**
 * What the warmer holds of each agent's keepalive, laid over the agents' rows. The switch is the warmer's
 * alone: a warmer that holds no subject holds no switch to show, and an agent the warmer knows nothing of
 * reads as running.
 */
export async function withKeepalive(agents: AgentSummary[], consumer: string | null): Promise<AgentSummary[]> {
  if (consumer === null) return agents.map((agent) => ({ ...agent, keepaliveStopped: false, keepalivePausedAt: null }));

  try {
    const named = encodeURIComponent(consumer);
    const [subjects, targets] = await Promise.all([
      pages<Subject>(`/consumers/${named}/subjects`),
      pages<Target>(`/targets?consumerName=${named}`)
    ]);
    const bySubject = new Map(subjects.map((subject) => [subject.name, subject]));
    const warmed = new Map<string, number>();

    for (const target of targets) {
      if (typeof target.warmedAt === "number") warmed.set(target.subjectName, Math.max(warmed.get(target.subjectName) ?? 0, target.warmedAt));
    }

    // The warmer counts in milliseconds and the rows in seconds.
    return agents.map((agent) => {
      const subject = bySubject.get(agent.name);
      const lastWarmed = warmed.get(agent.name);

      return {
        ...agent,
        keepaliveStopped: subject?.on === false,
        keepalivePausedAt: seconds(subject?.pausedAt ?? null),
        lastKeepaliveAt: lastWarmed === undefined ? agent.lastKeepaliveAt : seconds(lastWarmed)
      };
    });
  } catch {
    // A warmer that does not answer leaves every switch down, since the switch is the warmer's.
    return agents.map((agent) => ({ ...agent, keepaliveStopped: false, keepalivePausedAt: null }));
  }
}

function subjectPath(consumer: string, agent: string): string {
  return `/consumers/${encodeURIComponent(consumer)}/subjects/${encodeURIComponent(agent)}`;
}

function seconds(ms: number | null): number | null {
  return ms === null ? null : Math.floor(ms / 1000);
}
