import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { watchClock } from "@agency/agent/overtime-clock";

import type { OvertimeAnswer, TurnOverdue } from "@agency/agent/overtime-clock";

// Clocks of a few milliseconds, so the clock passes at once and only the order of what it does is
// looked at.
const boundMs = 5;

// Resolves once the run was cut, or with false once nothing has cut it for a while.
function cutWithin(clock: { cut: Promise<void> }, ms = 2_000): Promise<boolean> {
  return Promise.race([clock.cut.then(() => true), Bun.sleep(ms).then(() => false)]);
}

function watched(overtime: (overdue: TurnOverdue) => Promise<OvertimeAnswer>) {
  let cut!: () => void;
  const done = new Promise<void>((resolve) => {
    cut = resolve;
  });
  const watch = watchClock({ boundMs, startedAt: Date.now(), overtime, activity: () => ({ at: 1, what: "tool_use Bash" }), cut, onPolicyFault: () => undefined });

  return { cut: done, watch };
}

describe("a clock on a run of its own", () => {
  test("asks its policy with what the run says of itself, runs on while it keeps, and cuts on its cut", async () => {
    const asked: TurnOverdue[] = [];
    const clock = watched(async (overdue) => {
      asked.push(overdue);

      return asked.length < 3 ? { cut: false, askAgainAt: Date.now() + 1 } : { cut: true };
    });

    expect(await cutWithin(clock)).toBe(true);
    expect(asked).toHaveLength(3);
    expect(asked[0]).toMatchObject({ boundMs, lastActivityAt: 1, lastActivity: "tool_use Bash" });
  });

  test("goes on where the policy cannot answer, and asks again after the clock's own length", async () => {
    let asks = 0;
    const clock = watched(async () => {
      asks += 1;
      throw new Error("the archive is gone");
    });

    expect(await cutWithin(clock, 100)).toBe(false);
    expect(asks).toBeGreaterThan(1);
    clock.watch.stop();
  });

  test("cuts nothing once stopped, even with an answer still coming", async () => {
    let answer!: (value: OvertimeAnswer) => void;
    let asking!: () => void;
    const asked = new Promise<void>((resolve) => {
      asking = resolve;
    });
    const clock = watched(() => {
      asking();

      return new Promise((resolve) => {
        answer = resolve;
      });
    });

    await asked;
    clock.watch.stop();
    answer({ cut: true });
    expect(await cutWithin(clock, 50)).toBe(false);
  });
});
