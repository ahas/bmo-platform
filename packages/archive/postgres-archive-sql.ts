// The archive port over PostgreSQL (docs/store.md), on Bun's own
// client. The stores' statements are written once for both databases; what differs is answered
// here: the placeholders, the types values travel and come back as, the one writer a transaction
// is, and the catalogue.

import type { TransactionSQL } from "bun";
import { idleSecondsOf, postgresClient, serverWentAway } from "./postgres-client";
import type { PostgresClient, PostgresReach } from "./postgres-client";
import { AsyncLocalStorage } from "node:async_hooks";
import { jsonKey, jsonPath } from "./archive-sql";
import type { ArchiveSql, Changed, SqlDialect, SqlValue } from "./archive-sql";

// A JSON member is read off the text column cast to `json`, the very form the indexes over one are
// declared with (docs/store.md), so a question filtering by it uses them.
const postgresDialect: SqlDialect = {
  jsonText: (column, key) => `(${column}::json ->> '${jsonKey(key)}')`,
  jsonNumber: (column, key) => `((${column}::json ->> '${jsonKey(key)}')::double precision)`,
  orElse: (value, fallback) => `COALESCE(${value}, ${fallback})`,
  jsonValid: (column) => `(${column} IS JSON)`,
  // The document is read once for every member picked.
  jsonPick: (column, members) =>
    `(SELECT json_build_object(${Object.entries(members).map(([name, path]) => `'${jsonKey(name)}', picked.document #> '{${jsonPath(path).join(",")}}'`).join(", ")}) FROM (SELECT ${column}::json AS document) AS picked)`,
  jsonObject: (members) => `json_build_object(${Object.entries(members).map(([name, value]) => `'${jsonKey(name)}', ${value}`).join(", ")})`,
  jsonArray: (value, orderBy) => `COALESCE(json_agg(${value} ORDER BY ${orderBy}), '[]'::json)`,
  larger: (one, other) => `GREATEST(${one}, ${other})`,
  smaller: (one, other) => `LEAST(${one}, ${other})`,
  position: (haystack, needle) => `strpos(${haystack}, ${needle})`,
  localHour: (epochSeconds) => `to_char(to_timestamp(${epochSeconds}), 'HH24')`,
  joined: (value, separator, orderBy) => `string_agg(CAST(${value} AS text), ${separator} ORDER BY ${orderBy})`,
};

type Client = PostgresClient;
// One open transaction: the connection it holds, and how many savepoints were opened inside it.
type Unit = { tx: TransactionSQL; savepoints: number };
type Rows = Record<string, unknown>[] & { count?: number | null };

// The key of the advisory lock a transaction holds for as long as it is open (docs/store.md, 「Connecting」).
// Any number every opener agrees on serves; this one spells "bmo" in its low bytes.
const WRITER_LOCK = 0x626d6f;

// The pauses between attempts at work the server went away under.
const firstPauseMs = 100;
const lastPauseMs = 2_000;

// What an error the server went away with is called in the log: the server's code where it gave
// one, Bun's otherwise.
function goneName(error: unknown): string {
  const { code, errno, message } = error as { code?: unknown; errno?: unknown; message?: unknown };

  return [errno, code, message].filter((part) => typeof part === "string" && part !== "").join(" ");
}

/**
 * A statement's placeholders, `?` and `?NNN` as the stores write them, in PostgreSQL's numbered
 * form. The only reading this needs is telling a question mark that is a placeholder from one inside
 * a quoted literal, a quoted name or a comment; no maintained library answers that short of parsing
 * the whole statement as one dialect or the other, and the stores' statements are written for both.
 */
export function numberedPlaceholders(sql: string): string {
  let out = "";
  let next = 1;
  let at = 0;

  while (at < sql.length) {
    const c = sql[at]!;

    if (c === "'" || c === '"') {
      const close = sql.indexOf(c, at + 1);
      const end = close < 0 ? sql.length : close + 1;

      out += sql.slice(at, end);
      at = end;
    } else if (c === "-" && sql[at + 1] === "-") {
      const close = sql.indexOf("\n", at);
      const end = close < 0 ? sql.length : close;

      out += sql.slice(at, end);
      at = end;
    } else if (c === "/" && sql[at + 1] === "*") {
      const close = sql.indexOf("*/", at + 2);
      const end = close < 0 ? sql.length : close + 2;

      out += sql.slice(at, end);
      at = end;
    } else if (c === "?") {
      let digits = "";

      for (let d = at + 1; d < sql.length && sql[d]! >= "0" && sql[d]! <= "9"; d += 1) digits += sql[d];

      if (digits === "") {
        out += `$${next}`;
        next += 1;
      } else {
        out += `$${digits}`;
      }
      at += 1 + digits.length;
    } else {
      out += c;
      at += 1;
    }
  }

  return out;
}

