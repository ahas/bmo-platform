// A store of a test's own (docs/store.md): a database on the suite's PostgreSQL server, copied from a
// template this process makes once out of the store crate's 1.0.0 baseline and the migrations after it,
// and the account seed a deployment's settling writes, and dropped when the file or the test
// that asked for it is done, and the run fails if one cannot be. Before a process makes its first database,
// the templates and copies a process that is gone left behind are dropped.
//
// A store is asked for at a file's top level, inside a test or in a `beforeEach`, never in a
// `beforeAll`: Bun runs an `afterAll` registered inside a `beforeAll` as soon as that hook ends, so the
// store would be dropped before the first test reads it.

import { afterAll } from "bun:test";
import { randomUUID } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { seedClaudeAccount } from "@agency/archive/account-seed";
import { Archive } from "@agency/archive/archive";
import { databaseOn } from "@agency/archive/archive-place";
import { postgresClient } from "@agency/archive/postgres-client";

import type { PostgresClient } from "@agency/archive/postgres-client";

// What a template's and a copy's names begin with, followed by the process that made them, the run, and a
// copy's count.
const TEMPLATE = "agency_ts_tmpl_";
const COPY = "agency_ts_copy_";

// The store's tables, as the store crate's migrations make them, every file each runs, in the order
// `crates/store/src/migration.rs` runs them: what a store this process makes has to be the store the
// server's own migration leaves, or a column the TypeScript side reads would be missing only in its tests.
const MIGRATION_DIR = join(import.meta.dir, "..", "..", "..", "crates", "store", "src", "migration");
const MIGRATIONS: readonly string[] = ["baseline.sql"];

let swept: Promise<void> | null = null;
let made: Promise<string> | null = null;
let copies = 0;

// The run this file's stores belong to. A test file run beside another in one process has the same
// process id, so the pid alone would name one template twice and the second creation would fail.
const run = randomUUID().slice(0, 8);

/** The suite's PostgreSQL server, as the run names it, and the machine's own socket where nothing does. */
export function testServer(): string {
  return process.env.BMO_TEST_POSTGRES ?? "postgres:///bmo_test?host=/var/run/postgresql";
}

/** The test server's address with another database in place of its own. */
export function addressOf(database: string): string {
  return databaseOn(testServer(), database);
}

// One step on the server's own database, its connection held for as long as the step takes.
async function onServer<T>(work: (sql: PostgresClient) => Promise<T>): Promise<T> {
  const sql = postgresClient(addressOf("postgres"), { connections: 1 });

  try {
    return await work(sql);
  } finally {
    await sql.close();
  }
}

// The process id a template's or a copy's name carries, if the name is one of this harness's.
function processOf(name: string): number | null {
  const prefix = [TEMPLATE, COPY].find((one) => name.startsWith(one));
  const pid = prefix === undefined ? "" : (name.slice(prefix.length).split("_")[0] ?? "");

  return /^\d+$/.test(pid) ? Number(pid) : null;
}

// Whether a process runs on this machine, this one among them: a test file run beside another in this
// process may sweep after the other has made its stores. Where the process table cannot be read every
// process counts as alive, so nothing is dropped on a guess.
function lives(pid: number): boolean {
  return pid === process.pid || !existsSync("/proc/self") || existsSync(join("/proc", String(pid)));
}

// The databases a dead process of this harness left behind dropped, once per process and before it makes
// any of its own. A database is left alone while its process runs on this machine or any session is on it,
// so a run of another checkout against the same server keeps its own, and a name of another repository's
// harness is never read. One the sweep cannot drop is named on the error stream and left for the next run.
function sweep(): Promise<void> {
  swept ??= onServer(async (sql) => {
    const unattended = await sql.unsafe("SELECT datname::text AS name FROM pg_database d WHERE NOT EXISTS (SELECT 1 FROM pg_stat_activity a WHERE a.datname = d.datname)") as { name: string }[];

    for (const { name } of unattended) {
      const pid = processOf(name);

      if (pid === null || lives(pid)) continue;

      try {
        await dropped(sql, name);
      } catch (error) {
        console.error(`the scratch sweep leaves ${name}: ${String(error)}`);
      }
    }
  }).catch((error: unknown) => console.error(`the scratch sweep reaches no test server: ${String(error)}`));

  return swept;
}

