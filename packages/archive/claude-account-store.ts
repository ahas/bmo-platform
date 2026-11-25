// The archive's `claude_accounts` table, read and written here and nowhere else in the archive
// (docs/store.md).

import type { ArchiveSql } from "./archive-sql";

// How a run reaches an account's credential (docs/cache-fallback.md, 「The Claude Code road is a
// list of accounts」): a configuration directory holding a login, a file holding a long-lived token,
// or another provider's Anthropic-compatible endpoint reached under a configuration home of the
// account's own with the key its file holds.
export type ClaudeCredentialKind = "directory" | "token" | "endpoint";

// One Claude account agency holds. The row says where its credential is reached and nothing about
// what is in there; `packages/agent/claude-account.ts` is what says which ids, names and paths may be
// written into one.
export interface ClaudeAccountRow {
  id: string;
  name: string;
  place: number;
  enabled: boolean;
  credentialKind: ClaudeCredentialKind;
  credentialPath: string;
  // An endpoint account's own: where its requests go, the file its key sits in and the model map as
  // the JSON text the row keeps. Null on the other kinds, whose credential is the path alone.
  endpointUrl: string | null;
  keyFile: string | null;
  endpointModels: string | null;
  // The base prompt the account names, or null. A kept prompt cannot be removed from under it
  // (docs/base-prompts.md).
  basePrompt: string | null;
  addedAt: number;
  // When this login was found to be gone — nothing runs on it until a person signs in again.
  // Null while it is standing, which is every account's ordinary state.
  needsLoginAt: number | null;
  // The user whose account it is once the multi-user model is in force; null on the file
  // (docs/store.md).
  userId: number | null;
}

export interface NewClaudeAccountRow {
  id: string;
  name: string;
  enabled: boolean;
  credentialKind: ClaudeCredentialKind;
  credentialPath: string;
  // An endpoint account's own and a base prompt a row names; absent writes none.
  endpointUrl?: string | null;
  keyFile?: string | null;
  endpointModels?: string | null;
  basePrompt?: string | null;
}

interface ClaudeAccountDbRow {
  id: string;
  name: string;
  place: number;
  enabled: number;
  credential_kind: ClaudeCredentialKind;
  credential_path: string;
  endpoint_url?: string | null;
  key_file?: string | null;
  endpoint_models?: string | null;
  base_prompt?: string | null;
  added_at: number;
  needs_login_at: number | null;
  owner_account?: number | null;
}

export class ClaudeAccountStore {
  constructor(private readonly sql: ArchiveSql) {}

  // The Claude accounts in the order the owner put them, the off ones keeping their places. Where the
  // places cannot say which comes first — every one of them the same, as a half-written order
  // would leave them — the order they were registered in decides, so a list never reorders itself
  // on its own. That order is the table's own insertion order rather than the moment written into
  // a row: two accounts registered in one second carry the same moment and would otherwise be
  // read out in whichever order their names happen to sort in.
  async claudeAccounts(): Promise<ClaudeAccountRow[]> {
    const rows = await this.sql.all("SELECT * FROM claude_accounts ORDER BY place, rowid") as ClaudeAccountDbRow[];

    return rows.map((row) => this.claudeAccountOf(row));
  }

  /** The accounts in the order they were registered, which is the order with nothing else said. */
  async claudeAccountsAsAdded(): Promise<ClaudeAccountRow[]> {
    const rows = await this.sql.all("SELECT * FROM claude_accounts ORDER BY rowid") as ClaudeAccountDbRow[];

    return rows.map((row) => this.claudeAccountOf(row));
  }

  async getClaudeAccount(id: string): Promise<ClaudeAccountRow | null> {
    const row = await this.sql.get("SELECT * FROM claude_accounts WHERE id = ?", id) as ClaudeAccountDbRow | null;

    return row === null ? null : this.claudeAccountOf(row);
  }

