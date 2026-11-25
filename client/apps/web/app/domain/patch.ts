import { countText } from "./numbers";

import type { AgentLogPatch } from "#shared/types";
import type { Translate } from "#shared/wording";

// What a file tool changed, laid out the way Claude Code draws it in its own terminal: a head
// naming what was done to which file, how many lines went in and out, and the changed lines with
// their numbers and a mark.

export type DiffMark = "+" | "-" | " ";

export type DiffRow =
  | { kind: "line"; mark: DiffMark; old: number | null; new: number | null; text: string }
  // Where one hunk ends and the next begins further down the file.
  | { kind: "gap" };

export interface DiffView {
  head: string;
  // Whether a shell command made the change rather than a file tool: a command keeps its own line
  // and the change stands under it.
  byCommand: boolean;
  summary: string;
  added: number;
  removed: number;
  rows: DiffRow[];
}

// A diff this long is folded until it is asked for; a shorter one stands open, as the terminal
// shows it.
export const OPEN_DIFF_ROWS = 40;

// The word the terminal heads a file tool's change with. An edit updates a file; a write names
// itself whether it made the file or replaced it; a patch that removed a file deletes it.
function verb(patch: AgentLogPatch): string {
  return patch.type === null ? "Update" : patch.type === "delete" ? "Delete" : "Write";
}

// How the terminal heads one change: a file tool by the tool and the file, a file a shell command
// changed by the file and how many lines went in and out.
function headOf(patch: AgentLogPatch, path: string, added: number, removed: number): string {
  return patch.by === "command" ? `Updated ${path} (+${added} -${removed})` : `${verb(patch)}(${path})`;
}

// A path under the checkout the run worked in reads from that checkout, as the run itself wrote
// it; any other path stands whole.
export function shownPath(path: string, root: string | null): string {
  if (root === null || root === "") {
    return path;
  }

  const base = root.endsWith("/") ? root : `${root}/`;

  return path.startsWith(base) ? path.slice(base.length) : path;
}

// How many lines went in and out, naming only the side that moved.
export function diffSummary(t: Translate, added: number, removed: number): string {
  if (added > 0 && removed > 0) {
    return t("agents.label.diff_added_removed", { added: countText(added), removed: countText(removed) });
  }

  if (added > 0) {
    return t("agents.label.diff_added", { added: countText(added) });
  }

  return removed > 0 ? t("agents.label.diff_removed", { removed: countText(removed) }) : t("agents.label.diff_none");
}

// The rows of a created file — every line of it, added, numbered from one — or of a deleted one,
// every line removed.
function wholeRows(content: string, removed: boolean): DiffRow[] {
  const lines = content.split("\n");

  if (lines.at(-1) === "") {
    lines.pop();
  }

  return lines.map((text, index) =>
    removed
      ? { kind: "line", mark: "-", old: index + 1, new: null, text }
      : { kind: "line", mark: "+", old: null, new: index + 1, text }
  );
}

// The rows of the recorded hunks. A removed line carries the number it had, an added line the
// number it has now, and a line that stayed carries the new one. The marker git writes for a file
// with no final newline is no line of the file and is left out.
function hunkRows(patch: AgentLogPatch): DiffRow[] {
  const rows: DiffRow[] = [];

  patch.hunks.forEach((hunk, index) => {
    if (index > 0) {
      rows.push({ kind: "gap" });
    }

    let old = hunk.oldStart;
    let now = hunk.newStart;

    for (const line of hunk.lines) {
      const mark = line[0];
      const text = line.slice(1);

      if (mark === "-") {
        rows.push({ kind: "line", mark: "-", old: old++, new: null, text });
      } else if (mark === "+") {
        rows.push({ kind: "line", mark: "+", old: null, new: now++, text });
      } else if (mark === " ") {
        rows.push({ kind: "line", mark: " ", old: old++, new: now++, text });
      }
    }
  });

  return rows;
}

// The whole picture of one change, or null where there is nothing to draw — a change too large to
// have been sent, or a write that carried neither hunks nor content.
export function diffView(t: Translate, patch: AgentLogPatch | null, root: string | null): DiffView | null {
  if (patch === null || patch.tooLarge) {
    return null;
  }

  const rows =
    patch.hunks.length > 0
      ? hunkRows(patch)
      : patch.content === null
        ? null
        : wholeRows(patch.content, patch.type === "delete");

  if (rows === null) {
    return null;
  }

  const added = rows.filter((row) => row.kind === "line" && row.mark === "+").length;
  const removed = rows.filter((row) => row.kind === "line" && row.mark === "-").length;

  return {
    head: headOf(patch, shownPath(patch.filePath, root), added, removed),
    byCommand: patch.by === "command",
    summary: diffSummary(t, added, removed),
    added,
    removed,
    rows
  };
}

// What one result changed, all its files together: each file's picture, and the line the result
// folds under, which counts every file's lines and names the files the provider only counted.
export interface ChangesView {
  files: DiffView[];
  summary: string;
  rows: number;
  more: string | null;
}

export function changesView(t: Translate, patches: readonly AgentLogPatch[], more: number, root: string | null): ChangesView | null {
  const files = patches.map((patch) => diffView(t, patch, root)).filter((view) => view !== null);

  if (files.length === 0) {
    return null;
  }

  const added = files.reduce((sum, view) => sum + view.added, 0);
  const removed = files.reduce((sum, view) => sum + view.removed, 0);

  return {
    files,
    summary: diffSummary(t, added, removed),
    rows: files.reduce((sum, view) => sum + view.rows.length, 0),
    more: more > 0 ? t("agents.label.diff_more_files", { n: countText(more) }) : null
  };
}