// A value as it travels: as text with no type of its own, so PostgreSQL reads it as whatever the
// column or the expression beside it is, the way SQLite takes whatever it is handed. A NUL, the one
// character PostgreSQL's text cannot hold, travels as U+FFFD, as the move carried every one before.
function travelling(value: SqlValue): string | Uint8Array | null {
  if (value === null || value instanceof Uint8Array) return value;
  if (typeof value === "boolean") return value ? "1" : "0";

  return withoutNulEscape(String(value).replaceAll("\0", "�"));
}

// A backslash run before `u0000`: an odd run ends in the escape itself, an even one is backslashes
// written out in front of plain text.
const nulEscapes = /(\\+)u0000/g;

// So does a NUL a JSON document writes as its escape: PostgreSQL refuses to read any member of a
// document holding one, which would fail every question filtering its table by a member. The
// escape is swapped where it stands, only in a text the JSON parser reads whole, so the document's
// numbers and layout stay as written, which a parse and a fresh stringify would not keep for an
// integer past 2^53.
export function withoutNulEscape(text: string): string {
  if (!text.includes("\\u0000")) return text;

  try {
    JSON.parse(text);
  } catch {
    return text;
  }

  return text.replace(nulEscapes, (run: string, slashes: string) => (slashes.length % 2 === 1 ? `${slashes}ufffd` : run));
}

// A value as the stores read it, which is SQLite's shape: an integer is a number, a flag 1 or 0,
// and a JSON value the dialect built its JSON text.
function arriving(value: unknown): unknown {
  if (typeof value === "bigint") return Number(value);
  if (typeof value === "boolean") return value ? 1 : 0;
  if (value !== null && typeof value === "object" && !(value instanceof Uint8Array)) return JSON.stringify(value);

  return value;
}

function row(raw: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(raw)) out[key] = arriving(value);

  return out;
}

// Every opener of one database in a process shares one port, so the process keeps one small pool
// however often it opens the archive, and a transaction opened through one opener is the one every
// other opener's statement joins rather than waits behind.
const shared = new Map<string, PostgresArchiveSql>();

export class PostgresArchiveSql implements ArchiveSql {
  private readonly units = new AsyncLocalStorage<Unit>();
  readonly dialect = postgresDialect;
  private client: Client | null = null;
  // Statements and transactions under way outside any transaction's own connection.
  private working = 0;
  private resting: ReturnType<typeof setTimeout> | null = null;

  /**
   * A port opening its client with `connect` at its first statement, and giving it back once
   * nothing has used it for `idleSeconds`, the next statement opening a fresh one. Work the server
   * went away under is run again on a fresh client until the server answers it, the first pause
   * between attempts lasting `firstPause`.
   */
  constructor(
    private readonly connect: () => Client,
    private readonly idleSeconds: number,
    private readonly firstPause: number = firstPauseMs,
  ) {}

  /** The port over the database a `postgres:` address names, shared with every opener of it here. */
  static open(address: string, reach: PostgresReach = {}): PostgresArchiveSql {
    const key = JSON.stringify([address, reach]);
    const known = shared.get(key);

    if (known !== undefined) return known;

    const port = new PostgresArchiveSql(() => postgresClient(address, reach), idleSecondsOf(reach));

    shared.set(key, port);

    return port;
  }

  get inTransaction(): boolean {
    return this.units.getStore() !== undefined;
  }

  all(sql: string, ...params: SqlValue[]): Promise<unknown[]> {
    return this.query(sql, params).then((rows) => rows.map(row));
  }

  async get(sql: string, ...params: SqlValue[]): Promise<unknown> {
    const rows = await this.query(sql, params);

    return rows.length === 0 ? null : row(rows[0]!);
  }

  async values(sql: string, ...params: SqlValue[]): Promise<unknown[][]> {
    const rows = (await this.on((client) => client.unsafe(numberedPlaceholders(sql), params.map(travelling)).values())) as unknown[][];

    return rows.map((one) => one.map(arriving));
  }

  async run(sql: string, ...params: SqlValue[]): Promise<Changed> {
    const rows = await this.query(sql, params);

    return { changes: Number(rows.count ?? 0) };
  }