// The template this process's copies are made from, migrated the first time a test asks.
function template(): Promise<string> {
  made ??= (async () => {
    const name = `${TEMPLATE}${process.pid}_${run}`;

    await onServer((sql) => sql.unsafe(`CREATE DATABASE "${name}" TEMPLATE template0 ENCODING 'UTF8' LC_COLLATE 'C' LC_CTYPE 'C'`));

    const db = postgresClient(addressOf(name), { connections: 1 });

    try {
      for (const migration of MIGRATIONS) {
        await db.unsafe(readFileSync(join(MIGRATION_DIR, migration), "utf8")).simple();
      }
    } finally {
      await db.close();
    }

    // The account a deployment's settling gives a store holding none: the login the machine already
    // has, which is the test home's (docs/store.md, 「Its seeds」).
    const seeded = new Archive(addressOf(name));
    const account = seedClaudeAccount();

    try {
      await seeded.claudeAccounts().addClaudeAccount({ id: account.id, name: account.name, enabled: true, credentialKind: account.credential.kind, credentialPath: account.credential.path });
    } finally {
      await seeded.close();
    }

    return name;
  })();

  return made;
}

// How long a database's drop waits for its holders to leave, and how often it tries again.
const dropWithinMs = 15_000;
const dropEveryMs = 250;

// The server's SQLSTATE for a database other sessions still stand on.
const IN_USE = "55006";

function inUse(error: unknown): boolean {
  return (error as { errno?: unknown } | null)?.errno === IN_USE;
}

// A test database dropped. It first takes no new connection, so nothing the test left running opens a session
// again as its sessions are ended. The test role ends only its own sessions on it. A forced drop would try to end
// every holder, and the server refuses the whole drop, before ending any, where one holder is a worker of
// its own — an autovacuum worker visiting the database, say — since a role without the right to signal
// every backend may not end it. A plain drop asks the server's own workers there to stop and waits a few
// seconds for every holder to leave; where one stays past that, or a session of the role came back in
// between, it fails as in use, and the drop tries again within its bound and then says who held it.
async function dropped(sql: PostgresClient, name: string): Promise<void> {
  const until = Date.now() + dropWithinMs;

  // A database already gone refuses the change, and the drop below reads it as dropped.
  await sql.unsafe(`ALTER DATABASE "${name}" WITH ALLOW_CONNECTIONS false`).catch(() => undefined);

  for (;;) {
    try {
      await sql.unsafe("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1 AND usename = current_user", [name]);
      await sql.unsafe(`DROP DATABASE IF EXISTS "${name}"`);
      return;
    } catch (error) {
      if (!inUse(error) || Date.now() >= until) {
        const held = await sql.unsafe("SELECT usename::text AS role, backend_type AS kind FROM pg_stat_activity WHERE datname = $1", [name]) as { role: string | null; kind: string }[];

        throw new Error(`${name} could not be dropped while ${JSON.stringify(held)} held it: ${String(error)}`);
      }

      await Bun.sleep(dropEveryMs);
    }
  }
}

// A database of the test's own made from `from`, dropped when the file that asked is done; its address.
async function copyOf(from: string): Promise<string> {
  await sweep();
  copies += 1;

  const name = `${COPY}${process.pid}_${run}_${copies}`;

  await onServer((sql) => sql.unsafe(`CREATE DATABASE "${name}" TEMPLATE "${from}" ENCODING 'UTF8' LC_COLLATE 'C' LC_CTYPE 'C'`));
  afterAll(() => onServer((sql) => dropped(sql, name)));

  return addressOf(name);
}

/** A new copy of the migrated, empty store; its address. It is dropped when the file that asked is done. */
export async function scratchDatabase(): Promise<string> {
  await sweep();

  return await copyOf(await template());
}

/**
 * A database with no table in it, for agency's own server to migrate as it stands on it, the way it
 * stands on a deployment's new store; its address. It is dropped when the file that asked is done.
 */
export async function emptyDatabase(): Promise<string> {
  return await copyOf("template0");
}

/** A migrated, empty store of the test's own, opened; closed and dropped when the file is done. */
export async function scratchArchive(): Promise<Archive> {
  const archive = new Archive(await scratchDatabase());

  afterAll(() => archive.close());

  return archive;
}
