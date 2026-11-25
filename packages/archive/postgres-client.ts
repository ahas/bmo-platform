// A client of Bun's own for the PostgreSQL database an address names (docs/store.md). An address is a
// `postgres:` URL in libpq's form; one whose `host` parameter is a directory names the server's socket in
// it, which Bun's client is handed as the socket's own path. The role logged in as is the URL's user or its
// `user` parameter, as libpq reads either, and the user running this process where it names none.

import { SQL } from "bun";
import { existsSync } from "node:fs";
import { userInfo } from "node:os";

export type PostgresClient = InstanceType<typeof SQL>;

export interface PostgresReach {
  /** Another database on the same server, in place of the one the address names. */
  database?: string;
  /** The schema names are read in first, before `public`. */
  schema?: string;
  /** How many connections the client may hold at once. */
  connections?: number;
  /** How long the archive port's client stands unused before it is given back, in seconds. */
  idleSeconds?: number;
  /** Every statement refused a write by the server itself, as a SQLite file opened read-only is. */
  readonly?: boolean;
}

// Every process that opens the archive holds a pool of its own, and the server shares its hundred
// connections between every database on it, so a pool is kept small, and the archive port gives
// back a client it has not used for a little while.
const defaultConnections = 2;
const defaultIdleSeconds = 10;

export function idleSecondsOf(reach: PostgresReach): number {
  return reach.idleSeconds ?? defaultIdleSeconds;
}

// What Bun's client says when the connection closed under a statement, never opened, or answered
// with something it could not read; and the server's own codes for going down, not being up yet, or
// having no connection left to give.
const goneCodes = new Set(["ERR_POSTGRES_CONNECTION_CLOSED", "ERR_POSTGRES_CONNECTION_TIMEOUT", "ERR_POSTGRES_EXPECTED_REQUEST"]);
const goneStates = new Set(["57P01", "57P02", "57P03", "53300"]);
const goneSystemCodes = new Set(["ECONNREFUSED", "ECONNRESET", "ENOENT", "EPIPE"]);

/**
 * Whether an error is the server going away rather than the statement's own fault
 * (docs/store.md). A socket Bun's client cannot open arrives as a
 * plain error carrying no code, told only by the name of the failure its message opens with.
 */
export function serverWentAway(error: unknown): boolean {
  if (!(error instanceof Error)) return false;

  const { code, errno } = error as { code?: unknown; errno?: unknown };

  if (typeof code === "string" && (goneCodes.has(code) || goneSystemCodes.has(code))) return true;
  if (typeof errno === "string" && (goneStates.has(errno) || errno.startsWith("08"))) return true;

  return error.message.startsWith("FailedToOpenSocket");
}

export function postgresClient(address: string, reach: PostgresReach = {}): PostgresClient {
  return new SQL(postgresOptions(address, reach));
}

/** What Bun's client is handed for an address: plain values, which a process of its own can be given too. */
export function postgresOptions(address: string, reach: PostgresReach = {}): Bun.SQL.Options {
  const url = new URL(address);
  const socketDir = url.searchParams.get("host");
  const role = decodeURIComponent(url.username) || url.searchParams.get("user") || "";
  const database = reach.database ?? decodeURIComponent(url.pathname.replace(/^\//, ""));
  const options = {
    // Integers arrive whole, and a value travels as text PostgreSQL reads by where it stands.
    bigint: true,
    prepare: false,
    max: reach.connections ?? defaultConnections,
    // Bun's own idle timer counts a statement still running as idle and fails it, a wait on the
    // writer's lock included, so the client keeps no idle timer of its own.
    idleTimeout: 0,
    // A time is read on this machine's clock, as SQLite's `localtime` reads it.
    connection: {
      TimeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      ...(reach.schema === undefined ? {} : { search_path: `"${reach.schema}", public` }),
      ...(reach.readonly === true ? { default_transaction_read_only: "on" } : {}),
    },
  } as const;

  if (socketDir === null || !socketDir.startsWith("/")) {
    url.pathname = `/${encodeURIComponent(database)}`;
    url.username = encodeURIComponent(role);
    url.searchParams.delete("user");

    return { url: url.toString(), ...options };
  }

  const path = `${socketDir}/.s.PGSQL.${url.port === "" ? "5432" : url.port}`;

  // Bun's client drops a socket path it finds no file at and reaches TCP on localhost:5432 instead,
  // which is another server or none, so a server whose socket is gone — stopped, or between a stop
  // and a start — is refused here, as the server gone away it is.
  if (!existsSync(path)) {
    throw Object.assign(new Error(`the server's socket ${path} is absent`), { code: "ENOENT" });
  }

  return {
    ...options,
    path,
    username: role || process.env.USER || process.env.LOGNAME || userInfo().username,
    password: decodeURIComponent(url.password) || undefined,
    database,
  };
}
