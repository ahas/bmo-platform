import { wording } from "#shared/wording";

// One registered model taken away (the agency's docs/api.md, 「providers, models, pin-checks, tiers and
// fallback」), answered with nothing.
export default defineEventHandler(async (event): Promise<null> => {
  await requireUserSession(event);

  const key = (getRouterParam(event, "modelKey") ?? "").trim();

  if (key === "") throw fail(400, wording("agents.error.registry_value"));

  return await agencyFetch<null>(`/models/${encodeURIComponent(key)}`, { method: "DELETE" });
});
