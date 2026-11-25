import { createHash, randomUUID } from "node:crypto";
import type { Archive } from "@agency/archive/archive";
import type { ExecutionRecord } from "@agency/archive/execution-store";
import type { SettingStore } from "@agency/archive/setting-store";
import { someInOrder } from "@agency/support/in-order";

export interface ExecutionIdentity {
  agent?: string | null;
  root?: string;
  parent?: string | null;
  role?: string | null;
  model?: string | null;
  effort?: string | null;
  sessionId?: string | null;
  panePath?: boolean;
}

// A cancellation stands as one settings row per name it covers, and every launch under such a
// name is refused while it does. The key, the reading and the dropping live beside the root's own
// definition because a launch asks under its root as well as its own name, and a second spelling
// of the key anywhere would be a refusal that never fired or one that never lifted.
export const executionCancelPrefix = "execution_cancel:";

export function executionCancelKey(name: string): string {
  return `${executionCancelPrefix}${name}`;
}

/** Whether a standing cancellation covers any of these names. */
export async function executionCancelled(
  settings: Pick<SettingStore, "getSetting"> | null | undefined,
  names: readonly (string | null | undefined)[],
): Promise<boolean> {
  if (!settings) return false;
  return await someInOrder(names, async (name) => name !== null && name !== undefined && name !== "" && await settings.getSetting(executionCancelKey(name)) !== null);
}

export function executionRoot(owner: string | null): string {
  return owner && /^bmo-task-/.test(owner) ? owner : `bmo-root-${randomUUID()}`;
}

export async function beginExecution(archive: Archive | null | undefined, provider: string, line: string, identity: ExecutionIdentity): Promise<ExecutionRecord> {
  const record: ExecutionRecord = {
    id: `bmo-execution-${randomUUID()}`, agent: identity.agent ?? null, root: identity.root ?? executionRoot(null),
    parent: identity.parent ?? null, role: identity.role ?? null,
    model: identity.model ?? null, effort: identity.effort ?? null, sessionId: identity.sessionId ?? null,
    provider, line, paneId: null, state: "launching", startedAt: Date.now(), endedAt: null, exitCode: null,
    panePath: identity.panePath,
  };
  if (archive) return await archive.executions().beginExecution(record);
  record.attempt = 1;
  return record;
}

// Provider IDs are opaque data, never paths, pane tokens, or concatenated namespaces.
export function nativeExecution(parent: ExecutionRecord, nativeId: string): ExecutionRecord {
  const digest = createHash("sha256").update(JSON.stringify([parent.id, parent.provider, nativeId])).digest("hex");
  // A child of a pane-path turn is not itself one: it has no pane of its own to be absent from,
  // so the evidence that ends it is its parent's ending and never a pane's.
  return { ...parent, id: `bmo-native-${digest}`, parent: parent.id, nativeId,
    sessionId: null, model: null, effort: null, line: "native-child", sharedPane: true, panePath: undefined, startedAt: Date.now(), endedAt: null, exitCode: null };
}
