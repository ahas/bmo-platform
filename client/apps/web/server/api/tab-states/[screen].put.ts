import { wording } from "#shared/wording";

// One screen's tabs kept for the logged-in account, replacing what it kept before.
export default defineEventHandler(async (event): Promise<{ saved: true }> => {
  const signed = await signedInUser(event);
  const screen = getRouterParam(event, "screen");
  const body = (await readBody(event)) as { state?: unknown } | null;

  if (!isTabScreen(screen)) {
    throw fail(400, wording("screens.error.bad_screen_name"));
  }

  if (body === null || typeof body !== "object" || body.state === undefined || body.state === null || typeof body.state !== "object") {
    throw fail(400, wording("screens.error.no_tab_state"));
  }

  const text = JSON.stringify(body.state);

  if (Buffer.byteLength(text) > TAB_STATE_MAX_BYTES) {
    throw fail(413, wording("screens.error.tab_state_too_large"));
  }

  await saveTabState(signed.name, screen, text, Math.floor(Date.now() / 1000), signed.id);

  return { saved: true };
});
