// A clock asks, it never cuts: a run carries a clock only where its caller hands it a policy, and each
// time the clock passes the policy is asked, with what the run can say of itself, and answers cut — the
// caller's own call — or the next moment to ask again (docs/agent-control.md, 「A clock asks, it never
// cuts」). This is the mechanism; the policy, and whoever it asks, is the caller's.

// What a run past its clock can say of itself to the policy deciding whether to cut it.
export interface TurnOverdue {
  // When it started, and the length of its clock.
  startedAt: number;
  boundMs: number;
  // The run's last sign of work: when it was and what it was, each null where the run cannot say.
  lastActivityAt: number | null;
  lastActivity: string | null;
}

// Cut now, or look again at a moment the policy names; the run goes on in between.
export type OvertimeAnswer = { cut: true } | { cut: false; askAgainAt: number };

export type Overtime = (overdue: TurnOverdue) => Promise<OvertimeAnswer>;

export interface ClockWatch {
  // Takes the clock away once the run has ended on its own; nothing is asked or cut after it.
  stop: () => void;
}

// The clock on a run that is one process or one call rather than a pane turn: `cut` is called once
// the clock has passed and the policy answered cut. A policy that cannot answer leaves the run going,
// asked again after the clock's own length. Its timers never hold the process up.
export function watchClock(options: {
  boundMs: number;
  startedAt?: number;
  overtime: Overtime;
  // The run's last sign of work, read each time the policy is asked.
  activity?: () => { at: number | null; what: string | null };
  cut: () => void | Promise<unknown>;
  // Where the policy's failure is said; the service's journal otherwise.
  onPolicyFault?: (error: unknown) => void;
  now?: () => number;
}): ClockWatch {
  const now = options.now ?? Date.now;
  const startedAt = options.startedAt ?? now();
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const at = (moment: number, then: () => void | Promise<unknown>): void => {
    timer = setTimeout(then, Math.max(1, moment - now()));
    timer.unref?.();
  };

  const cut = async (): Promise<void> => {
    if (stopped) return;

    stopped = true;
    await options.cut();
  };

  const look = async (): Promise<void> => {
    if (stopped) return;

    const last = options.activity?.() ?? { at: null, what: null };

    await options.overtime({ startedAt, boundMs: options.boundMs, lastActivityAt: last.at, lastActivity: last.what })
      .catch(async (error: unknown): Promise<OvertimeAnswer> => {
        (options.onPolicyFault ?? ((fault: unknown) => console.error(`a clock's policy could not answer, so the run goes on and is asked about again: ${fault instanceof Error ? fault.message : String(fault)}`)))(error);

        return { cut: false, askAgainAt: now() + options.boundMs };
      })
      .then(async (answer) => {
        if (answer.cut) await cut();
        else if (!stopped) at(answer.askAgainAt, look);
      });
  };

  at(startedAt + options.boundMs, look);

  return {
    stop: () => {
      stopped = true;

      if (timer !== null) clearTimeout(timer);
    },
  };
}
