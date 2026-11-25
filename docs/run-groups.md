# What a run leaves running

A run starts processes of its own beside its program: a dev server it checks a screen against, a browser
and the test-browser daemon driving it, a watcher, a build it put in the background. None of them outlives
the run. When the run ends, however it ends, every process it started ends with it, and a run that needs a
server again starts it again.

## Which runs

- **A run in a pane of its own** (`docs/agents.md`, 「A run in a pane of its own」) on this machine's herdr:
  a job's stage, a worker ask, a search, a browse and every other run opened in a workspace of its own.
  Its program is started inside a run group of its own, and the group and the run's record are what end
  its processes.
- **A run put into a co-work branch checkout's standing program:** the program outlives the run, so the
  run has no group of its own and is ended by what it left marked (「A run in a standing program」). A
  supervisor's standing program is not a run's: what its turns start is its own.
- **Not a run's:** a checkout's dev server stands in that checkout's own fixed workspace as herdr's own
  child and carries no run name (`docs/worker-bench.md`, 「Dev server」); the panes of agency's own
  heads, links and room followers; a person's shell; a unit the system manager runs. None of them is ever
  taken down by a run's end.

## How it ends

A run ends in one of three ways, and each passes the same point: the run's program ends.

- **It finishes:** the turn's end is read, the door closes the pane, and the program ends on the hangup.
- **It is stopped:** the stop's words end the turn, or the stop closes the pane (`docs/agent-control.md`,
  「Stopping」); either way the program ends.
- **It is cut or lost:** the program dies, herdr loses the pane, agency's server restarts and the turn's
  pickup closes the pane, or the leftover sweep closes the pane of a run nothing carries any more.

So what is taken down is keyed on the program ending, never on which of the ways got there.

## The run group

- **The program is started through a launcher.** The pane's command is agency's own binary under its
  run-group verb, with the run's name, the record's path and the program's whole command after it. The
  launcher makes the group, writes the record, starts the run's reaper and then replaces itself with the
  program, so the program keeps the process id the launcher had, and its command line, its arguments and
  its environment are exactly the line's. Nothing the program's session holds as its cached prefix moves.
- **The group is a control group of its own** under the one the launcher was started in, which on this
  machine is herdr's unit, named after the run's execution. Every process the program starts lands in it,
  and stays in it whether it leaves the program's session, makes a session of its own, or is re-parented
  to the system manager when the process that started it ends.
- **The group turns on no controller.** Memory and processor time stay charged to herdr's unit, so the
  unit's own limits hold over every run as they hold over herdr.
- **A process can leave the group only by moving itself,** as a browser that puts itself into a scope of
  its own does. Such a process is still found by the run's record (「The record」).
- **Where no group can be made** — the control group is not writable, or the machine has no unified
  hierarchy — the launcher says so in the record and the run is taken down by its record alone.

## The record

- **Every grouped run writes one record** in the data root's `run-groups/`, named after the run's
  execution: the execution, the run's name, the folder it works in, the group's path where it has one,
  the program's process id and start, the reaper's process id and start, and when it began.
- **A process escaped from the group is found by the record:** a process whose environment names the
  run's name (`BMO_RUN_NAME`) and which began inside the run's window, with every member of that
  process's session where the session's leader is such a process or has gone. A browser's helpers, whose
  environment cannot be read, share their browser's session and are found that way.
- **The run's window** opens at the program's start and closes at the start of the next record of the
  same name, so a later run under the same name never loses what it started to an earlier run's ending.
- **A name with no record is never taken down:** agency's own standing panes carry a run's name too, and
  only a run that wrote a record is ever looked for by its name.

## Taking it down

1. **Every member is asked to end:** the processes in the group and every escaped process the record
   finds, each sent TERM.
2. **They are given the grace** the run-groups policy names to end by themselves.
3. **Whatever still stands is killed:** the group killed whole and every escaped process still standing
   sent KILL, then waited on for the bound the policy names.
4. **The group and the record are removed** once nothing of the run is left. Where something still
   stands past the bound, the record stays and the sweep takes the run down again.
5. **One line is written for each run that held anything,** to `run-groups.log` in the data root, and
   to agency's journal as well where the door or the sweep took it down: the run, its folder, how many
   processes ended, the memory they held and their commands.

A take-down is the same whoever does it and may be done twice: a process already gone is passed over, and a
group or record already removed is not an error.

- **A look reads the machine's processes one at a time,** letting each go before the next, so it holds only
  a few files open however many processes run. A unit's soft limit on open files stands far below the number
  of processes a busy machine runs, and a reaper started in herdr's pane carries that limit.
- **A look that could not read every process never counts as finding nothing.** A process gone or not the
  account's to read is passed over; a reading this process failed itself, for want of a descriptor or of
  memory, makes the look partial. The first look is taken again until one is whole, up to the grace; a
  partial look during the waits counts as something still standing; and a take-down whose last look was
  partial leaves the group and the record for the sweep.

## Who takes it down

- **The run's reaper.** The launcher starts it before the program, outside the run's group and in a
  session of its own, so neither the group's ending nor the pane's hangup reaches it. It waits on the
  program and takes the run down the moment the program ends. It needs nothing of agency's server, so a
  run ends whole while agency is down or restarting.
- **The door, at the run's end.** Once the pane is closed and the turn has settled, the door waits for
  the program to be gone, up to the grace, and takes the run down itself before it hands the turn back,
  so the caller frees the run's slot with nothing of the run left running.
- **The sweep.** Every ten minutes agency takes down every record whose program is gone. It catches a
  reaper that died, which herdr's own restart does along with everything in herdr's unit, and the
  escaped processes such a restart leaves outside the unit.
- **An agency server on a copy of the data root** sweeps nothing and answers the route with nothing: the
  machine is not the copy's.

## A run in a standing program

- **The program stands across runs,** so a group of the program's would hold every run's processes at
  once, and none is made.
- **At the run's end the door takes down what the run marked:** every process whose environment names
  the place (`BMO_AGENT_PLACE`), which began once the run's program stood — after any person's turn in it
  was waited out — and which stands outside the program's
  own session, with every member of that process's session as above. The program's own helpers — its
  tool servers and hooks — share the program's session; a command its shell tool runs opens a session of
  its own, which is what a server or browser the run started stands in.
- **Nothing is recorded for it,** so a standing program's run whose end the door did not see is not taken
  down later.

## Reading and asking for it

- `bmo-agency dev-server list` lists every run's record beside the checkouts' dev servers: the run, the
  folder, whether its program still runs, how many processes the run holds and their memory; `--sweep`
  takes down every one whose program is gone.
- `DELETE /repositories/{repositoryName}/servers` takes down every record of a run that worked in that
  checkout and whose program is gone, answering one line for each run it took down (`docs/api.md`).
