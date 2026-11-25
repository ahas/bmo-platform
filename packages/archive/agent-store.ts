// The supervisors and the repositories' checkouts as the TypeScript side reads them (docs/agents.md, 「Two
// axes」): the `supervisors`, `repositories` and `repository_node_checkouts` tables, read here and nowhere
// else on this side. A checkout is a place, never an agent: each repository's base, the support worktrees its
// count asks for beside it and the checkouts it keeps on the work nodes (docs/worker-slots.md), named as the
// server names them. Nothing here writes a row: the server's routes do.

import declaredWorkers from "@agency/policy/workers.toml";

import { conversationOf } from "./conversational";
import type { Conversation, ConversationColumns } from "./conversational";
import type { AgentProviderName as AgentMode } from "./provider-names";
import type { ArchiveSql } from "./archive-sql";

// What a row is read as: a supervisor's, or a repository's checkout.
export type AgentKind = "chat" | "worker";

/** A supervisor or a checkout, as what stands it in herdr and what its runs go out on read it. */
export interface AgentRecord {
  name: string;
  kind: AgentKind;
  // What a screen calls it, and the name herdr shows it under where the owner set one other than the label.
  label: string | null;
  herdrLabel: string | null;
  // Where a checkout's jobs work, and the node it stands on; null on a supervisor and on a checkout here.
  cwd: string | null;
  node: string | null;
  // The repository a support worktree or a node checkout is of; null on a base and on a supervisor.
  supportOf: string | null;
  // The model the repository pins its work to, a provider and one model of it, or null where it pins none
  // (docs/repositories.md, 「A pinned model」); null on a supervisor.
  pinnedProvider: AgentMode | null;
  pinnedModel: string | null;
  // How the one runner carries a supervisor (docs/supervisors.md, 「One runner」); null on a checkout.
  conversation: Conversation | null;
}

interface SupervisorRow extends ConversationColumns {
  name: string;
  label: string | null;
  herdr_label: string | null;
}

interface RepositoryRow {
  name: string;
  label: string | null;
  path: string;
  node: string | null;
  // Null where the repository is set to `auto`, its support checkouts then the ones it recorded.
  support_count: number | string | null;
  pinned_provider: AgentMode | null;
  pinned_model: string | null;
}

// The first support worktree's number, the policy's: the base is the first of its repository.
const firstSupportSlot = (declaredWorkers as { first_support_slot: number }).first_support_slot;

export class AgentStore {
  constructor(private readonly sql: ArchiveSql) {}

  // How the one runner carries a supervisor, or null where no supervisor has the name.
  async conversationOf(name: string): Promise<Conversation | null> {
    const row = await this.supervisorRow(name);

    return row === null ? null : conversationOf(row);
  }

  // The name a room's chat runs under: the supervisor of one session across its rooms whose turn went into
  // that room last, as the run ledger records it, the name itself otherwise.
  async canonicalAgent(name: string): Promise<string> {
    if (!/^chat:-?\d+$/.test(name)) return name;

    const row = await this.sql.get(
      "SELECT r.agent AS agent FROM runs r JOIN supervisors s ON s.name = r.agent WHERE r.chat_id = ? AND s.session_scope = 'agent' ORDER BY r.id DESC LIMIT 1",
      Number(name.slice("chat:".length)),
    ) as { agent: string } | null;

    return row?.agent ?? name;
  }

  // A supervisor or a checkout by its name, or null where neither stands under it.
  async getAgent(name: string): Promise<AgentRecord | null> {
    name = await this.canonicalAgent(name);

    const supervisor = await this.supervisorRow(name);

    if (supervisor !== null) {
      return {
        name: supervisor.name,
        kind: "chat",
        label: supervisor.label,
        herdrLabel: supervisor.herdr_label,
        cwd: null,
        node: null,
        supportOf: null,
        pinnedProvider: null,
        pinnedModel: null,
        conversation: conversationOf(supervisor),
      };
    }

    return await this.checkout(name);
  }

  private async supervisorRow(name: string): Promise<SupervisorRow | null> {
    return await this.sql.get("SELECT name, label, herdr_label, session_scope, turn_shape FROM supervisors WHERE name = ?", name) as SupervisorRow | null;
  }

  private async repository(name: string): Promise<RepositoryRow | null> {
    return await this.sql.get("SELECT name, label, path, node, support_count, pinned_provider, pinned_model FROM repositories WHERE name = ?", name) as RepositoryRow | null;
  }

  // Whether a repository has a support worktree at a slot: within its count, or recorded where it is set to
  // `auto` (docs/worker-slots.md, 「The base checkout and the support checkouts」).
  private async standsAt(of: RepositoryRow, slot: number): Promise<boolean> {
    if (slot < firstSupportSlot) return false;
    if (of.support_count !== null) return slot < firstSupportSlot + Number(of.support_count);

    return await this.sql.get("SELECT 1 AS held FROM support_checkouts WHERE repository = ? AND slot = ?", of.name, slot) !== null;
  }

  // A checkout by its name: a repository's base, a support worktree `<base>-w<slot>` within its count or recorded
  // for a repository set to `auto`, or a checkout on a node `<base>@<node>` its row keeps.
  private async checkout(name: string): Promise<AgentRecord | null> {
    const base = await this.repository(name);

    if (base !== null) return checkoutOf(base, name, base.label, base.path, nonEmpty(base.node), null);

    const support = /^(.+)-w(\d+)$/.exec(name);

    if (support !== null) {
      const of = await this.repository(support[1]!);
      const slot = Number(support[2]);

      if (of !== null && await this.standsAt(of, slot)) {
        return checkoutOf(of, name, `${of.label ?? of.name}#${slot}`, `${of.path}-w${slot}`, nonEmpty(of.node), of.name);
      }
    }

    const onNode = /^(.+)@([^@]+)$/.exec(name);

    if (onNode !== null) {
      const of = await this.repository(onNode[1]!);
      const kept = of === null ? null : await this.sql.get("SELECT path FROM repository_node_checkouts WHERE repository = ? AND node = ?", of.name, onNode[2]!) as { path: string } | null;

      if (of !== null && kept !== null) {
        return checkoutOf(of, name, `${of.label ?? of.name}@${onNode[2]!}`, kept.path, onNode[2]!, of.name);
      }
    }

    return null;
  }
}

function nonEmpty(value: string | null): string | null {
  return value === null || value.trim() === "" ? null : value;
}

// A checkout of a repository: its pin is the repository's, whichever of its checkouts the work stands in.
function checkoutOf(repository: RepositoryRow, name: string, label: string | null, cwd: string, node: string | null, supportOf: string | null): AgentRecord {
  const pinned = nonEmpty(repository.pinned_provider) as AgentMode | null;

  return {
    name,
    kind: "worker",
    label,
    herdrLabel: null,
    cwd,
    node,
    supportOf,
    pinnedProvider: pinned,
    pinnedModel: pinned === null ? null : nonEmpty(repository.pinned_model),
    conversation: null,
  };
}
