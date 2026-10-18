import { unitFetch } from "./units";

import type { UnitCall } from "./units";

// The receiver's own routes as its document states them (the receiver's `docs/boundary-api.md`, 「The receiver's own
// routes」 and 「For the web unit's console」): JSON in camelCase, and a refusal answered with its code and an English
// sentence for a caller, which the console says by the pack's sentence for the code (the receiver's
// `docs/codes.md`). The companions' refusals are said by `companions.ts`, beside the form they are about.

// A send waits its turn on the road it goes by, which may do one thing at a time, before the words go.
export const SEND_TIMEOUT_MS = 45_000;
// How much longer a send the screen was told ran late is still listened for, so what it went out as is written
// under its key while the screen checks it.
export const SEND_LATE_MS = 120_000;
// An upload travels twice — into this app, then into the receiver — and a file may be tens of megabytes.
export const ATTACH_TIMEOUT_MS = 120_000;

export const RECEIVER_CODES: Readonly<Record<string, string>> = {
  // Every route's.
  "bad-request": "room.error.receiver_bad_request",
  unauthorized: "room.error.receiver_unauthorized",
  forbidden: "room.error.receiver_forbidden",
  "not-found": "room.error.receiver_not_found",
  "not-ready": "room.error.receiver_not_ready",
  conflict: "room.error.receiver_conflict",
  "unit-failed": "room.error.receiver_unit_failed",
  unreachable: "room.error.receiver_unreachable",
  failed: "room.error.receiver_failed",
  // The owner's words from the console.
  "reply-missing": "room.error.reply_missing",
  "words-only": "room.error.words_only",
  "too-large": "room.error.file_too_large_named",
  "none-sent": "room.error.none_sent",
  // The voice briefing.
  "switched-off": "room.error.briefing_switched_off",
  voiceless: "room.error.briefing_voiceless",
  unsigned: "room.error.briefing_unsigned",
  wordless: "room.error.briefing_wordless",
  "not-up": "room.error.briefing_not_up",
  "one-switch": "room.error.briefing_one_switch",
  // Removing a room.
  "seeded-room": "room.error.removal_seeded_room",
  infrastructure: "room.error.removal_infrastructure",
  "owners-chat": "room.error.removal_owners_chat",
  "not-administrator": "room.error.removal_not_administrator",
  chat: "room.error.removal_chat"
};

/** One call on the receiver, its refusals said in the pack's words. */
export async function receiverFetch<T>(path: string, init?: RequestInit, call: Omit<UnitCall, "codes"> = {}): Promise<T> {
  return await unitFetch<T>("RECEIVER", path, init, { ...call, codes: RECEIVER_CODES });
}

/** A read of the receiver's. */
export async function receiverRead<T>(path: string, call: Omit<UnitCall, "codes"> = {}): Promise<T> {
  return await receiverFetch<T>(path, undefined, call);
}

/** A POST of a JSON body to the receiver. */
export async function receiverPost<T>(path: string, body: unknown, call: Omit<UnitCall, "codes"> = {}): Promise<T> {
  return await receiverFetch<T>(path, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) }, call);
}
