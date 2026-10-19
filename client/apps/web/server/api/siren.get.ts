import { SIREN_MAX, SIREN_PAGE_SIZE } from "#shared/config";

import type { SirenWatch } from "#shared/types";

// What cache-siren is seeing right now. The watch is its own service on this machine's
// loopback; this route is the only thing that talks to it, and it stands behind the same
// session rule as everything else here.
export default defineEventHandler(async (event): Promise<SirenWatch> => {
  await requireUserSession(event);

  return await readSiren(asked(getQuery(event).limit));
});

// How far back the screen is asking. A page is a bigger ask rather than a cursor, since the watch
// keeps a ring and hands back the newest of it; anything that is not a whole positive number asks
// for the first page.
function asked(value: unknown): number {
  const limit = Number.parseInt(typeof value === "string" ? value : "", 10);

  return Number.isInteger(limit) && limit > 0 ? Math.min(limit, SIREN_MAX) : SIREN_PAGE_SIZE;
}
