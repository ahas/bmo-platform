import { mkdirSync, readFileSync, statSync } from "node:fs";
import { dirname } from "node:path";

import { credentialFilePath, recordedAccount } from "./claude-account";
import { takeLock } from "@agency/support/file-lock";
import { emitAgencyEvent } from "./events";
import { dataPath } from "@agency/support/roots";
import declaredClaudeCode from "@agency/policy/providers/claude-code.toml";

import type { Archive } from "@agency/archive/archive";
import type { ClaudeAccountRow } from "@agency/archive/claude-account-store";

// A login and its ending. Every Claude account is a login somebody once made, and a login does
// not last: a subscription login is an OAuth pair the CLI renews for itself, and the long-lived
// token the CLI issues stands for a year and then simply dies, carrying no refresh at all. What
// Agency can do about that is the whole of this file.
//
// **The moment a credential file states is not the login's end.** A subscription login's file
// states when its access token stops being accepted, which is an hour or so away at any time and
// is renewed by the CLI itself out of the refresh it holds beside it. So a file that holds a
// refresh is a login with no end anybody here can read, and reading its stated moment as an ending
// would mark a perfectly good login as gone every hour and say so to the owner every hour. What does
// end is a login whose file holds no refresh — a long-lived token, whose year is its whole life —
// and a login whose refresh has stopped being taken, which nothing in a file says and only a run
// that went out on it can tell.
//
// **Only moments are ever kept.** A credential file is opened to learn those two things — when the
// access token it holds stops, and whether it holds anything to renew with — and for nothing else:
// no token, no cookie and no part of the file's body is written into a row, a log line, a document
// or a report, and nothing here returns any of it to a caller.
//
// Three things stand on that.
//
// - **One renewal, not several.** Runs of one account share that account's configuration
//   directory, and it is two of them renewing at once that leaves each holding what the other has
//   already replaced. The renewal happens inside the CLI's own run rather than in a call of its
//   own, so holding it means holding that run: a run whose login is inside the last half hour of
//   its life takes that account's own lock on the way out, the one holding it is the one whose CLI
//   renews, and the others wait for it and then go out on what it wrote. Away from that window
//   nothing is held at all.
// - **A login that is gone is marked, not retried.** The account's row says so, the walk passes
//   it over, and its work goes to the next login and then to the next provider.
// - **The owner is told before it ends, once.** One message in the owner's private chat carrying the one
//   command the owner runs to be done with it — never a credential, and never a second copy of the same
//   warning.

// How close to its end a login has to be for a run to take the account's lock on the way out.
// Wide enough that the CLI's own renewal falls inside it, narrow enough that the ordinary run of
// a day holds nothing at all.
export const refreshWindowMs = 30 * 60_000;

// How a login's end is told, Claude Code's own declaration in agency's policy, which agency's server
// reads too (docs/policy.md).
const declaredLogin = (declaredClaudeCode as { login: LoginDeclaration }).login;

interface LoginDeclaration {
  notice_ahead_hours: number;
  token_life_days: number;
  head: string;
  gone: string;
  ending: string;
  ending_in: string;
  minutes: string;
  hours: string;
  run: string;
  token_command: string;
  directory_command: string;
  token_after: string;
  directory_after: string;
  gone_after: string;
  ending_after: string;
}

// How long before the end the owner is told. It is asked only of a login that has an end to speak of —
// a token account's year, or a login the run itself said is gone.
export const noticeAheadMs = declaredLogin.notice_ahead_hours * 60 * 60_000;

// What a long-lived token the CLI issues stands for, with no refresh of its own. The token itself
// says nothing about its end, so the day its file was written is the day it was issued.
export const tokenLifeMs = declaredLogin.token_life_days * 24 * 60 * 60_000;

/**
 * Where an account's own lock sits: one file per login, under the runtime data root. The locks
 * directory is made by whichever holder of it runs first on a machine, so a renewal does not
 * depend on anything else having stood in a line there before.
 */
