# The agents' sleep

Once a day, at a quiet hour, agency starts each supervisor's sleep: one run of the supervisor's own outside every
room, which consolidates what its brain holds pending, folds its duplicates together, closes what has ended and
says what came of it (the brain's `docs/design.md`, 「Sleep」). The brain stays passive and runs nothing; agency's
clock is what starts a sleep, and agency writes its summary and tells the owner.

## When a supervisor sleeps

- **Once a day, at its hour.** A supervisor sleeps at the hour its setting names (「Its setting」), on the
  deployment's clock, the policy's `utc_offset_hours` (`policy/sleep.toml`), and within the policy's
  `window_hours` from that hour on. The day it sleeps for is the date its window began on, so a window that runs
  past midnight sleeps for the day before.
- **At most one try an hour.** A sleep that could not go out, or did not answer, is tried again in the next hour of
  the window and no sooner; a window that ends with no sleep leaves that day unslept, and the next day's window
  tries again.
- **Only while the cards are free.** The nodes' cards are their owner's first (the arbiter's claims): a sleep goes
  out only while no card of any node stands under a claim, as the arbiter's master pictures them, and a claim
  standing puts the sleep off to the next hour. A deployment naming no arbiter has no claims, and its sleeps are
  never put off for one.
- **Only where there is something to sleep on.** A supervisor with no brain does not sleep. One whose brain holds
  no unconsolidated episode and no row written since its last sleep — the moment of the newest summary its brain
  holds — is counted as having slept that day, and no run goes out.
- **The clock** is a pass of agency's own, every `tick_seconds`, that sleeps the supervisors due one after another
  and never runs beside itself.

## The run

- **On the supervisor's own road, apart from its sessions.** The sleep is a conversation of the supervisor's
  (`docs/api.md`, 「sessions」): started on the provider and model the supervisor's road resolves to now, in a
  folder of its own under the agents' tree, one turn, and ended once that turn answers. It is the supervisor's own
  program on the chat line, with its persona and its brain's tools, so what it writes is the supervisor's; it
  answers no room, resumes none of the supervisor's sessions, and leaves none of them changed.
- **What it is asked** is the policy's sleep instruction, with the date and the token budget in it: read every
  unconsolidated episode, as many readings as it takes and within the budget, write what each comes to through the
  write judgement and mark each consolidated; read the duplicate candidates and merge the rows that say one thing,
  write the fuller of two as a new version, and for a contradiction keep the newer or the better grounded and
  invalidate the other; list the live `state`, `followup` and `promise` rows and move each whose release condition
  has come true to `done`; say nothing in any room; and answer one JSON object, `summary` (what was learned, merged
  and closed, in English) and the counts `learned`, `merged` and `closed`.
- **What agency writes of it.** An answer that reads is written into the supervisor's brain as one `reflection`,
  its text the summary, labelled `sleep` with the date, written as `sleep`; the owner is then told in one record-only
  line, the receiver's `slept` with the date and the counts, kept in the supervisor's owner's private room and sent
  to no chat service (the receiver's `docs/api.md`, 「Kept lines」). The day is then slept. An answer that does not
  read writes nothing and tells nobody, and the sleep is tried again in the next hour of the window.

## Its setting

- **One setting a supervisor, on its own screen:** whether it sleeps and at what hour, `on` and `hour` (0 to 23,
  or none for the policy's `hour`), read and written on `GET` and `PUT /supervisors/{supervisorName}/sleep`
  (`docs/api.md`, 「supervisors」). A supervisor whose setting was never written sleeps, at the policy's hour: every
  supervisor with a brain sleeps unless its setting says otherwise.
- **What the reading answers beside it:** the policy's hour it falls back on, the day it last slept for, and the
  moment it last tried.
- **Kept in the store's `supervisor_sleep` table,** one row a supervisor, let go with the supervisor.
