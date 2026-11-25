import declaredRest from "@agency/policy/rest.toml";
import declaredClaudeCode from "@agency/policy/providers/claude-code.toml";

import { enabledClaudeAccounts, recordedAccount, SEED_ACCOUNT_ID } from "./claude-account";

import type { Archive } from "@agency/archive/archive";
import { mapInOrder } from "@agency/support/in-order";

// A rest belongs to one Claude account or to the service, and never to the agent as a whole. A
// limit is one login's: its window fills, that login answers nothing until the window resets, and
// every other login answers through it — so each account holds its own rest and its own moment of
// coming back, and no account's can be swallowed by another's. An outage is the service's and
// belongs to no account at all.
//
// What the rest of the program asks is still one question — "is the agent resting?" — and it is an
// aggregate: the agent is at rest when nothing will answer, which is when the service is down or
// when every enabled account has reached its own ceiling. The first login back ends the silence.
//
// This is the rest as it stands in the archive. Who is told of a rest beginning or lifting, and in
// what words, is the deployment's (`rest.ts` for agency's).
const accountRestKey = (account: string) => `rest_until:${account}`;
// A rest that names no account. An outage is always one — it is the service's — and a limit rest
// written with no login named is the login this machine already had, which is the account that
// stands for it.
const restUntilKey = "rest_until";
// Why the rest began: a usage limit or the service being down.
const restKindKey = "rest_kind";

export type RestKind = "limit" | "outage";

// How long a limit that named no reset moment lasts. The deployment's configuration holds it.
export interface RestSettings {
  restFallbackMs: number;
}

// How long a login is put aside when its limit named no reset moment and the caller holds no
// configuration of its own: the shared policy's, which agency's server reads too.
export const restFallbackDefaultMs = declaredRest.fallback_seconds * 1000;

// What the rest standing now is about. An outage outranks a limit: while the service itself will
// not answer, which login's window is full says nothing about why the rooms are quiet.
export async function restKind(archive: Archive, now: number = Date.now()): Promise<RestKind> {
  return await outageRest(archive, now) !== null ? "outage" : "limit";
}

// The first moment, counted in seconds, past every date a deployment meets: a moment at or past it was written
// in milliseconds, as agency's Rust side reads one (docs/api.md, 「accounts, rest and usage」).
const MILLISECOND_MOMENTS = 100_000_000_000;

/** A moment in epoch seconds, whichever unit it was written in: one counted in milliseconds is brought to the
 * second it names. */
export function epochSeconds(moment: number): number {
  return Math.abs(moment) >= MILLISECOND_MOMENTS ? Math.floor(moment / 1000) : moment;
}

async function storedMoment(archive: Archive, key: string): Promise<number | null> {
  const value = await archive.settings().getSetting(key);

  if (value === null) {
    return null;
  }

  const parsed = Number.parseInt(value, 10);

  return Number.isNaN(parsed) ? null : epochSeconds(parsed);
}

// The provider the Claude logins' windows are kept under, and how long a reading of them stands: the reader's
// passes times the passes a reading outlives (docs/usage-windows.md, 「The Claude account reader」).
const CLAUDE_CODE_MODE = "claude-code";
const readingStandsSeconds = declaredClaudeCode.usage.pass_seconds * declaredClaudeCode.usage.fresh_passes;
// The used fraction at which a window is full.
const FULL = 1;

/**
 * The moment the latest of a login's windows read full resets, where one is and has not reset by `now`, in epoch
 * seconds: the account judgement's reading of the windows, as the Rust side makes it. A full window that names
 * its reset closes the login until then however old its reading; one that names none closes it only while its
 * reading stands — younger than the reader's span and later than the login's unread mark — for the policy's
 * fallback after it was seen.
 */
export async function fullWindowUntil(archive: Archive, account: string, now: number = Date.now()): Promise<number | null> {
  const nowSeconds = Math.floor(now / 1000);
  const unread = await storedMoment(archive, `usage_windows_unread:${account}`);
  let until: number | null = null;

  for (const window of await archive.usage().usageWindows(CLAUDE_CODE_MODE, account)) {
    if (window.utilization < FULL) {
      continue;
    }

    const stands = window.seenAt >= nowSeconds - readingStandsSeconds && (unread === null || window.seenAt > unread);
    const reset = window.resetsAt !== null ? epochSeconds(window.resetsAt) : stands ? window.seenAt + declaredRest.fallback_seconds : null;

    if (reset !== null && reset > nowSeconds) {
      until = until === null ? reset : Math.max(until, reset);
    }
  }

  return until;
}

function standing(until: number | null, now: number): number | null {
  return until !== null && until > Math.floor(now / 1000) ? until : null;
}

/** The service's own rest, which belongs to no account, or nothing where none stands. */
export async function outageRest(archive: Archive, now: number = Date.now()): Promise<number | null> {
  return await archive.settings().getSetting(restKindKey) === "outage" ? standing(await storedMoment(archive, restUntilKey), now) : null;
}

