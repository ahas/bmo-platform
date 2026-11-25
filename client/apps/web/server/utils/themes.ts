import { DEFAULT_THEME, isThemeId } from "bmo-ui/shared/themes";

// The theme each account reads the console in, kept against the account so it follows the person
// to every browser and device (docs/web.md, 「Themes」).

// The account's theme by its id: the default where it never chose one, or chose one the console no
// longer has.
export async function readTheme(user: string): Promise<string> {
  const row = await useWebSql().get("SELECT theme FROM web_themes WHERE user_name = $1", user) as { theme: string } | null;

  return row !== null && isThemeId(row.theme) ? row.theme : DEFAULT_THEME.id;
}

// The account's theme replaced. The row names the signed-in user on the multi-user model, and is the
// platform's where none is named.
export async function saveTheme(user: string, theme: string, now: number, userId: number | null): Promise<void> {
  await useWebSql().run(`INSERT INTO web_themes (user_name, theme, updated_at, user_id) VALUES ($1, $2, $3, $4)
       ON CONFLICT (user_name) DO UPDATE SET theme = excluded.theme, updated_at = excluded.updated_at, user_id = excluded.user_id`, user, theme, now, userId);
}
