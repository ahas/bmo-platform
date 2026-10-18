import { wording } from "#shared/wording";

// One change to the pool or a research profile, carried to bmo-artifact, which checks and writes it. Only the
// action's name is judged here: what a value may be is bmo-artifact's word, said in the pack's sentence for
// its refusal.
const actions = new Set(["set-pool-limit", "reset-pool-limits", "set-profile", "reset-profile"]);

export default defineEventHandler(async (event) => {
  await requireUserSession(event);
  const body = await readBody(event);

  if (typeof body !== "object" || body === null || !actions.has((body as { action?: unknown }).action as string)) {
    throw fail(400, wording("common.error.bad_action"));
  }

  return await researchChange(body);
});