/**
 * How long one account answers nothing for its limit, or nothing: the later of the rest written under
 * it and a window of its own read full (`fullWindowUntil`), as the one account judgement reads it on
 * the Rust side. A rest written with no login named is this machine's own login's, which is the account
 * that stands for it, so that account reads its own key and the unnamed one alike.
 */
export async function accountRestUntil(archive: Archive, account: string, now: number = Date.now()): Promise<number | null> {
  const own = await writtenRestUntil(archive, account);
  const windowed = await fullWindowUntil(archive, account, now);

  if (own === null || windowed === null) {
    return own ?? windowed;
  }

  return Math.max(own, windowed);
}

// The rest written under one account, or under no login where it is this machine's own.
async function writtenRestUntil(archive: Archive, account: string): Promise<number | null> {
  const own = await storedMoment(archive, accountRestKey(account));

  if (own !== null) {
    return own;
  }

  if (account !== SEED_ACCOUNT_ID || await archive.settings().getSetting(restKindKey) === "outage") {
    return null;
  }

  return storedMoment(archive, restUntilKey);
}

/** Whether this account will answer nothing until its own window resets. */
export async function isAccountResting(archive: Archive, account: string, now: number = Date.now()): Promise<boolean> {
  return standing(await accountRestUntil(archive, account, now), now) !== null;
}

/** Every login the Claude road walks, with the rest it holds. */
export async function accountRests(archive: Archive, now: number = Date.now()): Promise<{ account: string; until: number | null }[]> {
  return mapInOrder((await enabledClaudeAccounts(archive)), async (account) => ({
    account: account.id,
    until: await accountRestUntil(archive, account.id, now),
  }));
}

/**
 * When the agent answers again, or nothing where it is answering now. The service's own rest says
 * its own moment; a limit rest is over the moment the first login is back, so what stands is the
 * earliest of the accounts' deadlines, and it stands at all only while every one of them rests.
 */
export async function restUntil(archive: Archive, now: number = Date.now()): Promise<number | null> {
  const outage = await outageRest(archive, now);

  if (outage !== null) {
    return outage;
  }

  const rests = (await accountRests(archive, now)).map((one) => standing(one.until, now));

  if (rests.length === 0 || rests.some((until) => until === null)) {
    return null;
  }

  return Math.min(...(rests as number[]));
}

export async function isResting(archive: Archive, now: number = Date.now()): Promise<boolean> {
  return await restUntil(archive, now) !== null;
}

export function restUntilFor(settings: Pick<RestSettings, "restFallbackMs">, resetsAt: number | null): number {
  return resetsAt === null ? Math.floor((Date.now() + settings.restFallbackMs) / 1000) : epochSeconds(resetsAt);
}

/**
 * A rest written down. The answer is whether this rest is the one that silenced the agent: nothing
 * was resting before it, and nothing answers now. That is the moment a deployment tells anyone of,
 * and a rest that changed nothing of the kind — a second outage while the first goes on, an account
 * already resting, a login's window filling while another still answers — answers false.
 *
 * A limit rest is written under the account that reached its ceiling, and an account already
 * resting is left as it stands — but another account's rest never stops one being written, which
 * is the whole of what a second login is for. An outage is the service's, so a second one while
 * the same outage goes on is the same fact and writes nothing.
 */
export async function writeRest(
  archive: Archive,
  until: number,
  kind: RestKind = "limit",
  account: string = recordedAccount(null),
): Promise<boolean> {
  until = epochSeconds(until);
  const wasResting = await isResting(archive);

  if (kind === "outage") {
    if (await outageRest(archive) !== null) {
      return false;
    }

    // The unnamed pair is where an outage stands, and a limit rest written there by an older
    // hand is this machine's login's: it is put under that account's own name before the pair is
    // taken, so the service's rest never swallows a login's.
    const unnamed = await archive.settings().getSetting(restKindKey) === "outage" ? null : await storedMoment(archive, restUntilKey);

    if (unnamed !== null && await storedMoment(archive, accountRestKey(SEED_ACCOUNT_ID)) === null) {
      await archive.settings().setSetting(accountRestKey(SEED_ACCOUNT_ID), String(unnamed));
    }

    await archive.settings().setSetting(restUntilKey, String(until));
    await archive.settings().setSetting(restKindKey, kind);
  } else {
    if (await isAccountResting(archive, account)) {
      return false;
    }

    await archive.settings().setSetting(accountRestKey(account), String(until));
  }

  const whose = kind === "outage" ? "the service" : `account ${account}`;
  const silenced = !wasResting && await isResting(archive);

  console.log(`${whose} rests until ${new Date(until * 1000).toISOString()}${silenced ? ": nothing answers now" : ""}`);

  return silenced;
}

