import type { WorkOrderView } from "#shared/types";

// One item's work order as bmo-job reads it: the one it holds, or the draft the item would start from.
export default defineEventHandler(async (event): Promise<WorkOrderView | null> => {
  await requireUserSession(event);

  const order = await readWorkOrder(requireTodoId(event));

  return order === null ? null : workOrderView(order);
});
