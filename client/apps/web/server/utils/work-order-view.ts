import { jobRead } from "./job";

import type { WorkOrder, WorkOrderView } from "#shared/types";

/** One item's work order as bmo-job reads it, or null where there is no such item. */
export async function readWorkOrder(todoId: number): Promise<WorkOrder | null> {
  try {
    return (await jobRead<{ order: WorkOrder }>(`/work-orders/${todoId}`)).order;
  } catch (error) {
    if ((error as { statusCode?: number }).statusCode === 404) return null;
    throw error;
  }
}

// The order with the end of every task that has one. A task's end is its job's finish, which bmo-job reads
// out by no route, so every task stands with none (docs/boundary.md, 「Gaps the console meets」).
export function workOrderView(order: WorkOrder): WorkOrderView {
  return { ...order, ended: {} };
}
