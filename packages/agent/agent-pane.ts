// What a host agent's view is written with (docs/agent-control.md, 「The view」): what each
// execution's program says as it streams, and how it ended. The collector writes these lines into
// the agent's view, which its head pane follows, so the host never reads a provider's output.

import { closeSync, fstatSync, openSync, readSync } from "node:fs";
import { join } from "node:path";

// One execution as its view follows it: its name, where its envelope stands, whose raw output is
// read to show it as it streams, and the provider whose program it runs, which is the one that knows
// how that output reads.
export interface WatchedExecution {
  name: string;
  directory: string;
  provider: string | null;
}

export interface FinishedExecution {
  name: string;
  title: string;
  at: number;
  cause: string;
  exitCode: number | null;
}

// How wide a line is kept: long enough for a sentence, short enough that a pane never wraps a
// streamed paragraph into a wall.
export const progressWidth = 160;

function clock(at: number): string {
  return new Intl.DateTimeFormat("en-GB", { timeZone: "Asia/Seoul", hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).format(new Date(at));
}

// A line as the pane holds it: one line whatever it came as, and cut with a mark where it is longer.
export function flatLine(text: string, width: number = progressWidth): string {
  const flat = text.replace(/\s+/g, " ").trim();

  return flat.length <= width ? flat : `${flat.slice(0, width - 1).trimEnd()}…`;
}

export function progressLine(text: string): string {
  return `   ${flatLine(text)}`;
}

export function endedLine(execution: FinishedExecution): string {
  const how = execution.cause === "exited" ? (execution.exitCode === 0 ? "ended" : `ended · exit code ${execution.exitCode}`) : execution.cause;

  return `── ${clock(execution.at)} ${execution.title} ${how}`;
}

// How one line of a program's raw output reads to a person, or null for a line that says nothing to
// one: the provider's own reader.
export type ProgressReader = (line: string) => string | null;

// What of a file lies past `offset`: up to its last ended line, or the whole of it where `whole` asks.
// A cut is only ever made just after a newline, which is never a byte of a longer character.
function unread(path: string, offset: number, whole: boolean): { text: string; next: number } {
  let fd: number;

  try {
    fd = openSync(path, "r");
  } catch {
    return { text: "", next: offset };
  }

  try {
    const size = fstatSync(fd).size;

    if (size <= offset) return { text: "", next: offset };

    const bytes = Buffer.alloc(size - offset);
    const got = readSync(fd, bytes, 0, bytes.length, offset);
    const end = whole ? got : bytes.subarray(0, got).lastIndexOf(0x0a) + 1;

    return { text: bytes.subarray(0, end).toString("utf8"), next: offset + end };
  } finally {
    closeSync(fd);
  }
}

// What each execution's program wrote that its view has not taken yet, as the view's lines. Every
// look is synchronous, so the looks as output arrives and the last look before the outcome never
// interleave, and each piece of output is written once whichever of them reaches it.
export class OutputFollower {
  readonly #seen = new Map<string, { offset: number; ended: boolean }>();

  constructor(private readonly readerOf: (provider: string | null) => ProgressReader | undefined) {}

  // The lines said since the last look. The last look — `ended` — also takes a last line no newline
  // closed, and after it the execution says nothing more.
  look(execution: WatchedExecution, ended = false): string[] {
    const seen = this.#seen.get(execution.name) ?? { offset: 0, ended: false };
    const reader = this.readerOf(execution.provider);

    if (seen.ended || reader === undefined) return [];

    const { text, next } = unread(join(execution.directory, "stdout"), seen.offset, ended);

    this.#seen.set(execution.name, { offset: next, ended });

    return text.split("\n").map((line) => reader(line)).filter((said): said is string => said !== null).map(progressLine);
  }
}
