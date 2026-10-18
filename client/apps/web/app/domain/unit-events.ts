import type { BotEvent, WorkOrderProgress } from "#shared/types";

// bmo-job's and the agency's events, as the relay hands them on (docs/web.md, 「How the units are talked to」),
// read into the console's own events, so a screen that reads back on the TODO moving or a turn ending hears
// them as it hears the receiver's. bmo-job's are numbered in its own run and read under its own cursor; the
// agency's are numbered by nothing.

type Fields = Record<string, unknown>;

const text = (value: unknown): string | null => (typeof value === "string" ? value : null);
const number = (value: unknown): number | null => (typeof value === "number" && Number.isFinite(value) ? value : null);

// A work order's standing as bmo-job's `order_changed` carries it.
function orderOf(value: unknown): WorkOrderProgress | null {
  const order = (typeof value === "object" && value !== null ? value : null) as Fields | null;

  if (order === null) return null;

  return {
    status: order.status as WorkOrderProgress["status"],
    phase: order.phase as WorkOrderProgress["phase"],
    stage: number(order.stage) ?? 0,
    updatedAt: number(order.updated_at) ?? 0,
    tasks: (Array.isArray(order.tasks) ? (order.tasks as Fields[]) : []).map((task) => ({
      id: text(task.id) ?? "",
      state: task.state as WorkOrderProgress["tasks"][number]["state"],
      attempt: number(task.attempt) ?? 0,
      jobId: number(task.job),
      model: text(task.model) ?? "",
      error: text(task.error)
    }))
  };
}

/**
 * One of bmo-job's events as the console's (bmo-job's `docs/api.md`, 「The event stream」): the queue moving is
 * the TODO moving, an order changing is that item's work order, and the talk moving is that item's talk. Its id
 * is bmo-job's own, which the caller counts apart from the receiver's; a kind the console reads nothing of is
 * null.
 */
export function jobEvent(data: Fields): BotEvent | null {
  const id = number(data.id);
  const at = number(data.at) ?? 0;
  const item = number(data.item);

  if (id === null) return null;

  if (data.kind === "queue_moved") return { id, at, type: "todo", chatId: 0, messageId: null };

  if (data.kind === "order_changed" && item !== null) {
    const workOrder = orderOf(data.order);

    return workOrder === null ? null : { id, at, type: "work-order", chatId: 0, messageId: null, todoId: item, workOrder };
  }

  if (data.kind === "talk_moved" && item !== null) return { id, at, type: "todo-talk", chatId: 0, messageId: null, todoId: item };

  return null;
}

/** One of the agency's turn events as the console's (the agency's `docs/api.md`, 「Events」), or null. */
export function agencyEvent(data: Fields): BotEvent | null {
  const session = text(data.session);
  const run = text(data.run);
  const phase = data.kind === "turn-started" ? "started" : data.kind === "turn-ended" ? "ended" : null;

  if (session === null || run === null || phase === null) return null;

  return { id: 0, at: number(data.at) ?? Date.now(), type: "agency-turn", chatId: 0, messageId: null, phase, session, run };
}
