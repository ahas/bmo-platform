// Agency's store as the TypeScript side reaches it (docs/store.md): one PostgreSQL database at the
// address the store is named by, whose shape is the store crate's migrations and nothing here. Every
// store below reaches its tables through the one port, so the statements any of them runs inside a
// transaction are one commit.

import type { ArchiveSql } from "./archive-sql";
import { PostgresArchiveSql } from "./postgres-archive-sql";
import { AgentStore } from "./agent-store";
import { ClaudeAccountStore } from "./claude-account-store";
import { ExecutionStore } from "./execution-store";
import { FallbackStore } from "./fallback-store";
import { ModelRegistryStore } from "./model-registry";
import { RunStore } from "./run-store";
import { SettingStore } from "./setting-store";
import { UsageStore } from "./usage-store";
import { WorkNodeStore } from "./work-node-store";

export class Archive {
  // The port every store reaches its tables through, and the connection under it.
  readonly sql: ArchiveSql;
  private readonly port: PostgresArchiveSql;
  private settingStore: SettingStore | null = null;
  private runStore: RunStore | null = null;
  private agentStore: AgentStore | null = null;
  private executionStore: ExecutionStore | null = null;
  private claudeAccountStore: ClaudeAccountStore | null = null;
  private fallbackStore: FallbackStore | null = null;
  private modelRegistryStore: ModelRegistryStore | null = null;
  private usageStore: UsageStore | null = null;
  private workNodeStore: WorkNodeStore | null = null;

  /** The store at an address, as the store crate's migrations shaped it. */
  constructor(readonly path: string) {
    this.port = PostgresArchiveSql.open(path);
    this.sql = this.port;
  }

  /** The store at an address; the same as constructing it, named for what a caller reads. */
  static open(address: string): Archive {
    return new Archive(address);
  }

  /** Several writes as one commit. */
  transaction<T>(work: () => Promise<T>): Promise<T> {
    return this.sql.transaction(work);
  }

  /** The connection given back; nothing of this archive is read after. */
  close(): Promise<void> {
    return this.port.close();
  }

  /** Agency's own settings rows (docs/store.md, 「Its tables」). */
  settings(): SettingStore {
    return this.settingStore ??= new SettingStore(this.sql);
  }

  /** The run log, every line a run wrote (docs/run-records.md). */
  runs(): RunStore {
    return this.runStore ??= new RunStore(this.sql, this.agents());
  }

  /** The supervisors and the repositories' checkouts, as the TypeScript side reads them (docs/agents.md). */
  agents(): AgentStore {
    return this.agentStore ??= new AgentStore(this.sql);
  }

  /** The executions placed in panes (docs/agent-control.md). */
  executions(): ExecutionStore {
    return this.executionStore ??= new ExecutionStore(this.sql);
  }

  /** The Claude accounts the walk goes over (docs/cache-fallback.md). */
  claudeAccounts(): ClaudeAccountStore {
    return this.claudeAccountStore ??= new ClaudeAccountStore(this.sql);
  }

  /** The sessions kept on the roads other than a supervisor's own (docs/cache-fallback.md). */
  fallbacks(): FallbackStore {
    return this.fallbackStore ??= new FallbackStore(this.sql);
  }

  /** The registered models (docs/own-llm.md). */
  modelRegistry(): ModelRegistryStore {
    return this.modelRegistryStore ??= new ModelRegistryStore(this.sql);
  }

  /** The prefix moves the sessions' system prompts made (docs/sessions.md). */
  usage(): UsageStore {
    return this.usageStore ??= new UsageStore(this.sql);
  }

  /** The nodes' workplaces (docs/work-nodes.md). */
  workNodes(): WorkNodeStore {
    return this.workNodeStore ??= new WorkNodeStore(this.sql);
  }
}
