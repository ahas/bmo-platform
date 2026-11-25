import { existsSync, mkdirSync, readFileSync, renameSync, rmdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join } from "node:path";

import { homeDir } from "@agency/support/roots";

// Whether Claude Code trusts a folder, and making it trust one, the way the CLI itself keeps it
// (`docs/agent-control.md`, 「A first interactive run stops at the trust-this-folder check」).
//
// The CLI asks once per folder and per login whether the folder is to be trusted, and an interactive
// program sits on that question before it takes anything. The answer is the folder's entry in the
// CLI's global configuration file, so a folder written there as trusted never asks at all. The same
// write declines the CLI's plugin recommendation for the login, a choice it otherwise stands a turn's
// end on (「A turn's end stops at a plugin recommendation」).

const configName = ".claude.json";
const trustKey = "hasTrustDialogAccepted";
const declinedKey = "lspRecommendationDisabled";

// How old a lock may be before it is taken as one its holder left behind: the CLI's holder touches its
// lock every few seconds while it holds it, so an untouched one is a holder gone, not a slow one.
const lockStaleMs = 10_000;

/**
 * The configuration file a run started on this environment reads its folder trust from: the one in
 * the configuration directory it is handed, and the home's own where it is handed none.
 */
export function claudeConfigFile(env: Record<string, string | undefined>): string {
  const named = env.CLAUDE_CONFIG_DIR;

  return named === undefined || named === "" ? join(homeDir(), configName) : join(named, configName);
}

function readConfig(file: string): Record<string, unknown> | null {
  try {
    const held: unknown = JSON.parse(readFileSync(file, "utf8"));

    return typeof held === "object" && held !== null && !Array.isArray(held) ? held as Record<string, unknown> : null;
  } catch {
    return null;
  }
}

function projectsOf(config: Record<string, unknown>): Record<string, unknown> {
  const projects = config.projects;

  return typeof projects === "object" && projects !== null && !Array.isArray(projects) ? projects as Record<string, unknown> : {};
}

function entryTrusts(entry: unknown): boolean {
  return typeof entry === "object" && entry !== null && (entry as Record<string, unknown>)[trustKey] === true;
}

// The CLI's lock on its configuration file is a directory beside it, and holding it is what keeps a
// program saving its configuration from writing back a copy read before this one's entry. It is waited
// for until its holder lets it go, however long that takes.
function takeLock(lock: string): void {
  for (;;) {
    try {
      mkdirSync(lock);
      return;
    } catch {
      // Held: by a writer at work, or by one that died holding it.
    }

    try {
      if (Date.now() - statSync(lock).mtimeMs > lockStaleMs) {
        rmdirSync(lock);
        continue;
      }
    } catch {
      continue;
    }

    Bun.sleepSync(25);
  }
}

/** How a folder stands in one configuration file once asked to be trusted there. */
export type FolderTrust = "trusted" | "written" | "no-config-dir" | "unreadable";

/**
 * The folder written as trusted in the file and the plugin recommendation declined there, every other
 * entry kept as it stood. Nothing is written where the folder is already trusted and the recommendation
 * already declined, where the configuration directory is not there — no login
 * lives in it — or where the file is there and is not the CLI's configuration.
 */
export function trustFolder(file: string, folder: string): FolderTrust {
  if (!isAbsolute(folder)) throw new Error(`folder trust is kept by absolute path, not ${folder}`);
  if (!existsSync(dirname(file))) return "no-config-dir";

  const lock = `${file}.lock`;

  takeLock(lock);

  try {
    const present = existsSync(file);
    const config = present ? readConfig(file) : {};

    if (config === null) return "unreadable";

    const projects = projectsOf(config);
    const entry = projects[folder];

    const trusted = entryTrusts(entry);

    if (trusted && config[declinedKey] === true) return "trusted";

    const kept = typeof entry === "object" && entry !== null ? entry as Record<string, unknown> : {};
    const withTrust = trusted ? config : { ...config, projects: { ...projects, [folder]: { ...kept, [trustKey]: true } } };
    const next = { ...withTrust, [declinedKey]: true };
    const mode = present ? statSync(file).mode & 0o777 : 0o600;
    const fresh = `${file}.bmo-trust-${process.pid}`;

    writeFileSync(fresh, `${JSON.stringify(next, null, 2)}\n`, { mode });
    renameSync(fresh, file);

    return "written";
  } finally {
    rmdirSync(lock);
  }
}

/**
 * The file written as a configuration whose first-run questions are answered, every other entry kept
 * as it stood. A configuration home agency makes for a road of its own holds no login, and an
 * interactive program started on one asks for a colour theme before it takes anything. Nothing is
 * written where the file says so already, or where the directory is not there.
 */
export function markOnboarded(file: string): "onboarded" | "written" | "no-config-dir" | "unreadable" {
  if (!existsSync(dirname(file))) return "no-config-dir";

  const lock = `${file}.lock`;

  takeLock(lock);

  try {
    const present = existsSync(file);
    const config = present ? readConfig(file) : {};

    if (config === null) return "unreadable";
    if (config.hasCompletedOnboarding === true) return "onboarded";

    const fresh = `${file}.bmo-trust-${process.pid}`;

    writeFileSync(fresh, `${JSON.stringify({ theme: "dark", ...config, hasCompletedOnboarding: true }, null, 2)}\n`, { mode: present ? statSync(file).mode & 0o777 : 0o600 });
    renameSync(fresh, file);

    return "written";
  } finally {
    rmdirSync(lock);
  }
}

