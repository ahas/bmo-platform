// The archive port: what every store is handed to reach its tables, whichever database stands
// under it (docs/store.md). Every statement answers a promise and
// every transaction is an async unit of work, so a store reads the same over each adapter.

/** A value a statement may be given for one of its parameters. */
export type SqlValue = string | number | bigint | boolean | null | Uint8Array;

/** What a write came to: the rows it changed. A key an insert was given is asked for with `RETURNING`. */
export interface Changed {
  changes: number;
}

/**
 * What the two databases write differently, as SQL text each adapter answers for its own. A store
 * builds such a fragment into its statement; everything else in a statement is written once and
 * read by both.
 */
export interface SqlDialect {
  /** A member of the JSON text a column holds, by its key; null where the member is absent. */
  jsonText(column: string, key: string): string;
  /** The same member read as a number, for comparing with one. */
  jsonNumber(column: string, key: string): string;
  /** A value, or the fallback where it is null, in the form an index over it is declared with. */
  orElse(value: string, fallback: string): string;
  /** Whether a text is JSON at all. */
  jsonValid(column: string): string;
  /**
   * Several members of the JSON a column holds, each by its path (`$.a.b`), picked into one JSON
   * object of the names given: what a reader needs of a document too large to send whole.
   */
  jsonPick(column: string, members: Readonly<Record<string, string>>): string;
  /** A JSON object built of names and the values of the expressions beside them. */
  jsonObject(members: Readonly<Record<string, string>>): string;
  /** A group's values as one JSON array, in the order given; `[]` where the group is empty. */
  jsonArray(value: string, orderBy: string): string;
  /** The larger of two values, and the smaller. */
  larger(one: string, other: string): string;
  smaller(one: string, other: string): string;
  /** Where a text first stands in another, counted from 1, or 0 where it does not. */
  position(haystack: string, needle: string): string;
  /** The hour of an epoch-seconds value on this machine's clock, as two digits. */
  localHour(epochSeconds: string): string;
  /** A group's values joined into one text with a separator between them, in the order given. */
  joined(value: string, separator: string, orderBy: string): string;
}

/** A JSON member's key as both dialects take it into a statement: a plain name and nothing else. */
export function jsonKey(key: string): string {
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) throw new Error(`a JSON member is read by a plain name, not "${key}"`);

  return key;
}

/** A JSON path's names, from `$.a.b`: plain names joined by dots after the root and nothing else. */
export function jsonPath(path: string): string[] {
  if (!/^\$(\.[A-Za-z_][A-Za-z0-9_]*)+$/.test(path)) throw new Error(`a JSON path is plain names under the root, not "${path}"`);

  return path.slice(2).split(".");
}

export interface ArchiveSql {
  /** Every row a statement answers, in the order it answers them. */
  all(sql: string, ...params: SqlValue[]): Promise<unknown[]>;
  /** The first row a statement answers, or null where it answers none. */
  get(sql: string, ...params: SqlValue[]): Promise<unknown>;
  /** Every row as a list of its column values, in the statement's column order. */
  values(sql: string, ...params: SqlValue[]): Promise<unknown[][]>;
  /** A statement that answers no rows. */
  run(sql: string, ...params: SqlValue[]): Promise<Changed>;
  /**
   * Several statements as one commit. Every statement any store of this archive runs inside
   * `work` is part of it, and a statement run beside it waits for it to end; the whole is run
   * again where the database refused it for a lock another writer held.
   */
  transaction<T>(work: () => Promise<T>): Promise<T>;
  /**
   * A single write made a transaction of its own, for the retry a transaction has; inside one
   * already it is simply run.
   */
  wrote<T>(work: () => Promise<T>): Promise<T>;
  /**
   * Whether a table stands, and the columns it has, for a reader of an archive copied before the
   * table or the column was made; each database keeps its own catalogue.
   */
  hasTable(table: string): Promise<boolean>;
  columnNames(table: string): Promise<string[]>;
  /**
   * A table or a column the code makes for itself outside the settling, named by `what` and made by
   * `statement`, where `stands` says it is missing. A database whose shape the code settles runs the
   * statement; one whose shape is its migrations' never does, since the runtime's role there makes
   * nothing, and refuses a missing one by name (docs/store.md).
   */
  makeMissing(what: string, stands: boolean, statement: string): Promise<void>;
  /** Whether the work asking runs inside a transaction of this archive. */
  readonly inTransaction: boolean;
  /** What this database writes differently from the other. */
  readonly dialect: SqlDialect;
}
