import { DEFAULT_PALETTE, isPaletteId } from "bmo-ui/shared/palettes";

// The palette each account's bubbles are drawn in, kept against the account so it follows the
// person to every browser and device (docs/web.md, 「Palettes」).

// The account's palette by its id: the default where it never chose one, or chose one the console
// no longer has.
export async function readPalette(user: string): Promise<string> {
  const row = await useWebSql().get("SELECT palette FROM web_palettes WHERE user_name = $1", user) as { palette: string } | null;

  return row !== null && isPaletteId(row.palette) ? row.palette : DEFAULT_PALETTE.id;
}

// The account's palette replaced. The row names the signed-in user on the multi-user model, and is
// the platform's where none is named.
export async function savePalette(user: string, palette: string, now: number, userId: number | null): Promise<void> {
  await useWebSql().run(`INSERT INTO web_palettes (user_name, palette, updated_at, user_id) VALUES ($1, $2, $3, $4)
       ON CONFLICT (user_name) DO UPDATE SET palette = excluded.palette, updated_at = excluded.updated_at, user_id = excluded.user_id`, user, palette, now, userId);
}
