import { unitFetch } from "./units";

import type { AgentSummary } from "#shared/types";

// The keepalive as the cache warmer holds it (the cache warmer's `docs/api.md`): the round, the owner's one
// switch over every warming at once, and each agent's own switch and pause, kept as the subject of that name
// under the consumer the agency registers its targets as. Which consumer that is the deployment names in
// `KEEPALIVE_CONSUMER`; with none named, an agent's own switch is not offered and the round still is.

// The warmer's codes, each with the pack's key for its sentence.
const WARMER_CODES: Readonly<Record<string, string>> = {
  invalid_body: "agents.error.keepalive_refused",
  invalid_name: "agents.error.keepalive_refused",
  unknown_consumer: "agents.error.keepalive_no_consumer",
  database: "agents.error.keepalive_failed"
};

const CALL = { codes: WARMER_CODES };

/** The consumer an agent's keepalive stands under, or null where the deployment names none. */
export function keepaliveConsumer(environment: Record<string, string | undefined> = process.env): string | null {
  const named = environment.KEEPALIVE_CONSUMER?.trim() ?? "";

  return named === "" ? null : named;
}

/** Whether the owner has stopped every warming at once. */
export async function keepaliveRound(): Promise<{ stopped: boolean }> {
  return await unitFetch<{ stopped: boolean }>("CACHE_WARMER", "/round", undefined, CALL);
}

/** The round stopped or let run. */
export async function setKeepaliveRound(stopped: boolean): Promise<void> {
  await unitFetch("CACHE_WARMER", "/round", { method: "PUT", headers: { "content-type": "application/json" }, body: JSON.stringify({ stopped }) }, CALL);
}

/** One agent's warming switched off or on again. */
export async function switchAgentKeepalive(consumer: string, agent: string, stopped: boolean): Promise<void> {
  await unitFetch(
    "CACHE_WARMER",
    `${subjectPath(consumer, agent)}/switch`,
    { method: "PUT", headers: { "content-type": "application/json" }, body: JSON.stringify({ on: !stopped }) },
    CALL
  );
}

/** One agent's warming turned off until its next use. */
export async function pauseAgentKeepalive(consumer: string, agent: string): Promise<void> {
  await unitFetch("CACHE_WARMER", `${subjectPath(consumer, agent)}/pause`, { method: "POST" }, CALL);
}

interface Subject {
  subject: string;
  switch: "on" | "off" | null;
  paused_at?: number | null;
}

interface Target {
  subject: string;
  last_warmed_at?: number | null;
}

/** What the warmer holds of each agent's keepalive, laid over the agents' rows; the rows as they came where it holds nothing. */
export async function withKeepalive(agents: AgentSummary[], consumer: string | null): Promise<AgentSummary[]> {
  if (consumer === null) return agents;

  try {
    const named = encodeURIComponent(consumer);
    const [{ subjects }, { targets }] = await Promise.all([
      unitFetch<{ subjects: Subject[] }>("CACHE_WARMER", `/consumers/${named}/subjects`, undefined, CALL),
      unitFetch<{ targets: Target[] }>("CACHE_WARMER", `/targets?consumer=${named}`, undefined, CALL)
    ]);
    const bySubject = new Map(subjects.map((subject) => [subject.subject, subject]));
    const warmed = new Map<string, number>();

    for (const target of targets) {
      if (typeof target.last_warmed_at === "number") warmed.set(target.subject, Math.max(warmed.get(target.subject) ?? 0, target.last_warmed_at));
    }

    // The warmer counts in milliseconds and the rows in seconds.
    return agents.map((agent) => {
      const subject = bySubject.get(agent.name);
      const lastWarmed = warmed.get(agent.name);

      return {
        ...agent,
        keepaliveStopped: subject === undefined ? agent.keepaliveStopped : subject.switch === "off",
        keepalivePausedAt: subject === undefined ? agent.keepalivePausedAt : seconds(subject.paused_at ?? null),
        lastKeepaliveAt: lastWarmed === undefined ? agent.lastKeepaliveAt : seconds(lastWarmed)
      };
    });
  } catch {
    // A warmer that does not answer leaves the rows as the agency gave them.
    return agents;
  }
}

function subjectPath(consumer: string, agent: string): string {
  return `/consumers/${encodeURIComponent(consumer)}/subjects/${encodeURIComponent(agent)}`;
}

function seconds(ms: number | null): number | null {
  return ms === null ? null : Math.floor(ms / 1000);
}
