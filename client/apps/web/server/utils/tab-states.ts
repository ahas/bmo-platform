// The open tabs of the console's screens, kept per account on the server so that a screen opened
// in another browser or on another device stands with the same tabs (docs/web.md, 「Tab bar and
// toolbar」). A screen names its own key and owns the shape of what it keeps; this store only holds
// the JSON it is handed.

// A screen's key: lower-case words joined by hyphens.
const SCREEN_PATTERN = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;
const SCREEN_MAX = 40;

// How large one screen's state may be once written as JSON. A tab list is a few kilobytes.
export const TAB_STATE_MAX_BYTES = 64 * 1024;

export function isTabScreen(value: unknown): value is string {
  return typeof value === "string" && value.length <= SCREEN_MAX && SCREEN_PATTERN.test(value);
}

// What the account last kept for the screen, or null where it kept nothing or the stored text no
// longer reads as JSON.
export async function readTabState(user: string, screen: string): Promise<unknown> {
  const row = await useWebSql().get("SELECT state FROM web_tab_states WHERE user_name = $1 AND screen = $2", user, screen) as { state: string } | null;

  if (row === null) {
    return null;
  }

  try {
    return JSON.parse(row.state) as unknown;
  } catch {
    return null;
  }
}

// The screen's state replaced whole. A value that is not JSON-shaped or is too large is refused
// by the caller before it reaches here. The row names the signed-in user on the multi-user model,
// and is the platform's where none is named.
export async function saveTabState(user: string, screen: string, text: string, now: number, userId: number | null = null): Promise<void> {
  if (userId === null) {
    await useWebSql().run(`INSERT INTO web_tab_states (user_name, screen, state, updated_at) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_name, screen) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at`, user, screen, text, now);

    return;
  }

  await useWebSql().run(`INSERT INTO web_tab_states (user_name, screen, state, updated_at, user_id) VALUES ($1, $2, $3, $4, $5)
       ON CONFLICT (user_name, screen) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at, user_id = excluded.user_id`, user, screen, text, now, userId);
}
