import { wording } from "#shared/wording";

import type { WorkerChanged } from "#shared/types";

interface WorkerNodeBody {
  action?: unknown;
  name?: unknown;
  node?: unknown;
  path?: unknown;
}

// Where a worker's checkouts stand on the nodes (the agency's docs/work-nodes.md, 「A checkout on a node」): a base's
// checkout on a node added or removed, or a worker of its own placed on a node or brought back. The
// rows are the agency's to write, so this asks the agency, and its refusals are said by the pack's sentence for their code.
export default defineEventHandler(async (event): Promise<WorkerChanged> => {
  await requireUserSession(event);

  const body = await readBody<WorkerNodeBody>(event);
  const action = body?.action;
  const name = typeof body?.name === "string" ? body.name.trim() : "";

  if (action !== "add-checkout" && action !== "remove-checkout" && action !== "place") {
    throw fail(400, wording("common.error.bad_action"));
  }

  if (name === "") {
    throw fail(400, wording("agents.error.worker_name_empty"));
  }

  return await agencyFetch<WorkerChanged>(
    "/worker/node",
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        action,
        name,
        node: typeof body?.node === "string" ? body.node : undefined,
        path: typeof body?.path === "string" ? body.path : undefined
      })
    }
  );
});
