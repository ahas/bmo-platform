import { accountFailure, ClaudeLimitError } from "./claude-failure";
import { chosenClaudeAccount, claudeAccountInUse, claudeAccounts, enabledClaudeAccounts, endpointKeyHeld, holdClaudeAccount, recordedAccount } from "./claude-account";
import { providerSettingsArchive } from "./provider-order";
import { clearLoginIfBack, markLoginGone, tellAboutLogin, withRenewedLogin } from "./claude-login";
import { isAccountResting, restFallbackDefaultMs, restUntilFor, writeRest } from "./rest-state";

import type { Archive } from "@agency/archive/archive";
import type { ClaudeAccount } from "./claude-account";
import type { AccountFailure } from "./claude-failure";
import type { RestSettings } from "./rest-state";
import type { HarnessRun, AgentAccount } from "./harness";
import { filterInOrder } from "@agency/support/in-order";

// The Claude road, walked login by login. Agency holds several Claude accounts, and a run has to go
// out on one of them: this is the one place that says which, and the one place that moves a run to
// the next login when the one it went out on is the thing that failed.
//
// Why it is one place. A run fails for two quite different reasons — the work went wrong, or the
// login did — and only the second is somebody else's to carry. A caller that read the difference
// for itself would read it differently from the next caller, and a login at its ceiling would be
// retried by one line and given up on by another. So the judgement is made here, from what the
// harness says about the finished run, and every caller asks the same question.
//
// What it is not. It does not walk the providers: that is the provider order's. This walk sits
// inside the Claude entry of that order — the entry is open while any login is open — and a run
// that exhausts every login throws what the last one threw, which is what sends the caller down the
// provider order.

/**
 * Whether a login will answer now: it is on, its key is there where it is an endpoint account, its
 * own window is not full, and its login has not gone. A login waiting for a person answers nothing
 * at all, so the walk passes it over exactly as it passes over one at its ceiling — and a login
 * somebody has signed in to again is read as standing the moment its credential says so.
 */
export async function claudeAccountOpen(archive: Archive, account: string): Promise<boolean> {
  const held = (await enabledClaudeAccounts(archive)).find((one) => one.id === account);

  if (held === undefined || !endpointKeyHeld(held.credential) || await isAccountResting(archive, account)) {
    return false;
  }

  return !await needsLogin(archive, account);
}

// Whether this login is waiting for a person. A login marked gone is asked once more of its own
// credential: the owner signing in again writes a new moment there, and the mark goes rather than
// standing until something else notices.
async function needsLogin(archive: Archive, account: string): Promise<boolean> {
  if ((await archive.claudeAccounts().getClaudeAccount(account))?.needsLoginAt === null) {
    return false;
  }

  return !await clearLoginIfBack(archive, account);
}

/**
 * The logins the road offers a run, in order: the login in use first while it can
 * answer, then the open ones in the owner's order, started on the account the owner chose to run on now
 * where the owner chose one, and where none is open the chosen account and the order as they stand, so the
 * run fails honestly on the first login instead of not happening at all. A login in use is used to the end:
 * an account ahead of it, or the chosen one, answering again moves nothing.
 */
export async function openClaudeAccounts(archive: Archive): Promise<ClaudeAccount[]> {
  const chosen = await chosenClaudeAccount(archive);
  const enabled = await enabledClaudeAccounts(archive);
  const ordered = chosen === null ? enabled : [chosen, ...enabled.filter((one) => one.id !== chosen.id)];
  const open = await filterInOrder(ordered, (account) => claudeAccountOpen(archive, account.id));
  const inUse = await claudeAccountInUse(archive);
  const walk = inUse === null || !open.some((one) => one.id === inUse)
    ? open
    : [...open.filter((one) => one.id === inUse), ...open.filter((one) => one.id !== inUse)];

  return walk.length > 0 ? walk : ordered;
}

/**
 * The login an automatic run goes out on now, or nothing where no account is registered at all.
 * It is the head of that walk rather than the first row of the order: a login at its ceiling or
 * waiting for a person is passed over, so its own figures say nothing about what a run will meet,
 * and whoever shows what the account has left has to show this one's.
 */
export async function walkingClaudeAccount(archive: Archive): Promise<string | null> {
  return (await openClaudeAccounts(archive))[0]?.id ?? null;
}

// A setting's leading whole number, as agency's Rust side reads one; none where it holds none.
function leadingInteger(value: string | null): number | null {
  const found = value?.trim().match(/^[+-]?\d+/);

  return found === null || found === undefined ? null : Number(found[0]);
}

/**
 * Which accounts may carry a run made for a user (docs/store.md): a
 * user's run goes on that user's own accounts alone, and a run made for no user — the platform's
 * own work — on the administrator's and on any account no user owns. Off the multi-user model no
 * account names a user, and every account carries every run.
 */
async function carriers(archive: Archive, user: number | null): Promise<(account: string) => boolean> {
  const owners = new Map((await claudeAccounts(archive)).map((account) => [account.id, account.userId]));
  const administrator = user === null ? leadingInteger(await archive.settings().getSetting("users_administrator")) : null;

  return (account) => {
    const owner = owners.get(account) ?? null;

    return user === null ? owner === null || administrator === null || owner === administrator : owner === user;
  };
}

