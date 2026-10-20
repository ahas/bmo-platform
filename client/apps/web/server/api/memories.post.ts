import { wording } from "#shared/wording";

import type { MemoryChanged } from "#shared/types";

interface ActiveBody {
  slug?: unknown;
  active?: unknown;
  why?: unknown;
}

// One row taken down or put back. It is the only write this screen makes: a row is never deleted, so the state is
// the whole of what a person changes here, and taking one down says why in its labels.
export default defineEventHandler(async (event): Promise<MemoryChanged> => {
  await requireUserSession(event);

  const body = await readBody<ActiveBody>(event);
  const slug = typeof body?.slug === "string" ? body.slug.trim() : "";
  const why = typeof body?.why === "string" ? body.why.trim() : "";

  if (slug === "") {
    throw fail(400, wording("memories.error.pick_memory"));
  }

  if (typeof body?.active !== "boolean") {
    throw fail(400, wording("memories.error.direction"));
  }

  if (!body.active && why === "") {
    throw fail(400, wording("memories.error.reason_empty"));
  }

  const page = await brainCall(brain().GET("/memories", { params: { query: { label: `slug=${slug}`, limit: 1 } } }));
  const row = page.items[0];

  if (row === undefined) throw fail(404, wording("memories.error.row_missing"));

  const labels = body.active
    ? Object.fromEntries(Object.entries(row.labels).filter(([key]) => key !== "downReason" && key !== "downAt"))
    : { ...row.labels, downReason: why, downAt: String(Date.now()) };
  const changed = await brainCall(
    brain().PATCH("/memories/{memoryId}", {
      params: { path: { memoryId: row.id } },
      body: { state: body.active ? "standing" : "retired", labels }
    })
  );

  return { ok: true, memory: (await memoryViews([memoryRecordOf(changed)]))[0]! };
});
