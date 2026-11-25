// One archive file, many writers: the bot, the web console, every script a job runs. SQLite lets
// one of them write at a time, and this is the whole of how the rest behave while it does.

import type { Database } from "bun:sqlite";
import { AsyncLocalStorage } from "node:async_hooks";
import { positiveFromEnv } from "@agency/support/env-number";

// How far the write-ahead log may stand once it has been checkpointed. Without a limit the file
// only ever grows to its high-water mark and stays there: the live archive was found holding half
// a megabyte of frames in a sixty-eight megabyte file.
export const walLimitBytes = 32 * 1024 * 1024;

/**
 * How long one attempt waits for whoever holds the write lock. Measured against the live archive
 * with three processes writing agent-log lines flat out: the lock is normally free inside 11 ms,
 * but a burst can hold it for over six seconds, and a five-second budget was exhausted outright in
 * two attempts of forty. Ten seconds covers the worst measured stretch with margin. It paces the
 * attempts rather than ending the work: every writer here is on a thread that draws chats and
 * answers routes, so one attempt's wait is that thread held, and the work is started over after it
 * for as long as the lock stays taken.
 */
export function lockWaitMs(): number {
  return positiveFromEnv(process.env.BMO_ARCHIVE_LOCK_WAIT_MS, 10_000);
}

// The longest one backoff waits, so a retry's own pause never rivals the wait it follows.
const maxBackoffMs = 1_000;

/** Whether an error is SQLite refusing because another connection holds the lock. */
export function archiveBusy(error: unknown): boolean {
  const code = (error as { code?: unknown } | null)?.code;

  return typeof code === "string" && (code.startsWith("SQLITE_BUSY") || code === "SQLITE_LOCKED")
    || (error instanceof Error && /database is (locked|busy)/.test(error.message));
}

// What `run` a connection has to offer to be settled here, which is all `bun:sqlite` and every
// stand-in of it needs to have in common.
export interface LockedStore {
  run(sql: string): unknown;
}

// The levels SQLite's sync setting takes, by the names it gives them.
const syncLevels = new Set(["off", "normal", "full", "extra"]);

// How far a commit waits for the disk: the level the environment names where it is one SQLite
// knows, and `normal` otherwise. In WAL mode `normal` leaves the waiting to the checkpoint, since a
// sync on a disk something else is writing to can hold one commit, and the write lock with it, for
// seconds; a run whose archives are thrown away with their directory may name `off`.
function archiveSyncLevel(): string {
  const level = process.env.BMO_ARCHIVE_SYNC?.trim().toLowerCase() ?? "";

  return syncLevels.has(level) ? level : "normal";
}

/** The settings a connection needs before it shares an archive with the other writers. */
export function shareArchive(db: LockedStore): void {
  db.run(`PRAGMA busy_timeout = ${lockWaitMs()}`);
  db.run(`PRAGMA journal_size_limit = ${walLimitBytes}`);
  db.run(`PRAGMA synchronous = ${archiveSyncLevel()}`);
}

export interface LockWait {
  backoffMs?: number;
  sleep?: (ms: number) => unknown;
  now?: () => number;
}

/**
 * What this process has met of the lock since it started. Kept because the only account of the
 * lock there has ever been is the failures it left behind, which counts the times the retries did
 * not save the write and nothing at all of the times they did — so a burst that cost the bot
 * twenty seconds and lost nothing reads exactly like an hour of quiet.
 *
 * `met` is the refusals seen, `saved` the pieces of work that a retry carried through, and
 * `waitedMs` the time spent waiting on the lock rather than working.
 */
export interface LockTally {
  met: number;
  saved: number;
  waitedMs: number;
  longestMs: number;
}

const tally: LockTally = { met: 0, saved: 0, waitedMs: 0, longestMs: 0 };

/** The tally as it stands. */
export function lockTally(): LockTally {
  return { ...tally };
}

/** Takes the tally and leaves it empty, for a writer that has just recorded what it held. */
export function takeLockTally(): LockTally {
  const held = lockTally();

  tally.met = 0;
  tally.saved = 0;
  tally.waitedMs = 0;
  tally.longestMs = 0;

  return held;
}

/**
 * Runs one piece of archive work, and runs it again while the lock is what refused it.
 *
 * The wait above is not always spent. A transaction that reads before it writes cannot wait at
 * all — SQLite refuses it the instant the lock is somebody else's, because a waiter holding a read
 * would only deadlock — so the wait goes unused and the refusal arrives in a millisecond. What
 * answers that is starting the work over, which takes a fresh snapshot and a fresh place in the
 * queue, for as long as the lock is somebody else's: the work waits for the lock however long
 * that takes.
 */
export async function throughLock<T>(work: () => Promise<T>, options: LockWait = {}): Promise<T> {
  const now = options.now ?? Date.now;
  const sleep = options.sleep ?? Bun.sleep;
  const backoff = options.backoffMs ?? 50;
  const started = now();
  // One write is one thing met, however many of these it passed through on the way: the archive
  // retries a transaction of its own inside the retries a caller asked for, and a tally that counted
  // both would say the lock was met twice where a writer met it once. Which retries are outermost
  // is a question of the piece of work, not of the process, since several run at once.
  const counting = retrying.getStore() === undefined;
  let met = false;

  return retrying.run(true, async () => {
    for (let attempt = 0; ; attempt += 1) {
      try {
        const answer = await work();

        if (met && counting) count("saved", now() - started);

        return answer;
      } catch (error) {
        if (!archiveBusy(error)) throw error;

        met = true;

        if (counting) tally.met += 1;

        await sleep(Math.min(backoff * 2 ** attempt, maxBackoffMs));
      }
    }
  });
}

// The settling's own form of the same, for the schema work the bot's start and the deployment run
// over a bare connection before anything else touches it, where there is nothing to wait beside.
function throughLockNow<T>(work: () => T): T {
  const started = Date.now();
  let met = false;

  for (let attempt = 0; ; attempt += 1) {
    try {
      const answer = work();

      if (met) count("saved", Date.now() - started);

      return answer;
    } catch (error) {
      if (!archiveBusy(error)) throw error;

      met = true;
      tally.met += 1;

      Bun.sleepSync(Math.min(50 * 2 ** attempt, maxBackoffMs));
    }
  }
}

// Set inside a piece of work the retries above already count.
const retrying = new AsyncLocalStorage<true>();

function count(outcome: "saved", waitedMs: number): void {
  tally[outcome] += 1;
  tally.waitedMs += waitedMs;
  tally.longestMs = Math.max(tally.longestMs, waitedMs);
}

/**
 * The same, for archive work whose loss is not worth what it is part of — a log line, a row that
 * records what already happened. It waits for the lock as every write does; the name it is given
 * says what the work is, and the answer is the work's, or null where the work answered none.
 */
export async function keptThroughLock<T>(_what: string, work: () => Promise<T>, options: LockWait = {}): Promise<T | null> {
  return await throughLock(work, options);
}

// The settling's transaction over a bare connection (docs/store.md): taken immediate and
// run again where the lock refused it, as every transaction of the archive is. SQLite will not let
// a transaction that has already read wait for the lock — it refuses on the spot rather than hold a
// read while it queues — so a deferred transaction is one the busy wait never covers at all.
export function writing<T>(db: Database, work: () => T): T {
  return throughLockNow(() => db.transaction(work).immediate());
}