/**
 * The logins a run is offered, in order, for the user it is made for: the walk above, the same for every
 * run, with no login the run's user may not run on (docs/cache-fallback.md, 「The account judgement」).
 */
export async function claudeAccountWalk(archive: Archive, user: number | null = null): Promise<AgentAccount[]> {
  const carries = await carriers(archive, user);

  return (await openClaudeAccounts(archive))
    .filter((account) => carries(account.id))
    .map((account) => ({ id: account.id, name: account.name, credential: account.credential }));
}

// What a run that answered leaves behind: the login it went out on becomes the one in use where no
// login is in use or the one in use can no longer answer. A run that answers on another login while
// the one in use still answers — one that went out before the road moved and finished after — does
// not move it back.
async function holdWhereMoved(archive: Archive, account: string): Promise<void> {
  const inUse = await claudeAccountInUse(archive);

  if (inUse === account || (inUse !== null && await claudeAccountOpen(archive, inUse)) || !await claudeAccountOpen(archive, account)) {
    return;
  }

  await holdClaudeAccount(archive, account);
}

export interface ClaudeDispatchDeps {
  archive: Archive | null;
  // The configuration where the caller has one. A judgement's own run has none, and the only
  // thing it would be asked for is how long to put a login aside when the limit named no reset
  // moment, which has one answer whether or not a configuration is in hand.
  config?: Pick<RestSettings, "restFallbackMs"> | null;
  // The user the run is made for, whose own logins alone carry it; the platform's own work where
  // none is named (docs/store.md).
  user?: number | null;
}

/** The deps a caller on the installed machine has: the installed store, and its configuration where it holds one. */
export function installedDispatch(config?: Pick<RestSettings, "restFallbackMs"> | null): ClaudeDispatchDeps {
  return { archive: providerSettingsArchive(), config: config ?? null };
}

/**
 * One run, carried by the logins the key is offered. The attempt is handed the run with the login
 * it goes out on named on it; where it fails on the login rather than on the work, the next open
 * login carries it and the caller never learns that the first one was tried. A login at its
 * ceiling is put to rest as it is passed over, so nothing else asks it again until its window
 * resets.
 *
 * With no store installed — a fixture holding no accounts — the attempt is made once, exactly as
 * every run was made before there was more than one login.
 */
export async function onClaudeAccounts<T>(
  deps: ClaudeDispatchDeps,
  key: string,
  run: HarnessRun,
  attempt: (run: HarnessRun) => Promise<T>,
): Promise<T> {
  const archive = deps.archive;

  if (archive === null) {
    return attempt(run);
  }

  const user = deps.user ?? null;
  const walk = await claudeAccountWalk(archive, user);

  if (walk.length === 0 && user !== null) {
    throw new Error(`claude ${key}: no Claude account of user ${user} is there to carry this run`);
  }

  if (walk.length === 0) {
    return attempt(run);
  }

  let last: unknown = new Error(`claude ${key}: no login answered`);

  for (const [place, account] of walk.entries()) {
    try {
      // The login is renewed by one run and not by several: where its credential is close to its
      // end, this run takes the account's own lock on the way out and the others wait for what it
      // wrote. Away from that window nothing is held at all.
      const result = await withRenewedLogin({ archive }, account, () => attempt({ ...run, account }));

      await holdWhereMoved(archive, account.id);

      return result;
    } catch (error) {
      const failure = accountFailure({ error });

      if (failure === null || failure === "outage") {
        throw error;
      }

      last = error;
      await passOver({ ...deps, archive }, account, failure, error);

      const next = walk[place + 1];

      console.log(
        `claude ${key}: ${account.id} ${failure === "limit" ? "is at its ceiling" : "needs a login"}` +
          `${next === undefined ? ", and no login is left" : `, carried by ${next.id}`}`,
      );
    }
  }

  throw last;
}

/**
 * What passing a login over leaves behind. A window that is full is a rest for that login alone —
 * written here so no caller has to, and so nothing asks that login again until it resets — and a
 * login that is gone waits for a person, which is the login watch's to record.
 */
async function passOver(
  deps: ClaudeDispatchDeps & { archive: Archive },
  account: AgentAccount,
  failure: AccountFailure,
  error: unknown,
): Promise<void> {
  if (failure === "login") {
    // A login the run itself said was gone, or one whose own credential has ended: it waits for a
    // person, and the walk passes it over until somebody signs in again. The owner hears of it once.
    await markLoginGone(deps.archive, recordedAccount(account));

    const held = await deps.archive.claudeAccounts().getClaudeAccount(recordedAccount(account));

    if (held !== null && deps.config !== null && deps.config !== undefined) {
      await tellAboutLogin({ archive: deps.archive }, held);
    }

    return;
  }

  if (failure !== "limit") {
    return;
  }

  const resetsAt = error instanceof ClaudeLimitError ? error.resetsAt : null;
  const settings = deps.config ?? { restFallbackMs: restFallbackDefaultMs };

  await writeRest(deps.archive, restUntilFor(settings, resetsAt), "limit", recordedAccount(account));
}
