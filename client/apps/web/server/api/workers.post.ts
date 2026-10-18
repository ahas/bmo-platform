import { wording } from "#shared/wording";

import type { WorkerChanged } from "#shared/types";

interface WorkerBody {
  action?: unknown;
  name?: unknown;
  label?: unknown;
  path?: unknown;
  preamble?: unknown;
  devCommand?: unknown;
  keepaliveStopped?: unknown;
  supportCount?: unknown;
  supportKeepalive?: unknown;
  pinnedProvider?: unknown;
}

const text = (value: unknown): string | undefined => (typeof value === "string" ? value : undefined);

// Adding, changing and removing a job worker. The workers are bmo-job's to keep, so this asks bmo-job rather
// than writing a row of its own, and what each field may hold is bmo-job's to judge: a refusal is said in the
// pack's words for its code.
export default defineEventHandler(async (event): Promise<WorkerChanged> => {
  await requireUserSession(event);

  const body = await readBody<WorkerBody>(event);
  const action = body?.action;
  const name = typeof body?.name === "string" ? body.name.trim() : "";

  if (action !== "add" && action !== "update" && action !== "remove") {
    throw fail(400, wording("common.error.bad_action"));
  }

  if (name === "") {
    throw fail(400, wording("agents.error.worker_name_empty"));
  }

  await jobPost("/worker", {
    action,
    name,
    label: text(body?.label),
    path: text(body?.path),
    preamble: text(body?.preamble),
    dev_command: text(body?.devCommand),
    keepalive_stopped: typeof body?.keepaliveStopped === "boolean" ? body.keepaliveStopped : undefined,
    support_count: typeof body?.supportCount === "number" ? body.supportCount : undefined,
    support_keepalive: typeof body?.supportKeepalive === "number" ? body.supportKeepalive : undefined,
    pinned_provider: text(body?.pinnedProvider)
  });

  return { ok: true };
});
