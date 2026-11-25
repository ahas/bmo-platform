// The store's `prefix_moves` table, read and written here and nowhere else on the TypeScript side
// (docs/sessions.md), and the usage windows the Rust side keeps, read here for the one account judgement. Usage itself is the usage reader's: agency publishes `usage-spent` and keeps no
// usage rows (docs/api.md, 「Events」).

import type { ArchiveSql } from "./archive-sql";

// Why a cached prefix moved. A document a session reads was written and the session was told to
// read it again; the code the bot runs went live; or the line a session is opened on — the tools
// it is shown, the model it runs as, the documents its own prompt is built out of — moved with a
// deployment. Nothing else moves a prefix from this side, and what the watch sees that none of
// these explains is the CLI's own doing rather than the bot's.
export type PrefixMoveWhy = "reload" | "deploy" | "launch";

export interface NewPrefixMove {
  agent: string;
  at: number;
  why: PrefixMoveWhy;
  detail?: string | null;
}

export interface PrefixMove extends NewPrefixMove {
  id: number;
  detail: string | null;
}

/** One window of a login's limit as the store keeps its last reading (docs/usage-windows.md): the fraction used,
 * when it resets and when it was seen, both in epoch seconds. */
export interface KeptUsageWindow {
  name: string;
  utilization: number;
  resetsAt: number | null;
  seenAt: number;
}

export class UsageStore {
  constructor(private readonly sql: ArchiveSql) {}

  // The windows kept of one login under one provider, as agency's Rust side keeps them in `usage_windows`.
  async usageWindows(mode: string, account: string): Promise<KeptUsageWindow[]> {
    const rows = await this.sql.all(
      "SELECT name, utilization, resets_at, seen_at FROM usage_windows WHERE mode = ? AND account = ? ORDER BY name",
      mode,
      account,
    ) as { name: string; utilization: number | string; resets_at: number | string | null; seen_at: number | string }[];

    return rows.map((row) => ({
      name: row.name,
      utilization: Number(row.utilization),
      resetsAt: row.resets_at === null ? null : Number(row.resets_at),
      seenAt: Number(row.seen_at),
    }));
  }

  // Puts one prefix move on the record. The agent is the bot's own name for whose prefix moved, or
  // the mark that stands for all of them — a deployment moves what every session stands on, and
  // naming them one by one would be a list that goes stale the moment a worker is added.
  async notePrefixMove(move: NewPrefixMove): Promise<void> {
    await this.sql.run("INSERT INTO prefix_moves (agent, at, why, detail) VALUES (?, ?, ?, ?)", move.agent, move.at, move.why, move.detail ?? null);
  }

  // What moved between two moments, oldest first. It is asked for a span rather than row by row
  // because a screen full of requests would otherwise be a query apiece.
  async prefixMovesBetween(from: number, to: number): Promise<PrefixMove[]> {
    return await this.sql.all("SELECT id, agent, at, why, detail FROM prefix_moves WHERE at >= ? AND at <= ? ORDER BY at ASC", from, to) as PrefixMove[];
  }
}
