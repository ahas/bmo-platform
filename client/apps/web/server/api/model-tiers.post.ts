export default defineEventHandler(async (event) => {
  await requireUserSession(event);
  const body = await readBody(event);
  return await agencyFetch("/model-tiers", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) }, { timeoutMs: LISTING_TIMEOUT_MS });
});
