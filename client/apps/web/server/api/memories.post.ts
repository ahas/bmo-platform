import { wording } from "#shared/wording";

import type { MemoryChanged, MemoryRecord } from "#shared/types";

interface ActiveBody {
  slug?: unknown;
  active?: unknown;
  why?: unknown;
}

// One row taken down or put back. It is the only write this screen makes: a row is never deleted, so the flag is
// the whole of what a person changes here, and taking one down says why.
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

  // A row that is not there, or one already in the state asked for, is the brain's refusal, said by its code.
  const changed = await brainPost<{ ok: true; memory: MemoryRecord }>("/memories/active", { slug, active: body.active, ...(why === "" ? {} : { why }) });

  return { ok: true, memory: (await memoryViews([changed.memory]))[0]! };
});