  // An account joins the end of the order. Where it goes afterwards is the owner's to drag.
  async addClaudeAccount(account: NewClaudeAccountRow, now: number = Date.now()): Promise<void> {
    return await this.sql.wrote(async () => {
      const last = await this.sql.get("SELECT MAX(place) AS place FROM claude_accounts") as { place: number | null };

      await this.sql.run(`
          INSERT INTO claude_accounts (
            id, name, place, enabled, credential_kind, credential_path,
            endpoint_url, key_file, endpoint_models, base_prompt, added_at
          )
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        `, account.id, account.name, (last.place ?? 0) + 1, account.enabled ? 1 : 0, account.credentialKind, account.credentialPath, account.endpointUrl ?? null, account.keyFile ?? null, account.endpointModels ?? null, account.basePrompt ?? null, Math.floor(now / 1000));
    });
  }

  // What the owner changed about an account. Only what was named moves; its id and the moment it was
  // added are the record and are not the owner's to write.
  async updateClaudeAccount(
    id: string,
    changes: {
      name?: string;
      place?: number;
      enabled?: boolean;
      credentialKind?: ClaudeCredentialKind;
      credentialPath?: string;
      // Null clears the column; absent leaves it as it stands.
      endpointUrl?: string | null;
      keyFile?: string | null;
      endpointModels?: string | null;
      basePrompt?: string | null;
    },
  ): Promise<boolean> {
    const sets: string[] = [];
    const args: (string | number | null)[] = [];

    if (changes.name !== undefined) {
      sets.push("name = ?");
      args.push(changes.name);
    }

    if (changes.place !== undefined) {
      sets.push("place = ?");
      args.push(changes.place);
    }

    if (changes.enabled !== undefined) {
      sets.push("enabled = ?");
      args.push(changes.enabled ? 1 : 0);
    }

    if (changes.credentialKind !== undefined) {
      sets.push("credential_kind = ?");
      args.push(changes.credentialKind);
    }

    if (changes.credentialPath !== undefined) {
      sets.push("credential_path = ?");
      args.push(changes.credentialPath);
    }

    for (const [column, value] of [
      ["endpoint_url", changes.endpointUrl],
      ["key_file", changes.keyFile],
      ["endpoint_models", changes.endpointModels],
      ["base_prompt", changes.basePrompt],
    ] as const) {
      if (value !== undefined) {
        sets.push(`${column} = ?`);
        args.push(value);
      }
    }

    if (sets.length === 0) {
      return false;
    }

    return (await this.sql.run(`UPDATE claude_accounts SET ${sets.join(", ")} WHERE id = ?`, ...args, id)).changes > 0;
  }

  async deleteClaudeAccount(id: string): Promise<boolean> {
    return (await this.sql.run("DELETE FROM claude_accounts WHERE id = ?", id)).changes > 0;
  }

  // The whole order in one write: every account's place and its on/off together, so the sequence
  // and the switches are never half moved and no reader can see one without the other.
  async setClaudeAccountOrder(order: readonly { id: string; place: number; enabled: boolean }[]): Promise<void> {
    await this.sql.transaction(async () => {
      for (const one of order) await this.sql.run("UPDATE claude_accounts SET place = ?, enabled = ? WHERE id = ?", one.place, one.enabled ? 1 : 0, one.id);
    });
  }

  private claudeAccountOf(row: ClaudeAccountDbRow): ClaudeAccountRow {
    return {
      id: row.id,
      name: row.name,
      place: row.place,
      enabled: row.enabled === 1,
      credentialKind: row.credential_kind,
      credentialPath: row.credential_path,
      endpointUrl: row.endpoint_url ?? null,
      keyFile: row.key_file ?? null,
      endpointModels: row.endpoint_models ?? null,
      basePrompt: row.base_prompt ?? null,
      addedAt: row.added_at,
      needsLoginAt: row.needs_login_at ?? null,
      userId: row.owner_account === undefined || row.owner_account === null ? null : Number(row.owner_account),
    };
  }

  // A login found gone, or found standing again. Nothing else about the account moves: it keeps
  // its place, its name and its switch, and it is the walk that passes it over while this stands.
  async setClaudeAccountLogin(id: string, needsLoginAt: number | null): Promise<boolean> {
    return (await this.sql.run("UPDATE claude_accounts SET needs_login_at = ? WHERE id = ?", needsLoginAt, id)).changes > 0;
  }
}