export function accountLockPath(account: string): string {
  const path = dataPath("locks", `claude-account-${account}.lock`);

  try {
    mkdirSync(dirname(path), { recursive: true });
  } catch {
    // Nothing to do about it here: the taker will say it could not have the lock, and the run
    // goes out rather than being lost.
  }

  return path;
}

/** How a login is standing: answering, near its end, or gone. */
export type LoginStanding = "standing" | "ending" | "gone";

// What one reading of a credential file yields: the moment it states, and whether it holds
// anything to renew itself with. Both come out of one open, and neither is any part of the
// credential.
interface CredentialReading {
  expiry: number | null;
  refresh: boolean;
}

function readCredential(credential: { kind: string; path: string }): CredentialReading {
  const path = credentialFilePath(credential);

  // An endpoint account holds no login to read: its key states no end and carries no refresh.
  if (path === null) {
    return { expiry: null, refresh: false };
  }

  let text: string;

  try {
    text = readFileSync(path, "utf8");
  } catch {
    return { expiry: null, refresh: false };
  }

  const said = expiryIn(text);

  if (credential.kind !== "token") {
    return { expiry: said, refresh: refreshIn(text) };
  }

  if (said !== null) {
    return { expiry: said, refresh: false };
  }

  // A token carries no ending of its own, so what stands for it is the year it was issued for,
  // counted from the day its file was written.
  try {
    return { expiry: statSync(path).mtimeMs + tokenLifeMs, refresh: false };
  } catch {
    return { expiry: null, refresh: false };
  }
}

/**
 * The moment this credential states: a subscription login's access token stopping, or a
 * long-lived token's year. It is what a renewal is timed against and not what a login's life is
 * measured by — `loginEnd` is that. A file that cannot be read or states no moment answers null.
 */
export function credentialExpiry(credential: { kind: string; path: string }): number | null {
  return readCredential(credential).expiry;
}

/**
 * When this login ends, in the only sense worth telling the owner about: when nothing but a person can
 * carry it on. A file that holds a refresh answers null — the CLI renews that login for itself,
 * and the hour its access token has left says nothing about the login's life. A long-lived token
 * holds no refresh, so its year is the answer; and a login whose refresh has stopped being taken
 * is not in this reading at all, because no file says it — the run that went out on it does.
 */
export function loginEnd(credential: { kind: string; path: string }): number | null {
  const held = readCredential(credential);

  return held.refresh ? null : held.expiry;
}

// The one moment a credential file states, wherever it states it. The shape is the CLI's own and
// is free to move, so what is looked for is a moment rather than a field: any number under a name
// that speaks of expiring, read as seconds or milliseconds by its own size. Nothing else in the
// file is read, kept or returned.
function expiryIn(text: string): number | null {
  let held: unknown;

  try {
    held = JSON.parse(text);
  } catch {
    return null;
  }

  const found: number[] = [];
  const walk = (value: unknown): void => {
    if (value === null || typeof value !== "object") {
      return;
    }

    for (const [name, held] of Object.entries(value as Record<string, unknown>)) {
      if (typeof held === "number" && /expir/i.test(name) && held > 1_000_000_000) {
        found.push(held > 1_000_000_000_000 ? held : held * 1000);
      }

      walk(held);
    }
  };

  walk(held);

  return found.length === 0 ? null : Math.min(...found);
}

// Whether the file holds something to renew itself with. As with the moment, the shape is the
// CLI's own and free to move, so what is looked for is a name that speaks of refreshing with a
// value under it — never the value, which is not read, kept or returned.
function refreshIn(text: string): boolean {
  let held: unknown;

  try {
    held = JSON.parse(text);
  } catch {
    return false;
  }

  let found = false;
  const walk = (value: unknown): void => {
    if (found || value === null || typeof value !== "object") {
      return;
    }

    for (const [name, under] of Object.entries(value as Record<string, unknown>)) {
      if (typeof under === "string" && under !== "" && /refresh/i.test(name)) {
        found = true;

        return;
      }

      walk(under);
    }
  };

  walk(held);

  return found;
}

