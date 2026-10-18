import { JOB_CODES } from "./job-codes";
import { unitFetch } from "./units";
import { wording } from "#shared/wording";

import type { UnitCall } from "./units";

// bmo-job's routes as its document states them (bmo-job's `docs/api.md`, 「Routes」): JSON in snake_case both ways,
// times in epoch milliseconds, and a refusal answered with its code and an English sentence, which the console
// says by the pack's sentence for the code.

// A verb that stops runs or starts a work order waits on what it asks of the agency.
export const JOB_VERB_TIMEOUT_MS = 180_000;

/** One call on bmo-job, its refusals said in the pack's words. */
export async function jobFetch<T>(path: string, init?: RequestInit, call: Omit<UnitCall, "codes"> = {}): Promise<T> {
  return await unitFetch<T>("JOB", path, init, { ...call, codes: JOB_CODES });
}

/** A read of bmo-job's. */
export async function jobRead<T>(path: string, call: Omit<UnitCall, "codes"> = {}): Promise<T> {
  return await jobFetch<T>(path, undefined, call);
}

/** What came of telling the agent how the queue stands, by its kind (bmo-job's `docs/api.md`, 「A queue told」). */
export type QueueTold = string;

const QUEUE_TOLD: Readonly<Record<QueueTold, string>> = {
  told: "todo.message.queue_told",
  nothing_waiting: "todo.message.queue_nothing_waiting",
  all_busy: "todo.message.queue_all_busy",
  paused: "todo.message.queue_paused",
  unreadable: "todo.message.queue_unreadable",
  machine_short: "todo.message.queue_machine_short",
  unchanged: "todo.message.queue_unchanged"
};

/** What came of telling the queue, in the pack's words, or null where nothing was told. */
export function queueToldText(told: QueueTold | null | undefined): string | null {
  if (told === null || told === undefined) return null;

  const key = QUEUE_TOLD[told];

  return key === undefined ? told : wording(key);
}

/** A POST of a JSON body to bmo-job. */
export async function jobPost<T>(path: string, body: unknown, call: Omit<UnitCall, "codes"> = {}): Promise<T> {
  return await jobFetch<T>(path, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) }, call);
}
