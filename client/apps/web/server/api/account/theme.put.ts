import { isThemeId } from "bmo-ui/shared/themes";
import { wording } from "#shared/wording";

// The logged-in account's theme replaced, kept as it is chosen.
export default defineEventHandler(async (event): Promise<{ theme: string }> => {
  const signed = await signedInUser(event);
  const body = (await readBody(event)) as { theme?: unknown } | null;
  const theme = body?.theme;

  if (!isThemeId(theme)) {
    throw fail(400, wording("account.error.bad_theme"));
  }

  await saveTheme(signed.name, theme, Math.floor(Date.now() / 1000), signed.id);

  return { theme };
});
