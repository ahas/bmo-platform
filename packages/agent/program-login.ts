import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { accountHome, machineConfigDir } from "./claude-account";

import type { ClaudeAccount } from "./claude-account";

// The login a Claude program runs under, as its own environment names it, and the registered
// account that login belongs to. A resident program's status line runs in the program's own
// environment, so what it stamps on a reading is the login that made the reading; the reader
// attributes the reading by that stamp and never by the login a caller meant the turn to use.

/**
 * The configuration home the program reads its login from, and — for a program handed a token
 * instead — a fingerprint of that token. The fingerprint is a digest cut short, from which no token
 * can be made back: it lets a record say which token account wrote it without the token being
 * written anywhere.
 */
export interface ProgramLogin {
  configDir: string;
  token: string | null;
}

export function tokenFingerprint(token: string): string {
  return createHash("sha256").update(token.trim()).digest("hex").slice(0, 16);
}

export function programLogin(env: Record<string, string | undefined>): ProgramLogin {
  const dir = env.CLAUDE_CONFIG_DIR;
  const token = env.CLAUDE_CODE_OAUTH_TOKEN;

  return {
    configDir: dir === undefined || dir === "" ? machineConfigDir() : dir,
    token: token === undefined || token.trim() === "" ? null : tokenFingerprint(token),
  };
}

function fileFingerprint(path: string): string | null {
  try {
    return tokenFingerprint(readFileSync(path, "utf8"));
  } catch {
    return null;
  }
}

/**
 * The registered account a program's login belongs to, or null where none does. A program handed a
 * token answers on that token whatever directory it keeps its records in, so a token account is
 * matched by its token alone; any other program answers under a configuration home of its own, so a
 * directory account and an endpoint account — whose home holds no login but is still the account's
 * own — are matched by that home.
 */
export function accountOfLogin(accounts: readonly ClaudeAccount[], login: ProgramLogin): ClaudeAccount | null {
  if (login.token !== null) {
    return accounts.find((account) => account.credential.kind === "token" && fileFingerprint(account.credential.path) === login.token) ?? null;
  }

  const home = resolve(login.configDir);

  return accounts.find((account) => {
    const own = accountHome(account.credential);

    return own !== null && resolve(own) === home;
  }) ?? null;
}
