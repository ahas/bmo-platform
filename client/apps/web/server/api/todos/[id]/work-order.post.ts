import type { WorkOrder, WorkOrderView } from "#shared/types";

// A change to one item's work order, made by bmo-job under the order's lock; it answers the order as it now
// stands, and a refusal of a step verb is said in the pack's words for its code.
export default defineEventHandler(async (event): Promise<WorkOrderView> => {
  await requireUserSession(event);

  const id = requireTodoId(event);
  const body = await readBody<Record<string, unknown>>(event);
  const order = await jobPost<WorkOrder>("/work-order", { ...body, id }, { timeoutMs: JOB_VERB_TIMEOUT_MS });

  return workOrderView(order);
});
