import { readFileSync, statSync } from "node:fs";

import { deployPhase, readDeployProgress } from "#shared/deploy-progress";

import { jobRead } from "./job";
import { holdOf } from "./todos";

import type { DeployLogLine, DeployRunView, DeployView } from "#shared/types";
import type { DeployProgress } from "#shared/deploy-progress";

// The deployment as the title bar reads it (docs/web.md, 「Deployment in the title bar」): the testbed's progress
// record (the testbed's `docs/deploy.md`, 「What a deployment says of itself as it runs」), read at the path the
// `DEPLOY_PROGRESS_FILE` setting names, whether the run behind it still runs, and the deployment's hold, which
// bmo-job keeps. None of the record asks a unit, so the reading stands while the units restart under the
// deployment; a console that names no record has no deployment to show.

/** The record's file, as the setting names it; null where it names none. */
export function deployProgressPath(environment: Record<string, string | undefined> = process.env): string | null {
  const named = environment.DEPLOY_PROGRESS_FILE?.trim() ?? "";

  return named === "" ? null : named;
}

/** Whether a process still runs, by asking the kernel whether it could be signalled. */
export function processAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);

    return true;
  } catch (error) {
    // Refused means it is there and is someone else's.
    return (error as NodeJS.ErrnoException).code === "EPERM";
  }
}

/** The record and when it was last written, or nothing where there is none. */
export interface DeployReading {
  record: DeployProgress | null;
  writtenAt: number | null;
}

export function readDeployReading(path: string | null = deployProgressPath()): DeployReading {
  if (path === null) return { record: null, writtenAt: null };

  let text: string | null;

  try {
    text = readFileSync(path, "utf8");
  } catch {
    text = null;
  }

  const record = readDeployProgress(text);

  if (record === null) return { record: null, writtenAt: null };

  try {
    return { record, writtenAt: statSync(path).mtimeMs };
  } catch {
    return { record, writtenAt: null };
  }
}

/** The run as a tab reads it. A run cut off ends at the record's last write, the latest moment known of it. */
export function deployRunView(record: DeployProgress, writtenAt: number | null, alive: (pid: number) => boolean = processAlive): DeployRunView {
  const phase = deployPhase(record, alive);

  return {
    phase,
    startedAt: record.startedAt,
    endedAt: record.ending?.at ?? (phase === "cut" ? (writtenAt ?? record.startedAt) : null),
    outcome: record.ending?.outcome ?? null,
    commit: record.commit,
    plan: record.plan,
    stages: record.stages,
    steps: record.steps,
    ahead: record.ahead,
    running: record.running
  };
}

/**
 * What a tab is handed. A member hears only that a run is under way and when it began: not what goes live, how
 * far it is, how it ended, nor the hold (docs/web.md, 「Access」).
 */
export function deployViewFor(reading: DeployReading, hold: DeployView["hold"], release: string, detailed: boolean, alive: (pid: number) => boolean = processAlive): DeployView {
  const run = reading.record === null ? null : deployRunView(reading.record, reading.writtenAt, alive);

  if (detailed) return { release, detailed, run, hold };

  return {
    release,
    detailed,
    run: run === null || run.phase !== "running" ? null : { ...run, commit: null, plan: [], stages: [], steps: [], ahead: [], running: null },
    hold: null
  };
}

/** The deployment's hold as bmo-job keeps it; none where bmo-job does not answer, since the mark then knows of none. */
export async function readDeployHoldRow(): Promise<DeployView["hold"]> {
  try {
    return holdOf((await jobRead<{ deploy_hold?: unknown }>("/queue/holds")).deploy_hold);
  } catch {
    return null;
  }
}

/**
 * The last lines of a deployment unit's journal, oldest first. journalctl hands each entry as one JSON object a
 * line; a message that is not text — journald keeps raw bytes as an array — is left out.
 */
export function deployJournal(unit: string, count: number): DeployLogLine[] {
  const done = Bun.spawnSync(["journalctl", "--user", "-u", unit, "-o", "json", "-n", String(count), "--no-pager"], { stdout: "pipe", stderr: "ignore", stdin: "ignore" });

  if (done.exitCode !== 0) return [];

  return done.stdout.toString().split("\n").flatMap((line) => {
    if (line.trim() === "") return [];

    try {
      const entry = JSON.parse(line) as { MESSAGE?: unknown; __REALTIME_TIMESTAMP?: unknown };
      const micros = Number(entry.__REALTIME_TIMESTAMP);

      return typeof entry.MESSAGE === "string" && Number.isFinite(micros) ? [{ at: Math.floor(micros / 1000), text: entry.MESSAGE }] : [];
    } catch {
      return [];
    }
  });
}
