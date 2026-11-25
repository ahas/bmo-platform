import { closeSync, mkdirSync, openSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";

// A lock on a file held by a process of this one's own, for as long as this process wants it
// rather than for its whole life. The holder is flock(1) keeping a pipe open: the lock is taken by
// the same flock(2) call every `flock <file> <command>` makes, so it excludes those runs as much
// as it excludes another holder, and it goes the moment the pipe closes — on release, or when this
// process dies without releasing.
//
// flock alone hands a released lock to whichever waiter the kernel wakes, so a process that lets
// the lock go and asks again at once can win it back over and over while others wait out their
// time. So every taker first stands a ticket in a queue beside the lock and goes for the lock only
// once no living ticket stands ahead of its own; the ticket stays while the lock is held and goes
// on release. A ticket whose process is gone is taken away.
//
// A taker waits for its turn however long it takes: no clock gives up the wait. It leaves the queue
// only where the event it names says it need not wait any more.
//
// A taker may stand at the front: its ticket sorts ahead of every ordinary one, so it takes the lock
// the moment the holder lets go, whoever asked before it. Front takers among themselves keep the
// order they asked in. The holder is never cut: standing at the front waits for the lock all the
// same. A taker may stand next instead: behind every front ticket and ahead of every ordinary one,
// in the order such takers asked.

export interface HeldLock {
  release: () => Promise<void>;
}

// How often a queued taker looks whether its turn has come.
const queuePollMs = 500;

let ticketCounter = 0;

function queueDirOf(path: string): string {
  return `${path}.queue`;
}

// A ticket's name sorts by when it was stood, and carries the process that stood it. A front
// ticket's name opens with a mark that sorts ahead of every time, and a next ticket's with one that
// sorts behind the front's and still ahead of every time, which opens with a zero digit.
const frontMark = "0-";
const nextMark = "0.";

function ticketName(rank: "front" | "next" | null): string {
  ticketCounter += 1;

  const mark = rank === "front" ? frontMark : rank === "next" ? nextMark : "";

  return `${mark}${String(Date.now()).padStart(15, "0")}-${String(ticketCounter).padStart(6, "0")}-${process.pid}`;
}

function ticketPid(name: string): number {
  return Number.parseInt(name.slice(name.lastIndexOf("-") + 1), 10);
}

// A ticket's file holds the name its taker stands under.

/** A ticket as the queue holds it. */
export interface QueueTicket {
  // The process standing it: the taker, or the stand-in keeping its place.
  pid: number;
  // The name its taker stands under; empty where it gave none.
  holder: string;
  // Whether it stands at the front, ahead of every ordinary ticket.
  front: boolean;
  // Whether it stands next: behind the front, ahead of every ordinary ticket.
  next: boolean;
}

/**
 * The living tickets of a lock's queue, in the order they stand: the holder's among them while the
 * lock is held, behind any front ticket stood since. Nothing is taken away; a ticket whose process is
 * gone is left out.
 */
export function queueTickets(path: string): QueueTicket[] {
  let names: string[];

  try {
    names = readdirSync(queueDirOf(path)).sort();
  } catch {
    return [];
  }

  return names.flatMap((name) => {
    const pid = ticketPid(name);

    if (!processAlive(pid)) return [];

    try {
      const lines = readFileSync(join(queueDirOf(path), name), "utf8").split("\n");

      return [{ pid, holder: lines[0] ?? "", front: name.startsWith(frontMark), next: name.startsWith(nextMark) }];
    } catch {
      return [];
    }
  });
}

/** Whether a process of this machine is still running. */
export function processAlive(pid: number): boolean {
  if (!Number.isInteger(pid) || pid <= 0) {
    return false;
  }

  try {
    process.kill(pid, 0);

    return true;
  } catch (error) {
    // Another user's process answers EPERM, and it is alive all the same.
    return (error as NodeJS.ErrnoException).code === "EPERM";
  }
}

/**
 * The living ticket standing ahead of this one, as the name it was stood under (empty where its
 * taker gave none), or undefined when none stands ahead. A ticket whose process is gone is taken
 * away.
 */
function ticketAhead(queue: string, ticket: string): string | undefined {
  let names: string[];

  try {
    names = readdirSync(queue).sort();
  } catch {
    return undefined;
  }

  for (const name of names) {
    if (name >= ticket) {
      return undefined;
    }

    if (!processAlive(ticketPid(name))) {
      rmSync(join(queue, name), { force: true });
      continue;
    }

    try {
      return readFileSync(join(queue, name), "utf8").split("\n")[0] ?? "";
    } catch {
      // Let go between the listing and the read: the next look finds the queue as it now is.
      return "";
    }
  }

  return undefined;
}

export interface LockWait {
  // The name this taker stands in the queue under.
  holder?: string;
  // The event that ends the wait without the lock: looked at on every look, and the taker leaves the
  // queue and answers null once it says so. With none, the taker waits until it has the lock.
  waitedOut?: () => boolean;
  // Stand at the front of the queue, ahead of every ordinary ticket (above).
  front?: boolean;
  // Stand next, behind the front and ahead of every ordinary ticket (above); the front wins where both are asked.
  next?: boolean;
  // How often a queued taker looks whether its turn has come; a test sets it short.
  pollMs?: number;
}

/** The lock once this taker's turn has come and the holder let go, or null where the event the taker named came first. */
export async function takeLock(path: string, wait: LockWait = {}): Promise<HeldLock | null> {
  closeSync(openSync(path, "a"));

  const queue = queueDirOf(path);

  mkdirSync(queue, { recursive: true });

  const ticket = ticketName(wait.front === true ? "front" : wait.next === true ? "next" : null);
  const pollMs = wait.pollMs ?? queuePollMs;
  const ticketPath = join(queue, ticket);

  writeFileSync(ticketPath, wait.holder ?? "");

  const leave = (): void => rmSync(ticketPath, { force: true });

  // A taker goes for the lock only while no living ticket stands ahead of its own, and only without
  // waiting on it: a taker blocked inside flock would be handed the lock by the kernel over a ticket
  // stood ahead of it meanwhile — a front ticket over a next one standing ahead of the holder's own.
  for (;;) {
    if (ticketAhead(queue, ticket) === undefined) {
      const holder = await tryHolding(path);

      if (holder !== null) return holding(holder, leave);
    }

    if (wait.waitedOut?.() === true) {
      leave();

      return null;
    }

    await Bun.sleep(pollMs);
  }
}

// The lock taken without waiting: the flock process keeping it, or null where another holds it.
async function tryHolding(path: string): Promise<Bun.Subprocess<"pipe", "pipe", "ignore"> | null> {
  const holder = Bun.spawn(["flock", "-n", path, "sh", "-c", "echo held; exec cat > /dev/null"], { stdin: "pipe", stdout: "pipe", stderr: "ignore" });
  const reader = holder.stdout.getReader();
  let said = "";

  while (!said.includes("\n")) {
    const { value, done } = await reader.read();

    if (done) break;

    said += new TextDecoder().decode(value);
  }

  reader.releaseLock();

  if (said.startsWith("held")) return holder;

  await holder.exited;

  return null;
}

// The lock as its taker holds it: the flock process kept open, and the ticket that goes with it.
function holding(holder: Bun.Subprocess<"pipe", "pipe", "ignore">, leave: () => void): HeldLock {
  let released = false;

  return {
    release: async () => {
      if (released) {
        return;
      }

      released = true;
      await holder.stdin.end();
      await holder.exited;
      leave();
    },
  };
}

// Where a checkout's job lock is kept: one file per working directory under the data root's
// locks, named so a person reading the directory can tell which checkout it belongs to.
//
// The name is two halves, and each answers something the other cannot. The slug is for a person
// reading the lock directory: it says which checkout the lock belongs to. It is lossy on purpose
// — every run of characters a file name will not take collapses to a single hyphen, and a path
// made of nothing else is left with no slug at all — so it can never be what tells two
// directories apart, and two paths differing only in those characters would otherwise queue
// behind one lock while sharing nothing. The digest is what separates them, and it is taken over
// the whole path rather than over the slug, so directories that differ anywhere differ here.
const lockSlugMax = 64;
const lockDigestChars = 12;

export function lockFileName(cwd: string): string {
  const slug = cwd.replaceAll(/[^A-Za-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "");
  // A file name is bounded and a path is not, so a long one is cut from the front: the tail is
  // the part that names the checkout, and the digest carries what the cut throws away.
  const kept = slug.slice(Math.max(0, slug.length - lockSlugMax)).replace(/^-+/, "");
  const digest = createHash("sha256").update(cwd).digest("hex").slice(0, lockDigestChars);

  return `${kept === "" ? "root" : kept}-${digest}.lock`;
}

export function workerLockPath(dataDir: string, cwd: string): string {
  return join(dataDir, "locks", lockFileName(cwd));
}

// A worker's pane lock beside the checkouts' locks, named by the worker, so two workers pointed at
// one checkout each hold their own (docs/agent-control.md, 「A worker's pane lock」).
export function workerPaneLockPath(dataDir: string, worker: string): string {
  return join(dataDir, "locks", `pane-${lockFileName(worker)}`);
}
