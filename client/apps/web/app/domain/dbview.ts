import { DB_VIEW } from "#shared/config";
import { countText } from "./numbers";

import type { DbCell, DbTable, DbValue } from "#shared/types";
import type { Translate } from "#shared/wording";

// What the DB screen reads with. The database is read one page at a time, so most of this is
// about saying honestly what a page is a page of: which one of how many, how much of a long
// value is actually on the screen, and whether an answer stopped where the answer stopped or
// where the cap did.

// The two ways of reading the database: walking a table, or asking a question of it.
export type DbMode = "table" | "sql";

export function isDbMode(value: unknown): value is DbMode {
  return value === "table" || value === "sql";
}

// What a column is sorted by. Nothing is the table's own order, which is the newest row first.
export interface DbSort {
  column: string;
  descending: boolean;
}

// Pressing a column once reads it upwards, again reads it downwards, and a third time gives the
// table back its own order. Every state is one press from the next, so nothing is reached by
// pressing something else first.
export function nextSort(current: DbSort | null, column: string): DbSort | null {
  if (current === null || current.column !== column) {
    return { column, descending: false };
  }

  return current.descending ? null : { column, descending: true };
}

export function sortIcon(current: DbSort | null, column: string): string {
  if (current === null || current.column !== column) {
    return "i-material-symbols-unfold-more";
  }

  return current.descending ? "i-material-symbols-keyboard-arrow-down" : "i-material-symbols-keyboard-arrow-up";
}

// How many pages a count of rows makes. A table with nothing in it is one empty page rather
// than none: there is still a table there, and it still says so.
export function pageCount(total: number, pageSize: number = DB_VIEW.pageSize): number {
  return Math.max(1, Math.ceil(total / Math.max(1, pageSize)));
}

export function clampPage(page: number, total: number, pageSize: number = DB_VIEW.pageSize): number {
  return Math.min(Math.max(1, Math.floor(page)), pageCount(total, pageSize));
}

// Where the reader is: which page of how many, and how many rows there are altogether under
// whatever search is on.
export function pageLabel(t: Translate, page: number, total: number, pageSize: number = DB_VIEW.pageSize): string {
  return t("db.label.page", { page: countText(page), pages: countText(pageCount(total, pageSize)), total: countText(total) });
}

// SQL NULL is not an empty string and is not the word NULL either, so the screen says it in its
// own hand rather than letting it read as a value.
export function isNull(value: DbValue): boolean {
  return value === null;
}

export function cellText(value: DbValue): string {
  return value === null ? "NULL" : value.text;
}

// Whether the screen is holding the head of a value rather than the value. What is said about
// it says how much there is, since a cut with no length reads as the whole of a short value.
export function isCut(value: DbValue): boolean {
  return value !== null && value.cut;
}

export function cutNote(t: Translate, value: DbValue): string {
  return isCut(value) ? t("db.label.cut_length", { n: countText((value as DbCell).length) }) : "";
}

// One line under a value in the one-row view: how long it is, and nothing at all when it is
// short enough that the length is no news.
export function lengthNote(t: Translate, value: DbValue): string {
  if (value === null) {
    return "";
  }

  return value.length > DB_VIEW.cellCap ? t("db.label.length", { n: countText(value.length) }) : "";
}

export interface DbChoice {
  value: string;
  label: string;
}

// The tables to pick from, each with how many rows it holds, since which table is worth opening
// is mostly a question of how much is in it.
export function tableChoices(tables: DbTable[]): DbChoice[] {
  return tables.map((table) => ({ value: table.name, label: `${table.name} (${countText(table.rows)})` }));
}

export function findTable(tables: DbTable[], name: string | null): DbTable | null {
  return tables.find((table) => table.name === name) ?? null;
}

// Which table a screen opens on: the one left open last time if it is still there, and
// otherwise the first of them. A table that has gone is not held on to.
export function openingTable(tables: DbTable[], remembered: string | null): string | null {
  if (findTable(tables, remembered) !== null) {
    return remembered;
  }

  return tables[0]?.name ?? null;
}

// What a typed query answered with, said in numbers. An answer that stopped at the cap says so
// rather than reading as the whole of it.
export function resultNote(t: Translate, rows: number, cut: boolean, ms: number): string {
  return cut
    ? t("db.label.result_cut", { rows: countText(rows), ms: countText(ms) })
    : t("db.label.result", { rows: countText(rows), ms: countText(ms) });
}

// Where a page of a table stands, said the same way.
export function pageNote(t: Translate, rows: number, ms: number): string {
  return t("db.label.result", { rows: countText(rows), ms: countText(ms) });
}

// A value as one line of a list. The database is full of prose with newlines in it, and a cell
// that keeps them makes one row as tall as a paragraph; what the list is for is seeing many
// rows at once, and the whole of a value is one press away.
export function oneLine(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

// What one cell reads as in the list: one line of it, with a mark where there is more.
export function cellPreview(value: DbValue): string {
  if (value === null) {
    return "";
  }

  const line = oneLine(value.text);

  return value.cut ? `${line}…` : line;
}
