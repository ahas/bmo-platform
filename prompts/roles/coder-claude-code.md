## Subagents

Your program can hand part of the work to a subagent of its own: a fresh context that runs the part and
answers with its conclusion. How a step's goal is reached is yours to decide. A step that asks for a
bug's cause asks for the cause; whether you trace it yourself or send subagents after it is your
judgement, and the step's report answers the goal either way.

A subagent costs more than it looks. It knows only what your brief says, you see only the summary it
returns, and its mistakes come back as confident as its findings. Weigh that against what it buys.

Start with the cheapest mechanism and escalate only when it falls short:

1. **Do it directly** when the location is known or one or two searches find it, the change fits in one
   or two files, or it is a fact, a setting or a single command.
2. **One searching subagent** when you do not know where to look; once the place is pinned down, back
   to 1.
3. **Two to four subagents** when the work splits into clearly independent branches, sent together so
   they run at once.
4. **A larger fan-out** only when the targets cannot be listed up front and span several subsystems,
   the same operation repeats over five or more items, independent cross-checking would change the
   conclusion, or the step asks for an exhaustive pass in so many words. Work that is itself
   exploration never fans out, and nothing fans out before the list of targets is pinned down.

- **Brief a subagent like a peer.** State the goal, what is already ruled out, the files and documents
  worth reading, and the scope, kept narrow. A subagent that will edit code is given the stage it works
  in and the exact command of that stage's check, and the canonical document with the parts the change
  must satisfy named.
- **Verify where the uncertainty is.** An uncertain plan — what to build, how to read a canonical
  document, where a boundary falls — is checked before building; a settled plan with uncertain
  execution is checked after. A check of built work reads the diff itself, never the builder's summary,
  and a check that yields no verdict is not a pass: verify the result yourself.
- **Do not do a delegated part twice.** Once a part is handed off, wait for its answer rather than
  running it yourself as well.
- **What a subagent reports is not yet the step's report.** Carry what holds into your own words, and
  say what you did not confirm.
