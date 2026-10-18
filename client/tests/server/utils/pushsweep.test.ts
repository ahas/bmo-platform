import { describe, expect, test } from "bun:test";

import { todoSweeper } from "../../../apps/web/server/utils/pushsweep";

import type { After } from "../../../apps/web/server/utils/pushsweep";

// What a run of the bot's "the table moved" costs this server. The events carry nothing, so what
// is rung about is read from the archive when one arrives — and a listener that has lost its place
// is handed the whole buffer at once. So what is held here is that the reading is done for the run
// and not for each word of it, and that collapsing it that way loses no news.

// A clock the test winds itself. Whatever was scheduled waits here until the test says so, which is
// what makes the settling observable rather than something to sit out in real seconds.
function clock(): { after: After; tick: () => Promise<void>; waiting: () => number } {
  let due: (() => void)[] = [];

  return {
    after: (_ms, run) => {
      due.push(run);
    },
    tick: async () => {
      const owed = due;

      due = [];

      for (const run of owed) {
        run();
      }

      // The sweep itself is a promise the timer does not wait on, so the test lets it finish
      // before it looks at what was done.
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    },
    waiting: () => due.length
  };
}

function counted(): { sweep: () => Promise<void>; times: () => number } {
  let times = 0;

  return {
    sweep: async () => {
      times += 1;
      await Promise.resolve();
    },
    times: () => times
  };
}

function failsLoudly(error: unknown): void {
  throw error;
}

describe("a run of the same news is read once", () => {
  test("three hundred events in one breath are one sweep, not three hundred", async () => {
    const { after, tick } = clock();
    const { sweep, times } = counted();
    const moves = todoSweeper({ sweep, failed: failsLoudly, after });

    for (let at = 0; at < 300; at += 1) {
      moves.moved();
    }

    // Nothing at all has been read yet: an event costs a flag and the window has not closed.
    expect(times()).toBe(0);

    await tick();

    expect(times()).toBe(1);
  });

  test("and no more sweeps stand waiting behind them", async () => {
    const { after, tick, waiting } = clock();
    const { sweep, times } = counted();
    const moves = todoSweeper({ sweep, failed: failsLoudly, after });

    for (let at = 0; at < 300; at += 1) {
      moves.moved();
    }

    // One window for the whole run, rather than one per event queued up behind it.
    expect(waiting()).toBe(1);

    await tick();

    expect(waiting()).toBe(0);
    expect(times()).toBe(1);
  });

  test("a lone event is still read, so one question rings as surely as a hundred do", async () => {
    const { after, tick } = clock();
    const { sweep, times } = counted();
    const moves = todoSweeper({ sweep, failed: failsLoudly, after });

    moves.moved();
    await tick();

    expect(times()).toBe(1);
  });
});

describe("what the collapsing does not lose", () => {
  test("news after a sweep is swept again rather than taken as already read", async () => {
    const { after, tick } = clock();
    const { sweep, times } = counted();
    const moves = todoSweeper({ sweep, failed: failsLoudly, after });

    moves.moved();
    await tick();
    expect(times()).toBe(1);

    moves.moved();
    await tick();
    expect(times()).toBe(2);
  });

  test("news arriving while a sweep is in flight is swept by the one that follows it", async () => {
    const { after, tick } = clock();
    let times = 0;
    let release = (): void => undefined;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const moves = todoSweeper({
      sweep: async () => {
        times += 1;

        if (times === 1) {
          await held;
        }
      },
      failed: failsLoudly,
      after
    });

    moves.moved();
    await tick();
    expect(times).toBe(1);

    // A row written after that read began, which the sweep in flight cannot have seen.
    moves.moved();
    release();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await tick();

    expect(times).toBe(2);
  });

  test("nothing new means nothing swept, so a quiet console reads the archive for nobody", async () => {
    const { after, tick } = clock();
    const { sweep, times } = counted();

    todoSweeper({ sweep, failed: failsLoudly, after });

    await tick();

    expect(times()).toBe(0);
  });
});

describe("a sweep that threw", () => {
  test("is said rather than swallowed, and the next one still happens", async () => {
    const { after, tick } = clock();
    const said: unknown[] = [];
    let times = 0;
    const moves = todoSweeper({
      sweep: async () => {
        times += 1;

        throw new Error(`sweep ${times}`);
      },
      failed: (error) => said.push(error),
      after
    });

    moves.moved();
    await tick();
    moves.moved();
    await tick();

    expect(times).toBe(2);
    expect(said).toHaveLength(2);
  });
});

describe("a console going down", () => {
  test("sweeps no more, whatever is marked after it", async () => {
    const { after, tick } = clock();
    const { sweep, times } = counted();
    const moves = todoSweeper({ sweep, failed: failsLoudly, after });

    moves.stop();
    moves.moved();
    await tick();

    expect(times()).toBe(0);
  });
});
