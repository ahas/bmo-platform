// How long a work order actually worked, told from the records it already keeps: a task holds the
// moment it started, and the job row it names holds the moment that run finished. Nothing else is
// written down for this.
//
// The reading that matters is the running, not the stretch it is spread over. An order is stopped
// and picked up a week later; a stage waits for a place in the pool; one branch of a stage is rerun
// days after the others. The clock between those is an order standing still, and counting it says
// an order took a week when it worked for an hour.
//
import type { WorkTask } from "./types";

// A stretch a task was running for, in unix seconds. It opens where the task started and closes
// where the job that ran it finished; a task still going has no close yet and runs to whatever
// second is being counted against.
export interface WorkRun {
  from: number;
  to: number | null;
}

// When a task started, in unix seconds. Nothing where it never began.
export function taskStartSeconds(task: WorkTask): number | null {
  return task.startedAt === null ? null : Math.floor(task.startedAt / 1000);
}

// When a task was running, given the ends the archive holds by task. A task that never began has no
// run, and neither has a settled one whose end is not there: a guessed end would be counted as work
// nobody did, and leaving it out says so by being absent.
export function taskRun(task: WorkTask, ended: Readonly<Record<string, number>>): WorkRun | null {
  const from = taskStartSeconds(task);

  if (from === null) {
    return null;
  }

  const to = ended[task.id];

  if (to !== undefined) {
    return { from, to };
  }

  return task.state === "running" ? { from, to: null } : null;
}

// How much time a set of runs takes together. They are merged where they overlap, so a stage's
// branches running side by side count once rather than several times over, and only what is inside
// a run counts at all — everything between the runs is time nothing was running and is counted for
// none of it. Nothing where there is no run to measure, since a nought would read as work that took
// no time.
export function runSeconds(runs: readonly WorkRun[], now: number): number | null {
  const spans = runs
    .map((run) => ({ from: run.from, to: Math.max(run.from, run.to ?? now) }))
    .sort((one, other) => one.from - other.from);
  const first = spans[0];

  if (first === undefined) {
    return null;
  }

  let whole = 0;
  let open = first;

  for (const span of spans.slice(1)) {
    if (span.from > open.to) {
      whole += open.to - open.from;
      open = span;
    } else if (span.to > open.to) {
      open = { from: open.from, to: span.to };
    }
  }

  return whole + (open.to - open.from);
}
