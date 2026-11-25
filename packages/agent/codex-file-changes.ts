import { closeSync, fstatSync, openSync, readSync } from "node:fs";
import { join } from "node:path";

import { homeDir } from "@agency/support/roots";

// What Codex changed in one file, in the shape the log screen draws a Claude Code change in: the
// hunks of an edit, which carries no type as a Claude Code edit carries none, or the whole content
// of a file it created or deleted. A moved file stands under the path it was moved to.
export interface FileDiff {
  filePath: string;
  type: "create" | "delete" | null;
  hunks: DiffHunk[];
  content: string | null;
}

export interface DiffHunk {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: string[];
}

// One change as the session record wrote it: the paths the stream's file_change item names, and
// what was done to each.
interface RecordedChange {
  paths: string[];
  diffs: FileDiff[];
}

const HUNK_HEAD = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/;

// The hunks of a unified diff. Lines outside a hunk — file headers, the no-newline marker — are
// no lines of the file and are left out.
export function unifiedHunks(diff: string): DiffHunk[] {
  const hunks: DiffHunk[] = [];
  let current: DiffHunk | null = null;
  const lines = diff.split("\n");

  if (lines.at(-1) === "") {
    lines.pop();
  }

  for (const line of lines) {
    const head = HUNK_HEAD.exec(line);

    if (head !== null) {
      current = {
        oldStart: Number(head[1]),
        oldLines: head[2] === undefined ? 1 : Number(head[2]),
        newStart: Number(head[3]),
        newLines: head[4] === undefined ? 1 : Number(head[4]),
        lines: [],
      };
      hunks.push(current);
    } else if (current !== null && (line.startsWith("+") || line.startsWith("-") || line.startsWith(" "))) {
      current.lines.push(line);
    } else if (current !== null && line === "") {
      // A context line that was empty loses its leading space in some writers.
      current.lines.push(" ");
    }
  }

  return hunks;
}

// The changes of one FileChange item of the session record, or null where the item is not one.
export function recordedChange(event: unknown): RecordedChange | null {
  const item = (event as { type?: string; payload?: { type?: string; item?: unknown } } | null)?.payload?.item as
    | { type?: string; changes?: unknown }
    | undefined;

  if ((event as { type?: string })?.type !== "event_msg" || item?.type !== "FileChange") {
    return null;
  }

  if (typeof item.changes !== "object" || item.changes === null) {
    return null;
  }

  const paths: string[] = [];
  const diffs: FileDiff[] = [];

  for (const [path, value] of Object.entries(item.changes as Record<string, unknown>)) {
    const change = value as { type?: string; unified_diff?: unknown; content?: unknown; move_path?: unknown };
    const filePath = typeof change.move_path === "string" && change.move_path !== "" ? change.move_path : path;

    paths.push(path);

    if (change.type === "update" && typeof change.unified_diff === "string") {
      diffs.push({ filePath, type: null, hunks: unifiedHunks(change.unified_diff), content: null });
    } else if ((change.type === "add" || change.type === "delete") && typeof change.content === "string") {
      diffs.push({ filePath, type: change.type === "add" ? "create" : "delete", hunks: [], content: change.content });
    }
  }

  return { paths, diffs };
}

// The paths a file_change item of Codex's own stream names.
export function streamPaths(item: Record<string, unknown>): string[] | null {
  if (!Array.isArray(item.changes)) {
    return null;
  }

  const paths = item.changes.map((change) => (change as { path?: unknown })?.path);

  return paths.every((path): path is string => typeof path === "string") ? paths : null;
}

function samePaths(a: readonly string[], b: readonly string[]): boolean {
  if (a.length !== b.length) {
    return false;
  }

  const set = new Set(a);

  return b.every((path) => set.has(path));
}

const chunkBytes = 1 << 20;