/**
 * How this login stands. A login already past its end is gone; one inside the warning window is
 * ending; everything else, including a login that renews itself and one whose end nothing can
 * read, is standing — a login is never called gone on a guess, and the hour a subscription
 * login's access token has left is not a guess anybody should be making.
 */
export function loginStanding(account: ClaudeAccountRow, now: number = Date.now()): LoginStanding {
  if (account.needsLoginAt !== null) {
    return "gone";
  }

  // An endpoint account has no login to end and no renewal to be warned about: it stands while its
  // key does, and only a run it refused says otherwise (docs/cache-fallback.md, 「A login that is
  // about to end」).
  if (account.credentialKind === "endpoint") {
    return "standing";
  }

  const ends = loginEnd({ kind: account.credentialKind, path: account.credentialPath });

  if (ends === null) {
    return "standing";
  }

  if (ends <= now) {
    return "gone";
  }

  return ends - now <= noticeAheadMs ? "ending" : "standing";
}

/** A login found gone. The walk passes such an account over until a person signs in again. */
export async function markLoginGone(archive: Archive, account: string, now: number = Date.now()): Promise<void> {
  const held = await archive.claudeAccounts().getClaudeAccount(account);

  if (held === null || held.needsLoginAt !== null) {
    return;
  }

  await archive.claudeAccounts().setClaudeAccountLogin(account, Math.floor(now / 1000));
  console.log(`claude account ${account}: its login is gone and waits for a person`);
}

// Whether an endpoint account's key file was written after `markedAt`, in the epoch seconds the
// row keeps. A missing or empty file is no key at all, and counts as nothing written.
function keyWrittenSince(keyFile: string | null, markedAt: number): boolean {
  if (keyFile === null || keyFile === "") {
    return false;
  }

  try {
    const held = statSync(keyFile);

    return held.isFile() && held.size > 0 && held.mtimeMs > markedAt * 1000;
  } catch {
    return false;
  }
}

/**
 * A login standing again. It is read off the credential rather than off anybody's word: a moment
 * in the future is a login somebody has signed in to since, and the mark goes. An endpoint account
 * has no such moment, so what says it stands again is its key file written after the mark.
 */
export async function clearLoginIfBack(archive: Archive, account: string, now: number = Date.now()): Promise<boolean> {
  const held = await archive.claudeAccounts().getClaudeAccount(account);

  if (held === null || held.needsLoginAt === null) {
    return false;
  }

  // An endpoint account's key states no moment to read, so what says the account is back is the key
  // file itself written after the mark: the owner put a fresh key there, which is the only thing on
  // this side that can say so.
  const back = held.credentialKind === "endpoint"
    ? keyWrittenSince(held.keyFile, held.needsLoginAt)
    : (() => {
        const expiry = credentialExpiry({ kind: held.credentialKind, path: held.credentialPath });

        return expiry !== null && expiry > now;
      })();

  if (!back) {
    return false;
  }

  await archive.claudeAccounts().setClaudeAccountLogin(account, null);
  console.log(`claude account ${account}: its login is standing again`);

  return true;
}

/**
 * The one command the owner runs to sign this account in again. A token account is issued a new token
 * by the CLI's own command for it; a directory account is signed in by the CLI's login, which the
 * command hands over as the run's first word. Either way the account's own configuration directory
 * goes in front, because that is what decides which login is being signed in.
 */
export function loginCommand(account: ClaudeAccountRow): string {
  // An endpoint account holds no login, so there is no command that would make one; nothing asks
  // for it either, since no warning is ever sent about such an account.
  if (account.credentialKind === "endpoint") {
    throw new Error(`Account ${account.id} holds no login, so it has no command to make one.`);
  }

  return (account.credentialKind === "token" ? declaredLogin.token_command : declaredLogin.directory_command).replace("{path}", account.credentialPath);
}

// What a warning about one ending is recorded under, so the same ending is said once and not on
// every run that reads it.
const noticeKey = (account: string) => `claude_login_notice:${account}`;

