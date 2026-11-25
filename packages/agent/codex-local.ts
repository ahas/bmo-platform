import { existsSync, lstatSync, mkdirSync, readFileSync, symlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { dataPath, homeDir } from "@agency/support/roots";

import type { UsageWindowReading } from "./agent-reply";

// What this machine's Codex keeps on disk, for everything that reads it without running the program:
// the two homes, where the threads' rollouts are written, and the account windows a rollout carries.

// The Codex home the interactive program runs under. The program has no flag that shuts the user's
// configuration out (docs/agent-control.md, 「What a resident turn reads」), so it is given a home of
// Agency's own whose configuration is agency's alone. The login and the threads are the user's: the login
// file and the sessions directory are links into the user's home, so the program goes out on the
// same login `exec` does and resumes the same threads, whose rollouts stay where every reader of
// them looks.
export function codexResidentHome(): string {
  return dataPath("codex-home");
}

// The user's own Codex home, whose login and threads the resident home shares.
export function codexUserHome(): string {
  return join(homeDir(), ".codex");
}

// Where every Codex thread's rollout is written, under whichever of the two homes it is read from.
export function codexSessionsRoot(home: string = codexUserHome()): string {
  return join(home, "sessions");
}

// The resident home made ready for a program started in `cwd`: the shared login and threads linked,
// and the folder trusted in its own configuration, which is what keeps the trust screen from coming
// up — the screen appears even with a trust override on the command line, and answering it writes
// this same entry (measured).
export function prepareCodexHome(cwd: string, home: string = codexResidentHome(), user: string = codexUserHome()): void {
  mkdirSync(home, { recursive: true, mode: 0o700 });
  mkdirSync(codexSessionsRoot(user), { recursive: true, mode: 0o700 });

  for (const shared of ["auth.json", "sessions"]) {
    const link = join(home, shared);
    let present = false;

    try {
      lstatSync(link);
      present = true;
    } catch {
      present = false;
    }

    if (!present) symlinkSync(join(user, shared), link);
  }

  const configPath = join(home, "config.toml");
  const held = existsSync(configPath) ? readFileSync(configPath, "utf8") : "";

  if (trustedIn(held, cwd)) return;

  writeFileSync(configPath, `${held}${held === "" || held.endsWith("\n") ? "" : "\n"}[projects.${JSON.stringify(cwd)}]\ntrust_level = "trusted"\n`, { mode: 0o600 });
}

// Whether a configuration already holds the folder's entry, read as the TOML it is; one that does not
// read is taken to hold none, and the entry is added after what it holds.
function trustedIn(config: string, cwd: string): boolean {
  try {
    const projects = (Bun.TOML.parse(config) as { projects?: Record<string, unknown> }).projects;

    return projects !== undefined && Object.hasOwn(projects, cwd);
  } catch {
    return false;
  }
}

// The account windows a rollout's token count carries, named the way the account reading names
// them, so a window read from a turn and one read from the account are the same window.
export function codexRolloutWindows(limits: unknown): (UsageWindowReading & { mode: "codex"; durationMinutes: number | null; limitName: string | null })[] {
  if (typeof limits !== "object" || limits === null) return [];

  const held = limits as Record<string, unknown>;
  const id = typeof held.limit_id === "string" && held.limit_id !== "" ? held.limit_id : "default";
  const name = typeof held.limit_name === "string" && held.limit_name !== "" ? held.limit_name : null;

  return (["primary", "secondary"] as const).flatMap((slot) => {
    const window = held[slot] as { used_percent?: unknown; window_minutes?: unknown; resets_at?: unknown } | null | undefined;

    if (typeof window?.used_percent !== "number" || window.used_percent < 0) return [];

    return [{
      name: `codex:${id}:${slot}`,
      mode: "codex" as const,
      utilization: window.used_percent / 100,
      resetsAt: typeof window.resets_at === "number" ? Math.floor(window.resets_at) : null,
      durationMinutes: typeof window.window_minutes === "number" && window.window_minutes > 0 ? Math.floor(window.window_minutes) : null,
      limitName: name,
    }];
  });
}