// A thread's session record, read forward as it grows. The stream's file_change item and the
// record's FileChange item carry different ids, so a change is matched by the set of paths it
// touched, in the order the changes happened.
export class CodexFileChanges {
  private path: string | null = null;
  private threadId: string | null = null;
  private offset = 0;
  private carry: Buffer = Buffer.alloc(0);
  private readonly recorded: RecordedChange[] = [];
  private readonly waiting: Array<{ paths: string[]; fill: (diffs: FileDiff[]) => void | Promise<unknown> }> = [];

  constructor(private readonly root: string = join(homeDir(), ".codex", "sessions")) {}

  // The thread the run is on. A resumed thread's record already holds its past turns, so reading
  // starts at its present end; a new thread's record may not exist yet and is read from its start
  // once it does.
  started(threadId: string): void {
    if (!/^[a-zA-Z0-9-]+$/.test(threadId) || this.path !== null) {
      return;
    }

    this.path = this.locate(threadId);

    if (this.path !== null) {
      this.offset = this.size(this.path);
    } else {
      this.threadId = threadId;
    }
  }

  // The diffs of a file_change the stream just completed, when the record already holds them.
  // Otherwise the change waits, and `fill` is called once the record catches up.
  take(paths: readonly string[], fill: (diffs: FileDiff[]) => void | Promise<unknown>): FileDiff[] | null {
    this.read();

    const found = this.match(paths);

    if (found !== null) {
      return found;
    }

    this.waiting.push({ paths: [...paths], fill });

    return null;
  }

  // Tries the waiting changes again against what the record has written since.
  async settle(): Promise<void> {
    if (this.waiting.length === 0) {
      return;
    }

    this.read();

    for (const held of [...this.waiting]) {
      const found = this.match(held.paths);

      if (found !== null) {
        this.waiting.splice(this.waiting.indexOf(held), 1);
        await held.fill(found);
      }
    }
  }

  get pending(): number {
    return this.waiting.length;
  }

  private match(paths: readonly string[]): FileDiff[] | null {
    const at = this.recorded.findIndex((change) => samePaths(change.paths, paths));

    if (at === -1) {
      return null;
    }

    // Changes the stream never named before this one are not going to be asked for.
    const [change] = this.recorded.splice(0, at + 1).slice(-1);

    return change?.diffs ?? null;
  }

  private locate(threadId: string): string | null {
    try {
      const paths = [...new Bun.Glob(`**/*${threadId}.jsonl`).scanSync({ cwd: this.root, absolute: true })];

      return paths.length === 1 ? paths[0]! : null;
    } catch {
      return null;
    }
  }

  private size(path: string): number {
    try {
      const fd = openSync(path, "r");

      try {
        return fstatSync(fd).size;
      } finally {
        closeSync(fd);
      }
    } catch {
      return 0;
    }
  }

  private read(): void {
    if (this.path === null && this.threadId !== null) {
      this.path = this.locate(this.threadId);
    }

    if (this.path === null) {
      return;
    }

    let fd: number;

    try {
      fd = openSync(this.path, "r");
    } catch {
      return;
    }

    try {
      const end = fstatSync(fd).size;
      const parts: Buffer[] = [this.carry];

      while (this.offset < end) {
        const length = Math.min(chunkBytes, end - this.offset);
        const chunk = Buffer.alloc(length);
        const got = readSync(fd, chunk, 0, length, this.offset);

        if (got <= 0) {
          break;
        }

        parts.push(chunk.subarray(0, got));
        this.offset += got;
      }

      // A newline byte never sits inside a multibyte character, so the bytes after the last one are
      // held back whole and decoded once their line is.
      const data = Buffer.concat(parts);
      const cut = data.lastIndexOf(10);

      this.carry = cut === -1 ? data : data.subarray(cut + 1);

      if (cut === -1) {
        return;
      }

      for (const line of data.subarray(0, cut).toString("utf8").split("\n")) {
        if (!line.includes(`"FileChange"`)) {
          continue;
        }

        let event: unknown;

        try {
          event = JSON.parse(line);
        } catch {
          continue;
        }

        const change = recordedChange(event);

        if (change !== null) {
          this.recorded.push(change);
        }
      }
    } finally {
      closeSync(fd);
    }
  }
}