  // A transaction takes the archive's one writer turn before its first statement, so whatever it
  // reads it may act on as SQLite's immediate transaction let it; one inside another is a savepoint,
  // undone alone where its own work fails.
  async transaction<T>(work: () => Promise<T>): Promise<T> {
    const open = this.units.getStore();

    if (open !== undefined) {
      const name = `unit_${open.savepoints += 1}`;

      await open.tx.unsafe(`SAVEPOINT ${name}`);

      try {
        const answer = await work();

        await open.tx.unsafe(`RELEASE SAVEPOINT ${name}`);

        return answer;
      } catch (error) {
        await open.tx.unsafe(`ROLLBACK TO SAVEPOINT ${name}`);
        await open.tx.unsafe(`RELEASE SAVEPOINT ${name}`);

        throw error;
      }
    }

    return await this.using((client) =>
      client.begin(async (tx) => {
        await tx.unsafe(`SELECT pg_advisory_xact_lock(${WRITER_LOCK})`);

        return await this.units.run({ tx, savepoints: 0 }, work);
      }),
    );
  }

  async wrote<T>(work: () => Promise<T>): Promise<T> {
    return this.inTransaction ? await work() : await this.transaction(work);
  }

  async hasTable(table: string): Promise<boolean> {
    return (await this.get("SELECT 1 AS found FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = ?", table)) !== null;
  }

  // A table's columns as SQLite lists them, so without the row id every table here carries.
  async columnNames(table: string): Promise<string[]> {
    const rows = (await this.all(
      "SELECT column_name AS name FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = ? AND column_name <> 'rowid' ORDER BY ordinal_position",
      table,
    )) as { name: string }[];

    return rows.map((one) => one.name);
  }

  // The shape here is the migrations', and the runtime's role holds no right to make any of it.
  async makeMissing(what: string, stands: boolean): Promise<void> {
    if (!stands) throw new Error(`${what} is not in the database, and on PostgreSQL only a migration of bmo-archive makes it`);
  }

  // The client given back at once; a later statement opens a fresh one, so one opener closing does
  // not end another's.
  async close(): Promise<void> {
    if (this.resting !== null) clearTimeout(this.resting);
    this.resting = null;

    const client = this.client;

    this.client = null;
    await client?.close();
  }

  // A statement goes on the open transaction's connection, and otherwise on the client.
  private on<T>(statement: (client: Client | TransactionSQL) => Promise<T>): Promise<T> {
    const open = this.units.getStore();

    return open === undefined ? this.using(statement) : statement(open.tx);
  }

  // Work the server went away under is run again whole on a fresh client, with a doubling pause between
  // attempts, until the server answers it however long its restart takes (docs/store.md); every other error
  // is thrown as it came.
  private async using<T>(work: (client: Client) => Promise<T>): Promise<T> {
    if (this.resting !== null) clearTimeout(this.resting);
    this.resting = null;
    this.working += 1;

    let pause = this.firstPause;

    try {
      for (let attempt = 1; ; attempt += 1) {
        let client: Client | null = null;

        try {
          client = this.client ??= this.connect();

          return await work(client);
        } catch (error) {
          if (!serverWentAway(error)) throw error;

          console.error(`archive: the database went away (${goneName(error)}); running the work again in ${pause} ms, attempt ${attempt + 1}`);
          if (client !== null) this.letGo(client);
          await Bun.sleep(pause);
          pause = Math.min(pause * 2, lastPauseMs);
        }
      }
    } finally {
      this.working -= 1;
      if (this.working === 0) this.rest();
    }
  }

  // A client the server went away under is given back, so the next attempt opens a fresh one; work
  // still standing on it ends as the server left it, and runs again on its own.
  private letGo(client: Client): void {
    if (this.client !== client) return;

    this.client = null;
    void client.close({ timeout: 0 }).catch(() => undefined);
  }

  // The client given back once it has stood unused for the idle time; the timer keeps no process
  // alive that has nothing else to do.
  private rest(): void {
    const client = this.client;

    this.resting = setTimeout(() => {
      this.resting = null;

      if (this.working === 0 && this.client === client) {
        this.client = null;
        void client?.close();
      }
    }, this.idleSeconds * 1000);
    this.resting.unref();
  }

  private async query(sql: string, params: SqlValue[]): Promise<Rows> {
    return (await this.on((client) => client.unsafe(numberedPlaceholders(sql), params.map(travelling)))) as Rows;
  }
}
