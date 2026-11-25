import { closeSync, fstatSync, openSync, readSync } from "node:fs";

// A Claude transcript read as it grows. A turn in a pane is watched by looking at its record every
// second, several times a look, and a chat session's record runs to tens of megabytes; parsed whole
// on every look it holds the watching process's one thread for most of each second. So the lines already parsed are kept, and each look reads only the bytes written since
// the last complete line it took.
//
// What a look answers is exactly what parsing the whole file would: every line that is JSON, in
// order, a line that is not passed over, and a last line still being written counted only if it
// already parses. The kept lines are thrown away and the file read again from its start whenever
// it is not the file they came from: another file under the path, a file shorter than what was
// taken, or bytes before the point taken that no longer read as they did.

interface Tail {
  ino: number;
  // Where the next look starts: just past the last newline taken.
  offset: number;
  // The bytes just before that point, to know the file under it is still the one they came from.
  seam: Buffer;
  records: unknown[];
  usedAt: number;
}

const seamBytes = 64;
// A tail nobody has looked at for this long belongs to a turn that has ended.
const idleMs = 60_000;
const tailsKept = 16;
const tails = new Map<string, Tail>();

function open(path: string): number | null {
  try {
    return openSync(path, "r");
  } catch {
    return null;
  }
}

function read(fd: number, from: number, length: number): Buffer {
  const buffer = Buffer.alloc(length);
  let got = 0;

  while (got < length) {
    const n = readSync(fd, buffer, got, length - got, from + got);

    if (n === 0) break;
    got += n;
  }

  return got === length ? buffer : buffer.subarray(0, got);
}

function parse(text: string, into: unknown[]): void {
  for (const line of text.split("\n")) {
    const trimmed = line.trim();

    if (trimmed === "") continue;

    try {
      into.push(JSON.parse(trimmed));
    } catch {
      continue;
    }
  }
}

function forget(now: number): void {
  for (const [path, tail] of tails) {
    if (now - tail.usedAt > idleMs) tails.delete(path);
  }

  while (tails.size > tailsKept) tails.delete(tails.keys().next().value!);
}

/** Every JSON line of the file at `path`, in order; nothing where there is no file. */
export function transcriptRecords(path: string, now: number = Date.now()): unknown[] {
  forget(now);

  const fd = open(path);

  if (fd === null) {
    tails.delete(path);

    return [];
  }

  try {
    const stat = fstatSync(fd);
    let tail = tails.get(path);

    if (tail !== undefined) {
      const seamFrom = tail.offset - tail.seam.length;
      const intact = tail.ino === stat.ino && stat.size >= tail.offset && read(fd, seamFrom, tail.seam.length).equals(tail.seam);

      if (!intact) tail = undefined;
    }

    tail ??= { ino: stat.ino, offset: 0, seam: Buffer.alloc(0), records: [], usedAt: now };
    tail.usedAt = now;
    tails.delete(path);
    tails.set(path, tail);

    const fresh = read(fd, tail.offset, stat.size - tail.offset);
    const end = fresh.lastIndexOf(0x0a);

    if (end >= 0) {
      parse(fresh.subarray(0, end + 1).toString("utf8"), tail.records);
      tail.offset += end + 1;
      tail.seam = read(fd, Math.max(0, tail.offset - seamBytes), Math.min(seamBytes, tail.offset));
    }

    // A last line with no newline yet is taken only if it already reads whole, and it is not kept:
    // the next look reads it again with whatever has been written after it.
    const unfinished: unknown[] = [];

    parse(fresh.subarray(end + 1).toString("utf8"), unfinished);

    // A copy, so a caller holding one look across an await still holds that look.
    return [...tail.records, ...unfinished];
  } finally {
    closeSync(fd);
  }
}
