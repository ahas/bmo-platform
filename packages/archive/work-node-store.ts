import type { ArchiveSql } from "./archive-sql";

// What the store holds of the machines work goes to (docs/work-nodes.md): each node's workplace,
// where each rotation over the machines last went, and how many jobs and runs each machine was given.
// The one place the three tables are read and written.

export const WORKPLACE_STATES = ["unknown", "ready", "missing", "unreachable"] as const;

export type WorkplaceState = (typeof WORKPLACE_STATES)[number];

export interface Workplace {
  node: string;
  // The root on the node, as the node's home reads it.
  root: string;
  // The home of the node's account, as its setup last read it, against which a path written from the
  // home is read; null before the first setup.
  home: string | null;
  // Whether heavy runs go to it as well as the jobs of the checkouts placed there.
  runs: boolean;
  state: WorkplaceState;
  // What its last check found missing, one thing per line; null where nothing was.
  detail: string | null;
  checkedAt: number | null;
}

/** What one machine was given: jobs launched on it, or heavy runs sent to it. */
export type AssignmentKind = "job" | "run";

export interface Assignment {
  node: string;
  kind: AssignmentKind;
  count: number;
  lastAt: number | null;
}

/** A path on a node as its programs take it: one written from the home read against the node's home. */
export function nodePath(path: string, home: string | null): string {
  if (home === null) return path;

  return path === "~" ? home : path.startsWith("~/") ? `${home}${path.slice(1)}` : path;
}

/** The rotation a base's launches go round in, and the one every heavy run goes round in. */
export function launchRotation(base: string): string {
  return `launch:${base}`;
}

export const RUNS_ROTATION = "runs";

interface WorkplaceRow {
  node: string;
  root: string;
  home: string | null;
  runs: number;
  state: string;
  detail: string | null;
  checked_at: number | null;
}

function isWorkplaceState(value: unknown): value is WorkplaceState {
  return typeof value === "string" && (WORKPLACE_STATES as readonly string[]).includes(value);
}

function workplaceOf(row: WorkplaceRow): Workplace {
  return { node: row.node, root: row.root, home: row.home ?? null, runs: row.runs !== 0, state: isWorkplaceState(row.state) ? row.state : "unknown", detail: row.detail, checkedAt: row.checked_at };
}

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

export class WorkNodeStore {
  constructor(private readonly sql: ArchiveSql) {}

  private async held(): Promise<boolean> {
    return await this.sql.hasTable("node_workplaces");
  }

  async workplaces(): Promise<Workplace[]> {
    if (!await this.held()) return [];

    return (await this.sql.all("SELECT * FROM node_workplaces ORDER BY node") as WorkplaceRow[]).map(workplaceOf);
  }

  async workplace(node: string): Promise<Workplace | null> {
    if (!await this.held()) return null;

    const row = await this.sql.get("SELECT * FROM node_workplaces WHERE node = ?", node) as WorkplaceRow | null;

    return row === null ? null : workplaceOf(row);
  }

  /** A workplace written where none stands, or its root moved; a root moved clears the last check. */
  async placeWorkplace(node: string, root: string): Promise<Workplace> {
    return await this.sql.wrote(async () => {
      const at = nowSeconds();
      const before = await this.workplace(node);

      if (before === null) {
        await this.sql.run("INSERT INTO node_workplaces (node, root, runs, state, created_at, updated_at) VALUES (?, ?, 0, 'unknown', ?, ?)", node, root, at, at);
      } else if (before.root !== root) {
        await this.sql.run("UPDATE node_workplaces SET root = ?, state = 'unknown', detail = NULL, checked_at = NULL, updated_at = ? WHERE node = ?", root, at, node);
      }

      return await this.workplace(node) as Workplace;
    });
  }

  async setRuns(node: string, runs: boolean): Promise<boolean> {
    if (!await this.held()) return false;

    return (await this.sql.run("UPDATE node_workplaces SET runs = ?, updated_at = ? WHERE node = ?", runs ? 1 : 0, nowSeconds(), node)).changes > 0;
  }

  /** One check's outcome: the state it came to and what it found missing. */
  async recordCheck(node: string, check: { state: WorkplaceState; detail: string | null; home?: string | null; at?: number }): Promise<void> {
    const at = check.at ?? nowSeconds();

    await this.sql.run(
      "UPDATE node_workplaces SET state = ?, detail = ?, home = COALESCE(?, home), checked_at = ?, updated_at = ? WHERE node = ?",
      check.state, check.detail, check.home ?? null, at, at, node,
    );
  }

  async removeWorkplace(node: string): Promise<boolean> {
    if (!await this.held()) return false;

    return (await this.sql.run("DELETE FROM node_workplaces WHERE node = ?", node)).changes > 0;
  }

  /** The machine a rotation last went to, or null where it has gone nowhere yet. */
  async lastOf(rotation: string): Promise<string | null> {
    if (!await this.sql.hasTable("node_rotation")) return null;

    const row = await this.sql.get("SELECT last_node FROM node_rotation WHERE rotation = ?", rotation) as { last_node: string } | null;

    return row?.last_node ?? null;
  }

  async rotations(): Promise<{ rotation: string; lastNode: string }[]> {
    if (!await this.sql.hasTable("node_rotation")) return [];

    return (await this.sql.all("SELECT rotation, last_node FROM node_rotation ORDER BY rotation") as { rotation: string; last_node: string }[])
      .map((row) => ({ rotation: row.rotation, lastNode: row.last_node }));
  }

  /** A rotation gone to a machine, and that machine's count of what it was given moved on by one. */
  async assign(rotation: string, node: string, kind: AssignmentKind, at: number = nowSeconds()): Promise<void> {
    await this.sql.transaction(async () => {
      await this.sql.run(
        "INSERT INTO node_rotation (rotation, last_node, updated_at) VALUES (?, ?, ?) ON CONFLICT (rotation) DO UPDATE SET last_node = excluded.last_node, updated_at = excluded.updated_at",
        rotation, node, at,
      );
      await this.sql.run(
        "INSERT INTO node_assignments (node, kind, count, last_at) VALUES (?, ?, 1, ?) ON CONFLICT (node, kind) DO UPDATE SET count = node_assignments.count + 1, last_at = excluded.last_at",
        node, kind, at,
      );
    });
  }

  async assignments(): Promise<Assignment[]> {
    if (!await this.sql.hasTable("node_assignments")) return [];

    return (await this.sql.all("SELECT node, kind, count, last_at FROM node_assignments ORDER BY node, kind") as { node: string; kind: AssignmentKind; count: number; last_at: number | null }[])
      .map((row) => ({ node: row.node, kind: row.kind, count: row.count, lastAt: row.last_at }));
  }
}
