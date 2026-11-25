# A run past its clock

Read this when you are handed a run that reached its time limit: an order in the owner's chat
headed `[A run reached its clock — keep it or cut it]`. The run is a word said to a worker, a browse,
a voice line being spoken, or a job's tool call that has gone silent and idle.

## What you are handed

- Which agent and run it is and what it is doing, how long it has run, when it last did anything and
  what that was, and which clock it reached with its length.
- The ask's number, which every command below takes.
- Nothing has been cut. The run goes on while you decide, and only your cut ends it: a clock never
  does.

## Deciding

- **The decision is yours alone.** Never ask the owner whether to keep or cut it; he left it to you.
- Look before you decide, as far as the facts leave it open: the run log the order names, a
  worker's screen and its pane, what the last activity was and how long ago.
- **Others wait on a kept run.** A word said to a worker holds the worker's lock, and the worker's
  queue behind it waits for as long as you keep it.
- **Keep it** when it is still working: its last activity is recent, or what it is waiting on (a
  build, a test run, a long command) is expected to take this long.
- **Cut it** when it is stuck: nothing written for a long while with no command that explains the
  wait, the same thing repeated, a question nobody is going to answer, or work that has gone off what
  it was asked.
- A fault you see in the run or in the clock itself (a run that keeps running long for a reason in
  the system, a clock that asks too soon for the work) is filed as a queue item and fixed by a job, as
  todo.md says.

## A job's tool call gone silent and idle

- The order names the worker, the job and its item, the call's command, how long it has run, when its
  output last grew and the last line it wrote. Its clock is the silent window: for that long the call
  wrote nothing and its processes used almost no processor time. That is how a wait that polls looks,
  a loop that sleeps and then looks for something.
- Read the command for what it waits on, then decide:
  - Keep a wait the work genuinely hangs on, such as a testbed line or a deployment the run must wait
    on, and always give `--for` with when to look again.
  - Cut a wait on another job's work, such as its close, its commit or its item, and a wait on an
    outside call that has stopped answering. What it waits on will not come while the call holds the
    turn.
- A cut call comes back to the job as a failed result with your reason, and the job goes on.

## Answering

- Keep: `bmo-agency overtime --keep <number> [--for <length>] --why '<one line>'`
  - `--for` takes `20m`, `1h` or a plain number of minutes; without it the run gets the clock's own
    length again, and two hours is the most one answer gives.
  - When that runs out with the run still going, you are asked again.
- Cut: `bmo-agency overtime --cut <number> --why '<one line>'`
  - The run is cut within seconds, and the record says it was your call and why.
- List the asks still open: `bmo-agency overtime --list`
- A refusal saying the run already ended means it finished on its own or was stopped: there is
  nothing left to decide.
- Until you answer, the run goes on and the ask stands; nothing cuts it for want of an answer.

## Telling the owner

- Once you have answered, nothing needs saying: answer `<<SKIP>>`.
- Where you cut it or filed an item about it, tell him in one short line what you did and why.
