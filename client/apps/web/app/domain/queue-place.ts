import { countText } from "./numbers";

import type { DeployHold, WorkOrder, WorkOrderStop, WorkQueuePlace } from "#shared/types";
import type { Translate } from "#shared/wording";

// Where a stopped work order stands in its worker's queue, and what an order holding its checkout stands on, in
// the words the console's card in the queue says them (bmo-job's docs/work-orders.md).

const TURN_WORDS: Record<WorkQueuePlace["turn"], string> = {
  resume: "todo.label.queue_turn_resume",
  passed: "todo.label.queue_turn_passed",
  held: "todo.label.queue_turn_held"
};

/** A stopped order's stand, or null for an order that is not stopped. */
export function workOrderStop(
  t: Translate,
  order: Pick<WorkOrder, "status" | "error" | "holdsCheckout" | "queuePlace" | "failedStops" | "ownerStop"> | null
): WorkOrderStop | null {
  if (order?.status !== "paused") return null;

  const error = order.error?.split("\n").map((line) => line.trim()).find((line) => line !== "") ?? null;

  return {
    // An order the owner stopped carries their reason, which no error stands beside.
    reason: error ?? (order.ownerStop === undefined ? null : t("todo.label.owner_stopped", { why: order.ownerStop.why })),
    place: order.holdsCheckout === true ? null : (order.queuePlace ?? null),
    failures: order.failedStops ?? 0
  };
}

/**
 * Whether an order stands only to be closed: its item is finished or merged into another, or a close asked of it
 * waits for its checkout. The runner closes such an order as soon as nothing of it runs, so it is never read as
 * stopped.
 */
export function workOrderClosing(order: Pick<WorkOrder, "status" | "closing"> | null, item: { done: boolean; mergedInto: number | null } | null): boolean {
  if (order?.status !== "active" && order?.status !== "paused") return false;

  return order.closing !== undefined || (item !== null && (item.done || item.mergedInto !== null));
}

/**
 * What an order that holds its checkout is standing on before anything may run there, in the runner's own words,
 * or null for one that holds no checkout or is standing on nothing.
 */
export function checkoutHold(t: Translate, order: Pick<WorkOrder, "status" | "holdsCheckout" | "releaseWait"> | null): string | null {
  if (order?.holdsCheckout !== true) return null;

  return order.releaseWait ?? (order.status === "paused" ? t("todo.label.checkout_release_wait") : null);
}

/** The word naming a stopped order's turn alone, as the console's card in the queue says it. */
export function queueTurnWord(t: Translate, place: WorkQueuePlace): string {
  return t(TURN_WORDS[place.turn]);
}

/** The one line naming where the order stands: its turn, why, and the failures counted. */
export function queuePlaceText(t: Translate, place: WorkQueuePlace, failures: number): string {
  const turn = queueTurnWord(t, place);

  return failures > 0 ? t("todo.label.queue_place_failures", { turn, why: place.why, failures }) : t("todo.label.queue_turn_why", { turn, why: place.why });
}

const HELD_WORDS = {
  stop: "todo.label.held_stop",
  stopping: "todo.label.held_stopping",
  pause: "todo.label.held_pause",
  pausing: "todo.label.held_pausing",
  failure: "todo.label.held_failure",
  plain: "todo.label.held_plain",
  pending: "todo.label.held_pending"
} as const;

/** The word a held item wears, by who set it aside and whether its run still ends; the kinds never share a word. */
export function heldLabel(t: Translate, kind: "stop" | "pause" | "failure" | null, pending: boolean): string {
  if (kind === "stop") return t(pending ? HELD_WORDS.stopping : HELD_WORDS.stop);
  if (kind === "pause") return t(pending ? HELD_WORDS.pausing : HELD_WORDS.pause);
  if (pending) return t(HELD_WORDS.pending);

  return t(kind === "failure" ? HELD_WORDS.failure : HELD_WORDS.plain);
}

// What the deployment's hold on the queue waits on, in the words a reader takes it by: the items whose jobs
// must end before the batch goes up — "goes up when #1389 ends" — or how many jobs where their items could
// not be read. The time it gives up at is not a time the deployment ends, so it is not said here.
export function holdWaits(t: Translate, hold: DeployHold): string {
  if (hold.waitingFor !== null && hold.waitingFor.length > 0) {
    return t("todo.status.deploy_hold_after", { items: hold.waitingFor.map((item) => `#${item}`).join(", ") });
  }

  if (hold.waitingOn === null) {
    return t("todo.status.deploy_hold_jobs_unread");
  }

  return hold.waitingOn.length === 0 ? t("todo.status.deploy_hold_no_jobs") : t("todo.status.deploy_hold_jobs", { n: countText(hold.waitingOn.length) });
}
