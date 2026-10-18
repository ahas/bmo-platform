import { wording } from "#shared/wording";

// What the logged-in account last kept for one screen's tabs; null where it kept nothing.
export default defineEventHandler(async (event): Promise<{ state: unknown }> => {
  const session = await requireUserSession(event);
  const screen = getRouterParam(event, "screen");

  if (!isTabScreen(screen)) {
    throw fail(400, wording("screens.error.bad_screen_name"));
  }

  return { state: await readTabState(session.user.name, screen) };
});
