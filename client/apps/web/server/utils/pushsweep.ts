// How the console reads a run of the same news without doing the same work for every word of it.
//
// The bot's todo event carries nothing but the fact that the table moved, so what there is to ring
// about has to be read from the archive when one arrives. That makes every such event ask for the
// same read, and the read answers the same thing to all of them: the work is proportional to the
// number of events rather than to the news in them. It is not a rare shape either — the stream
// hands a listener that has lost its place the whole buffer at once, so a reconnect is a run of
// hundreds of these arriving in the same breath, and this server has one thread to answer requests
// with and to do that reading in.
//
// So the news is swept rather than answered. An event marks the table as moved and nothing more;
// one sweep per settling window reads it. The sweep reads the state and not the event, so the one
// that stands for two hundred of them finds exactly what two hundred sweeps would have found —
// nothing is lost by the collapsing, because there was never anything in an event to lose. What
// arrives while a sweep is in flight is swept by the one that follows it, so news landing in that
// gap is not dropped either, and a sweep never overlaps another.

// How long the window is. Long enough that a buffer arriving in one breath is one sweep, short
// enough that a notification is not noticeably late: a question put a second ago and a question
// put now are the same question to somebody reading it off a locked screen.
export const SETTLE_MS = 1_000;

// The clock this stands on, given rather than taken, so a test can drive the settling itself
// instead of waiting the window out in real seconds.
export type After = (ms: number, run: () => void) => void;

export interface TodoSweeper {
  // The table moved. Costs nothing but a flag and, at most, one timer.
  moved(): void;
  // No more sweeps, whatever has been marked. The server is going down.
  stop(): void;
}

export function todoSweeper(input: {
  sweep: () => Promise<void>;
  failed: (error: unknown) => void;
  settleMs?: number;
  after?: After;
}): TodoSweeper {
  const settleMs = input.settleMs ?? SETTLE_MS;
  const after: After = input.after ?? ((ms, run) => void setTimeout(run, ms));
  // Whether there is news nobody has swept for yet, whether a window is open, and whether a sweep
  // is going on right now. The three are separate because each answers a different question about
  // whether arranging another one would be the same sweep twice.
  let marked = false;
  let waiting = false;
  let sweeping = false;
  let stopped = false;

  function arrange(): void {
    if (stopped || waiting || sweeping || !marked) {
      return;
    }

    waiting = true;
    after(settleMs, () => {
      waiting = false;
      void run();
    });
  }

  async function run(): Promise<void> {
    if (stopped || sweeping || !marked) {
      return;
    }

    // Cleared before the sweep rather than after it: the sweep reads the archive as it stands from
    // here on, so anything already written is in what it is about to read, and only what is written
    // after this line is news the next one owes.
    marked = false;
    sweeping = true;

    try {
      await input.sweep();
    } catch (error) {
      input.failed(error);
    } finally {
      sweeping = false;
    }

    arrange();
  }

  return {
    moved(): void {
      if (stopped) {
        return;
      }

      marked = true;
      arrange();
    },
    stop(): void {
      stopped = true;
    }
  };
}
