import { join } from "node:path";

import { homeDir } from "@agency/support/roots";

// The one account a fresh archive holds: the login this machine already has. It is the whole
// reason an untouched installation answers exactly as it did before there were accounts at all —
// every path names an account, and on such an installation that account is the login the CLI
// would have used with nothing said. The archive's settling writes it, so it stands with the
// archive; what an account is, is the agent base's (`claude-account.ts`).
export const SEED_ACCOUNT_ID = "default";
export const SEED_ACCOUNT_NAME = "Default account";

/**
 * The configuration directory the CLI itself would use in this process's environment:
 * `CLAUDE_CONFIG_DIR` where that is set, and `.claude` under the home otherwise. It is what the
 * seeded account is pointed at, and the only place the machine's own login is read from.
 */
export function machineConfigDir(): string {
  const named = process.env.CLAUDE_CONFIG_DIR;

  return named === undefined || named === "" ? join(homeDir(), ".claude") : named;
}

/** The account a fresh archive is seeded with, standing for the login this machine holds. */
export function seedClaudeAccount(): { id: string; name: string; credential: { kind: "directory"; path: string } } {
  return {
    id: SEED_ACCOUNT_ID,
    name: SEED_ACCOUNT_NAME,
    credential: { kind: "directory", path: machineConfigDir() },
  };
}
