import { readFileSync } from "node:fs";
import { join } from "node:path";

import { machineConfigDir } from "@agency/archive/account-seed";

// Where the Claude CLI keeps what it writes of a session: its projects directory, each working
// directory's records under it, and one session's transcript by its id. The CLI names these, so
// they are read the way it writes them.

// Where the CLI keeps everything it wrote for the sessions launched in one directory: that
// directory's path with every character that is not a letter or a digit turned into a dash —
// the dot of a hidden directory as much as a separator — under the CLI's own projects directory.
// The naming is the CLI's and it is the only way back from a working directory to what was
// written there. The CLI's own directory is `CLAUDE_CONFIG_DIR` where that is set, as it is for the
// CLI itself, and `.claude` under the home otherwise; a root handed in wins over both.
export function projectDir(cwd: string, root?: string): string {
  return join(root ?? cliProjectsRoot(), cwd.replace(/[^A-Za-z0-9]/g, "-"));
}

// The CLI's own projects directory, every working directory's records under it. Which
// configuration directory that is belongs to the account a session was written under, and the
// machine's own is what a process asking about nothing else gets.
export function cliProjectsRoot(configDir: string = machineConfigDir()): string {
  return join(configDir, "projects");
}

// Where the CLI keeps one session's transcript, which is the only way back to a session's file
// from its id.
export function transcriptPath(cwd: string, sessionId: string, root?: string): string {
  return join(projectDir(cwd, root), `${sessionId}.jsonl`);
}

// What the last request of one session carried, read off its record: everything the model was sent,
// read from the cache or written into it. A record keeps each request's own usage where a run's result
// sums every request of a turn that called tools, and a streamed answer of some roads carries none.
// Null where the record names no request with any.
export function lastRequestTokensFromText(raw: string): number | null {
  let last: number | null = null;

  for (const line of raw.split("\n")) {
    if (line.trim() === "") continue;

    let usage: Record<string, unknown> | undefined;

    try {
      usage = (JSON.parse(line) as { message?: { usage?: Record<string, unknown> } }).message?.usage;
    } catch {
      continue;
    }

    if (usage === undefined) continue;

    const count = (name: string): number => (typeof usage[name] === "number" ? usage[name] as number : 0);
    const tokens = count("input_tokens") + count("cache_creation_input_tokens") + count("cache_read_input_tokens");

    if (tokens > 0) last = tokens;
  }

  return last;
}

// The last request of one session's record; null where the record cannot be read.
export function readLastRequestTokens(path: string): number | null {
  try {
    return lastRequestTokensFromText(readFileSync(path, "utf8"));
  } catch {
    return null;
  }
}

