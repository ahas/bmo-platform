import { watchName } from "~/domain/watch-name";
import { spanClock } from "bmo-ui/app/domain/time";

import type { LiveRun, WatchView } from "#shared/types";
import type { Translate } from "#shared/wording";

// The exit a watch's check is recorded with when it was cut off rather than ended.
const TIMED_OUT_EXIT = -1;

// The watch a run holds: the standing one whose row names the run's unit.
export function watchOfRun(watches: readonly WatchView[], run: Pick<LiveRun, "unit">): WatchView | null {
  return run.unit === null ? null : (watches.find((watch) => watch.unit === run.unit) ?? null);
}

// The standing watches no running line shows: every timer, and a check between two runs.
export function unheldWatches(watches: readonly WatchView[], runs: readonly Pick<LiveRun, "unit">[]): WatchView[] {
  const units = new Set(runs.flatMap((run) => (run.unit === null ? [] : [run.unit])));

  return watches.filter((watch) => watch.unit === null || !units.has(watch.unit));
}

// The watch's number and its short name; the rest opens with the whole of what it promised.
export function watchTitle(t: Translate, watch: WatchView): string {
  return t("live.label.watch_title", { id: watch.id, name: watchName(watch) });
}

// How often the watch looks and how it stands against its moment. The check itself stands whole
// beside this in the detail, so it is not squeezed onto this line.
export function watchWait(t: Translate, watch: WatchView, now: number): string {
  if (watch.kind === "timer") {
    const due = watch.dueAt ?? watch.deadlineAt;

    return due > now ? t("live.label.timer_left", { time: spanClock(t, due - now) }) : t("live.label.timer_passed");
  }

  const left =
    watch.deadlineAt > now ? t("live.label.remaining", { time: spanClock(t, watch.deadlineAt - now) }) : t("live.label.deadline_passed");

  return watch.intervalSeconds === null ? left : t("live.label.watch_every", { interval: spanClock(t, watch.intervalSeconds), left });
}

// How the last check went; null for a timer, which runs no check.
export function watchLastCheck(t: Translate, watch: WatchView, now: number): string | null {
  if (watch.kind === "timer") {
    return null;
  }

  if (watch.lastCheckedAt === null || watch.lastExit === null) {
    return t("live.status.unchecked");
  }

  const ago = spanClock(t, Math.max(0, now - watch.lastCheckedAt));

  if (watch.lastExit === TIMED_OUT_EXIT) {
    return t("live.label.last_check_timed_out", { ago });
  }

  return watch.lastExit === 0
    ? t("live.label.last_check_met", { ago })
    : t("live.label.last_check_unmet", { ago, code: watch.lastExit });
}