/**
 * The owner, told before a login ends — once for that ending, whatever happens afterwards. What the
 * message carries is the account, when it ends, and the one command the owner runs; the credential itself
 * is never in it, and neither is anything out of its file.
 *
 * A login already gone is told the same way: the message then says it has ended rather than that
 * it is about to.
 */
export async function tellAboutLogin(
  deps: { archive: Archive },
  account: ClaudeAccountRow,
  now: number = Date.now(),
): Promise<boolean> {
  // An endpoint account has no login to end, so there is nothing to warn about and no command to
  // sign one in with (docs/cache-fallback.md, 「A login that is about to end」).
  if (account.credentialKind === "endpoint") {
    return false;
  }

  const standing = loginStanding(account, now);

  if (standing === "standing") {
    return false;
  }

  const expiry = loginEnd({ kind: account.credentialKind, path: account.credentialPath });
  // One warning per ending: the moment itself is the mark, so a login signed in again — a new
  // moment — is warned about again when its turn comes.
  const about = String(expiry ?? (account.needsLoginAt ?? 0));

  if (await deps.archive.settings().getSetting(noticeKey(account.id)) === about) {
    return false;
  }

  const left = expiry === null ? null : Math.max(0, expiry - now);
  const wording = declaredLogin;
  const span = left === null
    ? null
    : left < 60 * 60_000
      ? wording.minutes.replace("{n}", String(Math.max(1, Math.round(left / 60_000))))
      : wording.hours.replace("{n}", String(Math.round(left / (60 * 60_000))));
  const when = standing === "gone" ? wording.gone : span === null ? wording.ending : wording.ending_in.replace("{left}", span);

  const text = [
    wording.head.replace("{name}", account.name).replace("{when}", when),
    wording.run,
    loginCommand(account),
    account.credentialKind === "token" ? wording.token_after : wording.directory_after,
    standing === "gone" ? wording.gone_after : wording.ending_after,
  ].join("\n");

  // Published for the consumer that holds the owner's chat, which says it there word for word, since a
  // login is the owner's to answer and nobody else's (docs/api.md, 「Events」).
  emitAgencyEvent("account-login-notice", {
    account: account.id,
    login: "here",
    stands: standing === "gone" ? "gone" : "ending",
    leftMs: left,
    notice: text,
  });

  // Said, and said once: every pass that reads this same ending afterwards stays quiet.
  await deps.archive.settings().setSetting(noticeKey(account.id), about);

  return true;
}

/**
 * One run, on a login whose credential is renewed by exactly one of them. Where the login is
 * nowhere near its end — which is nearly always — nothing is taken and the run goes out at once.
 * Where it is close, the account's own lock is taken first: whoever holds it is the run whose CLI
 * renews the login, and every other run of that account waits and then goes out on what that
 * renewal wrote. The lock is the renewal's, not the run's: it is taken only inside that window.
 * A waiting run waits for its event, not for a span: the credential no longer near its end, or the
 * holder letting the lock go, its process ending among the ways.
 */
export async function withRenewedLogin<T>(
  deps: { archive: Archive | null },
  account: { id: string; credential: { kind: string; path: string } } | null | undefined,
  go: () => Promise<T>,
  now: () => number = Date.now,
): Promise<T> {
  if (deps.archive === null || account === null || account === undefined) {
    return go();
  }

  const due = (): boolean => {
    const expiry = credentialExpiry(account.credential);

    return expiry !== null && expiry - now() <= refreshWindowMs;
  };

  if (!due()) {
    return go();
  }

  const held = await takeLock(accountLockPath(recordedAccount(account)), { waitedOut: () => !due() });

  if (held === null) {
    // Another run renewed it while this one waited: what it wrote is what this run uses.
    console.log(`claude account ${account.id}: another run renewed its login, using what it wrote`);

    return go();
  }

  try {
    if (!due()) {
      // Another run renewed it while this one waited: what it wrote is what this run uses.
      console.log(`claude account ${account.id}: another run renewed its login, using what it wrote`);
    }

    return await go();
  } finally {
    await held.release();
  }
}
