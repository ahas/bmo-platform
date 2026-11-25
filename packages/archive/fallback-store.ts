// The store's `fallback_sessions` table, read and written here and nowhere else on the TypeScript side
// (docs/cache-fallback.md).

import type { AgentProviderName as AgentMode } from "./provider-names";
import type { ArchiveSql } from "./archive-sql";

// A resumable session owned by a non-primary mode. Scope is the concrete room, run or gate instance
// it stands for, and the key the piece of work it ran for; keeping both is what keeps two rooms from
// sharing a conversation.
export interface FallbackSessionRow {
  scope: string;
  key: string;
  mode: AgentMode;
  model: string;
  effort: string;
  sessionId: string;
  lastReplyAt: number;
  runJson: string;
}

interface FallbackSessionDbRow {
  scope: string;
  key: string;
  mode: AgentMode;
  model: string;
  effort: string;
  sessionId: string;
  lastReplyAt: number;
  runJson: string;
}

export class FallbackStore {
  constructor(private readonly sql: ArchiveSql) {}

  async fallbackSession(scope: string, mode: AgentMode, model: string, effort: string): Promise<FallbackSessionRow | null> {
    return await this.sql.get(`
        SELECT scope, key, mode, model, effort, session_id AS "sessionId",
               last_reply_at AS "lastReplyAt", run_json AS "runJson"
        FROM fallback_sessions
        WHERE scope = ? AND mode = ? AND model = ? AND effort = ?
      `, scope, mode, model, effort) as FallbackSessionDbRow | null;
  }

  async fallbackSessions(mode?: AgentMode): Promise<FallbackSessionRow[]> {
    return (mode === undefined
      ? await this.sql.all(`
          SELECT scope, key, mode, model, effort, session_id AS "sessionId",
                 last_reply_at AS "lastReplyAt", run_json AS "runJson"
          FROM fallback_sessions ORDER BY last_reply_at
        `)
      : await this.sql.all(`
          SELECT scope, key, mode, model, effort, session_id AS "sessionId",
                 last_reply_at AS "lastReplyAt", run_json AS "runJson"
          FROM fallback_sessions WHERE mode = ? ORDER BY last_reply_at
        `, mode)) as FallbackSessionDbRow[];
  }

  async setFallbackSession(session: FallbackSessionRow): Promise<void> {
    await this.sql.run(`
        INSERT INTO fallback_sessions
          (scope, key, mode, model, effort, session_id, last_reply_at, run_json)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (scope, mode, model, effort) DO UPDATE SET key = excluded.key, session_id = excluded.session_id, last_reply_at = excluded.last_reply_at, run_json = excluded.run_json
      `, session.scope, session.key, session.mode, session.model, session.effort, session.sessionId, session.lastReplyAt, session.runJson);
  }

  async deleteFallbackSession(scope: string, mode: AgentMode, model: string, effort: string): Promise<void> {
    await this.sql.run("DELETE FROM fallback_sessions WHERE scope = ? AND mode = ? AND model = ? AND effort = ?", scope, mode, model, effort);
  }

}
