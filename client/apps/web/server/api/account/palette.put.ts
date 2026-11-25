import { isPaletteId } from "bmo-ui/shared/palettes";
import { wording } from "#shared/wording";

// The logged-in account's palette replaced, kept as it is chosen.
export default defineEventHandler(async (event): Promise<{ palette: string }> => {
  const signed = await signedInUser(event);
  const body = (await readBody(event)) as { palette?: unknown } | null;
  const palette = body?.palette;

  if (!isPaletteId(palette)) {
    throw fail(400, wording("account.error.bad_palette"));
  }

  await savePalette(signed.name, palette, Math.floor(Date.now() / 1000), signed.id);

  return { palette };
});
