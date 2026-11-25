// Where agency's store is (docs/store.md, 「Connecting」): a PostgreSQL address, the one
// `BMO_ARCHIVE_URL` names, else the one the data root's `env/archive-url` holds. A process with
// neither names no store.

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { dataRoot } from "@agency/support/roots";

import type { ArchiveSql } from "./archive-sql";
import { PostgresArchiveSql } from "./postgres-archive-sql";

/** Whether a place names a PostgreSQL database. */
export function isPostgresPlace(place: string): boolean {
  return /^postgres(ql)?:\/\//.test(place);
}

/** The socket directory of this machine's own PostgreSQL server. */
export const LOCAL_POSTGRES_SOCKET = "/var/run/postgresql";

/** The address of a database on this machine's own server, reached through its socket. */
export function localDatabase(name: string): string {
  return `postgres:///${encodeURIComponent(name)}?host=${LOCAL_POSTGRES_SOCKET}`;
}

/** The database of a name on the server an address reaches. */
export function databaseOn(server: string, name: string): string {
  const url = new URL(server);

  url.pathname = `/${encodeURIComponent(name)}`;

  return url.toString().replace(/%2F/gi, "/");
}

/** The same database reached as another role, named in libpq's `user` parameter. */
export function asUser(address: string, role: string): string {
  const url = new URL(address);

  url.username = "";
  url.searchParams.set("user", role);

  return url.toString().replace(/%2F/gi, "/");
}

/** The file in a data root holding the store's address. */
export function archiveSettingPath(root: string = dataRoot()): string {
  return join(root, "env", "archive-url");
}

/** The store's address for this process, or null where it names none. */
export function storeAddress(env: NodeJS.ProcessEnv = process.env, root: string = dataRoot()): string | null {
  const named = env.BMO_ARCHIVE_URL?.trim();

  if (named !== undefined && named !== "") return named;

  const setting = archiveSettingPath(root);
  const held = existsSync(setting) ? readFileSync(setting, "utf8").trim() : "";

  return held === "" ? null : held;
}

/** The store's address for this process, refused where it names none. */
export function requireStoreAddress(env: NodeJS.ProcessEnv = process.env, root: string = dataRoot()): string {
  const address = storeAddress(env, root);

  if (address === null) {
    throw new Error(`no store is named for ${root}: set BMO_ARCHIVE_URL or write env/archive-url`);
  }

  return address;
}

/** A port of its own over the store at an address, for a reader no archive is handed to. */
export interface OpenedArchiveSql {
  sql: ArchiveSql;
  close(): Promise<void>;
}

export function openArchiveSql(address: string, options: { readonly?: boolean } = {}): OpenedArchiveSql {
  const sql = PostgresArchiveSql.open(address, { readonly: options.readonly === true });

  return { sql, close: () => sql.close() };
}
