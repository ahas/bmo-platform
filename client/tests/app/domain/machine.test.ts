import { describe, expect, test } from "bun:test";

import { holderName, machineParts, machineShortParts, memoryAmount, testbedRows } from "../../../apps/web/app/domain/machine";

import type { MachineStanding } from "../../../apps/web/shared/types";

// The machine's room on the live board (docs/web.md, 「Live status screen」): every figure in its own
// unit and precision, with thousands separators. The keys and their values stand in for the language pack.
const t = (key: string, values: Record<string, unknown> = {}): string => `${key}(${Object.entries(values).map(([name, value]) => `${name}=${String(value)}`).join(",")})`;

const standing: MachineStanding = {
  reading: {
    at: 0,
    load1: 1234.56,
    threads: 24,
    memTotalGb: 31.9,
    memAvailableGb: 7.47,
    swapTotalGb: 16,
    swapFreeGb: 0.19,
    diskFreeGb: null,
    memory: { some: { avg10: 1, avg60: 0.71, avg300: 1 }, full: { avg10: 1, avg60: 0.64, avg300: 1 } },
    cpu: null,
  },
  jobs: 4,
  browserJobs: 2,
  browsers: 3,
  held: [],
  holders: [],
  largest: [],
};

describe("the machine's line", () => {
  test("gives each figure in its unit, at one precision, with thousands separators", () => {
    expect(machineParts(t, standing)).toEqual([
      "live.label.machine_load(load=1,234.6,threads=24)",
      "live.label.machine_pressure(pct=0.6)",
      "live.label.machine_available(free=7.5,total=31.9)",
      "live.label.machine_swap(free=0.2,total=16.0)",
      "live.label.machine_jobs(jobs=4,browsers=2)",
    ]);
  });

  test("gives a phone the pressure and the jobs alone", () => {
    expect(machineShortParts(t, standing)).toEqual(["live.label.machine_pressure(pct=0.6)", "live.label.machine_jobs_short(jobs=4)"]);
    expect(machineShortParts(t, { ...standing, reading: null })).toEqual(["live.label.machine_unread()", "live.label.machine_jobs_short(jobs=4)"]);
  });

  test("says the machine could not be read rather than show noughts", () => {
    expect(machineParts(t, { ...standing, reading: null })).toEqual(["live.label.machine_unread()", "live.label.machine_jobs(jobs=4,browsers=2)"]);
    expect(machineParts(t, { ...standing, reading: { ...standing.reading!, memory: null } })[1]).toBe("live.label.machine_pressure_unread()");
  });

  test("names memory in GB from a gigabyte up and in MB below it", () => {
    expect(memoryAmount(3500)).toBe("3.4 GB");
    expect(memoryAmount(1_536_000)).toBe("1,500.0 GB");
    expect(memoryAmount(512.4)).toBe("512 MB");
  });

  // The testbed's queue: one more part of the line while a command holds the turn, and its rows in the
  // popover, the holder first.
  test("carries the testbed's queue while a command holds its turn", () => {
    const holder = { place: "worker-w2", checkout: "/repo/bmo-w2", command: "the close", asked: 1, pid: 7, jobId: 5, todoId: 1290 };
    const waiting = [{ place: null, checkout: "/repo/jake", command: "cargo test", asked: 2, pid: 8, jobId: null, todoId: null, kept: true }];
    const queued = { ...standing, testbed: { holder, waiting } };

    expect(machineParts(t, queued).at(-1)).toBe("live.label.machine_testbed_part(waiting=1)");
    expect(machineParts(t, { ...standing, testbed: { holder: null, waiting: [] } })).toHaveLength(5);
    expect(testbedRows(t, queued)).toEqual([
      { key: "7-1", label: "live.label.machine_testbed_held()", who: "live.label.machine_job_item(job=5,item=1290,place=worker-w2)", what: "the close", kept: false },
      { key: "8-2", label: "live.label.machine_testbed_waiting(n=1)", who: "jake", what: "cargo test", kept: true },
    ]);
    expect(testbedRows(t, standing)).toEqual([]);
  });

  test("names a job's memory by its job and item, an agent's by its own name", () => {
    expect(holderName(t, { place: "worker-w4", jobId: 903, todoId: 1202, megabytes: 1, browsers: 0 })).toBe("live.label.machine_job_item(job=903,item=1202,place=worker-w4)");
    expect(holderName(t, { place: "worker-w4", jobId: 903, todoId: null, megabytes: 1, browsers: 0 })).toBe("live.label.machine_job(job=903,place=worker-w4)");
    expect(holderName(t, { place: "agent-a", jobId: null, todoId: null, megabytes: 1, browsers: 0 })).toBe("agent-a");
  });
});
