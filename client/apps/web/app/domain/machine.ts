import type { MachineHolder, MachineReading, MachineStanding, TestbedEntry } from "#shared/types";

// The machine's room as the live board shows it (docs/web.md, 「Live status screen」): the figures the
// bot's minute watch last read, each in its own unit and precision, and who holds the memory.

type Translate = (key: string, values?: Record<string, unknown>) => string;

// Where this machine's own monitor stands: the node unit's screen, which stands as its placeholder here.
export const LOCAL_MONITOR_PATH = "/monitor/local";

const oneDecimal = new Intl.NumberFormat("ko-KR", { minimumFractionDigits: 1, maximumFractionDigits: 1 });
const whole = new Intl.NumberFormat("ko-KR", { maximumFractionDigits: 0 });

/** Gigabytes at one decimal, with thousands separators. */
export function gigabytes(value: number): string {
  return oneDecimal.format(value);
}

/** An amount of memory given in megabytes, in GB from a gigabyte up and in MB below it. */
export function memoryAmount(megabytes: number): string {
  return megabytes >= 1024 ? `${oneDecimal.format(megabytes / 1024)} GB` : `${whole.format(megabytes)} MB`;
}

// The memory pressure of the last minute, or that it could not be read.
function pressurePart(t: Translate, reading: MachineReading): string {
  const full = reading.memory?.full?.avg60 ?? null;

  return full === null ? t("live.label.machine_pressure_unread") : t("live.label.machine_pressure", { pct: oneDecimal.format(full) });
}

/** The parts of the one line, each a short label; empty where the machine could not be read. */
export function machineParts(t: Translate, standing: MachineStanding): string[] {
  const { reading } = standing;
  const jobs = t("live.label.machine_jobs", { jobs: whole.format(standing.jobs), browsers: whole.format(standing.browserJobs) });

  if (reading === null) return [t("live.label.machine_unread"), jobs];

  return [
    t("live.label.machine_load", { load: oneDecimal.format(reading.load1), threads: whole.format(reading.threads) }),
    pressurePart(t, reading),
    t("live.label.machine_available", { free: gigabytes(reading.memAvailableGb), total: gigabytes(reading.memTotalGb) }),
    t("live.label.machine_swap", { free: gigabytes(reading.swapFreeGb), total: gigabytes(reading.swapTotalGb) }),
    jobs,
    ...testbedPart(t, standing),
  ];
}

// The testbed's turn as one more part of the line, where a command holds it.
function testbedPart(t: Translate, standing: MachineStanding): string[] {
  const testbed = standing.testbed;

  return testbed === undefined || testbed.holder === null ? [] : [t("live.label.machine_testbed_part", { waiting: whole.format(testbed.waiting.length) })];
}

/** One row of the testbed's queue as the popover lists it. */
export interface TestbedRow {
  key: string;
  // Where it stands: holding the turn, or its place in the line.
  label: string;
  // Whose it is: its job and item, else its place, else its checkout's directory.
  who: string;
  // The command, and whether its place is kept for a job coming back.
  what: string;
  kept: boolean;
}

function entryWho(t: Translate, entry: TestbedEntry): string {
  // A checkout is named by its own directory, which is what tells one from another in a narrow row.
  if (entry.place === null) return entry.checkout.split("/").filter((part) => part !== "").at(-1) ?? entry.checkout;

  return holderName(t, { place: entry.place, jobId: entry.jobId, todoId: entry.todoId, megabytes: 0, browsers: 0 });
}

/** The testbed's queue in order, the holder first; empty where nothing holds or waits. */
export function testbedRows(t: Translate, standing: MachineStanding): TestbedRow[] {
  const testbed = standing.testbed;

  if (testbed === undefined) return [];

  const row = (entry: TestbedEntry, label: string): TestbedRow => ({ key: `${entry.pid}-${entry.asked}`, label, who: entryWho(t, entry), what: entry.command, kept: entry.kept === true });

  return [
    ...(testbed.holder === null ? [] : [row(testbed.holder, t("live.label.machine_testbed_held"))]),
    ...testbed.waiting.map((entry, at) => row(entry, t("live.label.machine_testbed_waiting", { n: whole.format(at + 1) }))),
  ];
}

/** The two a phone's toolbar has room for: the memory pressure and the open jobs. */
export function machineShortParts(t: Translate, standing: MachineStanding): string[] {
  const jobs = t("live.label.machine_jobs_short", { jobs: whole.format(standing.jobs) });

  return standing.reading === null ? [t("live.label.machine_unread"), jobs] : [pressurePart(t, standing.reading), jobs];
}

/** Who one holder is: the job and its item where it is a job's, the agent otherwise. */
export function holderName(t: Translate, holder: MachineHolder): string {
  if (holder.jobId === null) return holder.place;

  return holder.todoId === null
    ? t("live.label.machine_job", { job: holder.jobId, place: holder.place })
    : t("live.label.machine_job_item", { job: holder.jobId, item: holder.todoId, place: holder.place });
}
