// The archive's `agent_executions` and `execution_attempts` tables, read and written here and
// nowhere else in the archive (docs/store.md).

import type { ArchiveSql } from "./archive-sql";

export interface ResidentTurnRow {
  place: string;
  record: string;
  from: number;
  pid: number | null;
  birth: string | null;
}

export interface ExecutionRecord {
  id: string;
  // The agent the run is for: whose program a turn goes into, whose host a run is left in, or the agent a
  // host carries a run for. A supervisor's removal takes every record naming it.
  agent?: string | null;
  parent: string | null;
  root?: string;
  // The role the run holds, where it is a role's.
  role?: string | null;
  model?: string | null;
  effort?: string | null;
  sessionId?: string | null;
  nativeId?: string;
  nativeState?: "requested" | "started" | "completed" | "failed" | "unresolved";
  nativeEvidence?: string;
  reconciliationEvidence?: string;
  sharedPane?: boolean;
  // Whether the run is the pane's own process rather than a child under a supervisor envelope.
  // The row says which it is because the two are told apart by different evidence, and a guess
  // read off what happens to be on disk changes its answer the moment the other path writes a
  // file of its own.
  panePath?: boolean;
  // Set on a turn of a resident program, which outlives the turn: the agent's place, the program's
  // own record and the line it stood at when the turn went in, and the program's process at that
  // moment. The row is closed on that record rather than on its pane going away, since the pane
  // stays.
  resident?: ResidentTurnRow;
  attempt?: number;
  provider: string;
  line: string;
  paneId: string | null;
  state: "launching" | "running" | "exited" | "cancelled" | "timeout" | "launch-error" | "unknown";
  startedAt: number;
  endedAt: number | null;
  exitCode: number | null;
}

export class ExecutionStore {
  constructor(private readonly sql: ArchiveSql) {}

  // Whether a record has ended, in the very form the index over it is declared in.
  private endedAt(): string {
    return this.sql.dialect.jsonText("body", "endedAt");
  }

  async beginExecution(record: ExecutionRecord): Promise<ExecutionRecord> {
    return await this.sql.transaction(async () => {
      const counter = await this.sql.get("INSERT INTO execution_attempts(root, attempt) VALUES (?, 1) ON CONFLICT(root) DO UPDATE SET attempt=execution_attempts.attempt+1 RETURNING attempt", record.root!) as { attempt: number };
      const allocated = { ...record, attempt: counter.attempt };
      await this.sql.run("INSERT INTO agent_executions(id, body) VALUES (?, ?)", allocated.id, JSON.stringify(allocated));
      return allocated;
    });
  }

  async saveExecution(record: ExecutionRecord): Promise<void> {
    return await this.sql.wrote(async () => {
      const held = await this.sql.get("SELECT body FROM agent_executions WHERE id = ?", record.id) as { body: string } | null;
      if (held) {
        const previous = JSON.parse(held.body) as ExecutionRecord;
        for (const key of ["agent", "root", "parent", "attempt", "provider", "role", "nativeId", "effort"] as const) {
          if (previous[key] !== record[key]) throw new Error(`execution identity cannot change: ${key}`);
        }
      }
      await this.sql.run("INSERT INTO agent_executions(id, body) VALUES (?, ?) ON CONFLICT(id) DO UPDATE SET body=excluded.body", record.id, JSON.stringify(record));
    });
  }

  async executions(): Promise<ExecutionRecord[]> {
    return (await this.sql.all("SELECT body FROM agent_executions ORDER BY rowid DESC") as { body: string }[]).map(row => JSON.parse(row.body));
  }

  async execution(id: string): Promise<ExecutionRecord | null> {
    const row = await this.sql.get("SELECT body FROM agent_executions WHERE id = ?", id) as { body: string } | null;
    return row ? JSON.parse(row.body) : null;
  }

  // Ended records older than the moment given are dropped; open ones are never.
  async pruneExecutions(endedBefore: number): Promise<number> {
    return (await this.sql.run(`DELETE FROM agent_executions WHERE ${this.endedAt()} IS NOT NULL AND ${this.sql.dialect.jsonNumber("body", "endedAt")} < ?`, endedBefore)).changes;
  }

  // The rows that have not ended. Asked on every runner tick and by everything that wants to know
  // whether a name is still at work, so it is a query rather than a filter over every row ever
  // written: the table only grows, and the answer is a handful of rows at any moment.
  async openExecutions(): Promise<ExecutionRecord[]> {
    return (await this.sql.all(`SELECT body FROM agent_executions WHERE ${this.endedAt()} IS NULL ORDER BY rowid DESC`) as { body: string }[]).map(row => JSON.parse(row.body));
  }
}
