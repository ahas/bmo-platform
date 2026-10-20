import { afterAll, describe, expect, test } from "bun:test";
import { rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { scratchDir } from "../../support/scratch";
import { answer, standIn } from "../../support/units";

import type { DeployProgress } from "../../../apps/web/shared/deploy-progress";

// What `/api/deploy` hands a tab (docs/web.md, 「Deployment in the title bar」): the testbed's progress record read at
// the path the setting names, in the shape the testbed writes it, and the deployment's hold as bmo-job keeps it.

const scratch = scratchDir("bmo-web-deploy-");

afterAll(() => rmSync(scratch, { recursive: true, force: true }));

let holding: unknown = null;

standIn("JOB", (one) => (one.path.startsWith("/queues")
  ? answer({
      items: [{
        name: "default",
        state: "open",
        holds: holding === null ? [] : [{ actorName: "deploy", reason: "a job", until: 9_000 }],
        leasedCount: 0,
        openCount: 0,
        changedAt: 0
      }],
      more: false
    })
  : answer({ code: "not_found" }, 404)));

const { deployProgressPath, deployViewFor, readDeployHoldRow, readDeployReading } = await import("../../../apps/web/server/utils/deploy");

const record: DeployProgress = {
  unit: "deploy-unit-a",
  pid: 42,
  commit: "c".repeat(40),
  startedAt: 1_000,
  plan: ["backup", "restart", "after"],
  stages: [{ stage: "backup", at: 1_500 }, { stage: "restart", at: 2_000 }],
  steps: [
    { name: "pack the backup", part: "pack", stage: "backup", code: 0, waitedSeconds: 0, ranSeconds: 0.3, at: 1_800 },
    { name: "release build", stage: "restart", code: 0, waitedSeconds: 1, ranSeconds: 2, at: 1_900 }
  ],
  ahead: [{ name: "after-step" }],
  running: { name: "wait", stage: "restart", at: 2_100 },
  rolledBack: false,
  ending: null
};
const hold = { since: 1, until: 2, commits: [], breaks: [], waitingOn: [], waitingFor: [], passing: [] };

describe("the record", () => {
  test("is read at the path the setting names, as the testbed writes it", () => {
    const path = join(scratch, "deploy-progress.json");

    writeFileSync(path, `${JSON.stringify(record, null, 2)}\n`);

    expect(deployProgressPath({ DEPLOY_PROGRESS_FILE: path })).toBe(path);
    expect(readDeployReading(path).record).toEqual(record);
    expect(readDeployReading(path).writtenAt).toBeGreaterThan(0);
  });

  test("is none where no path is named, the file is missing or it does not read as a record", () => {
    const broken = join(scratch, "broken.json");

    writeFileSync(broken, "{ not json");

    expect(deployProgressPath({})).toBeNull();
    expect(readDeployReading(null)).toEqual({ record: null, writtenAt: null });
    expect(readDeployReading(join(scratch, "missing.json")).record).toBeNull();
    expect(readDeployReading(broken).record).toBeNull();
  });

  test("leaves off a stage it does not know rather than the record", () => {
    const path = join(scratch, "unknown-stage.json");

    writeFileSync(path, JSON.stringify({ ...record, stages: [...record.stages, { stage: "elsewhere", at: 3_000 }] }));

    expect(readDeployReading(path).record?.stages).toEqual(record.stages);
  });
});

describe("the deployment a tab is handed", () => {
  test("the administrator is handed the run and the hold as they stand, with every heavy step the run ended", () => {
    const view = deployViewFor({ record, writtenAt: 3_000 }, hold, "r1", true, () => true);

    expect(view).toEqual({
      release: "r1",
      detailed: true,
      hold,
      run: { phase: "running", startedAt: 1_000, endedAt: null, outcome: null, commit: record.commit, plan: record.plan, stages: record.stages, steps: record.steps, ahead: record.ahead, running: record.running }
    });
  });

  test("a run whose process went without ending is cut off at the record's last write", () => {
    expect(deployViewFor({ record, writtenAt: 3_000 }, null, "r1", true, () => false).run).toMatchObject({ phase: "cut", endedAt: 3_000 });
  });

  test("a member is handed only that a run is under way, and nothing of a run that ended or of the hold", () => {
    expect(deployViewFor({ record, writtenAt: 3_000 }, hold, "r1", false, () => true)).toEqual({
      release: "r1",
      detailed: false,
      hold: null,
      run: { phase: "running", startedAt: 1_000, endedAt: null, outcome: null, commit: null, plan: [], stages: [], steps: [], ahead: [], running: null }
    });
    expect(deployViewFor({ record: { ...record, ending: { outcome: "live", at: 4_000, why: null } }, writtenAt: 4_000 }, null, "r1", false).run).toBeNull();
  });
});

describe("the hold", () => {
  test("is bmo-job's, read as the TODO board reads it, and none where none stands", async () => {
    expect(await readDeployHoldRow()).toBeNull();

    holding = { actorName: "deploy", reason: "a job", until: 9_000 };

    expect(await readDeployHoldRow()).toEqual({ since: 0, until: 9, commits: [], breaks: null, waitingOn: ["a job"], waitingFor: null, passing: [] });
  });
});
