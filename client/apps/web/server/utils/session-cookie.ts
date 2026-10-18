import type { H3Event } from "h3";

// The session cookie as a request carries it (docs/web.md, 「Access」).

// The cookie's name as the session layer is configured, which names it nuxt-session where the
// configuration does not.
function cookieName(event: H3Event): string {
  return useRuntimeConfig(event).session.name ?? "nuxt-session";
}

/**
 * Whether a request carries a session cookie with anything in it. A cleared seal leaves the browser
 * an empty cookie of the same name, which is no session: asking the session layer about it would mint
 * an empty seal and set it on a visitor nobody signed in.
 */
export function carriesSession(event: H3Event): boolean {
  return (getCookie(event, cookieName(event)) ?? "") !== "";
}

/**
 * Ends a session in the request that found it gone. The seal is cleared in the browser, and the
 * session cookie is taken out of the request itself, since the session layer clears only what it
 * holds for the request and reads the request's own cookie again the next time it is asked: every
 * later read in the same request — the route, a page drawn on the server — then finds nobody signed.
 * The request is marked, so a page drawn on it can say the login ended.
 */
export async function endSession(event: H3Event): Promise<void> {
  const name = cookieName(event);

  await clearUserSession(event);

  const rest = Object.entries(parseCookies(event)).filter(([key]) => key !== name);

  if (rest.length === 0) {
    delete event.node.req.headers.cookie;
  } else {
    event.node.req.headers.cookie = rest.map(([key, value]) => `${key}=${encodeURIComponent(value)}`).join("; ");
  }

  event.context.sessionEnded = true;
}
