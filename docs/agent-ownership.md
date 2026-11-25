# What each supervisor owns

Every supervisor (`docs/supervisors.md`) keeps its own state:
its sessions, its long-term memory, the reminders and watches that wake it, its gate settings, its voice
and its usage. This document is the canonical statement of how that state is split. Agency holds the
supervisor's row in the `supervisors` table and its room sessions; the other stores named below belong
to the units that keep them, and are listed so every consumer reads the same owner column.
`docs/supervisors.md` is what a supervisor's session does with it. A role owns none of this: a run of a
role keeps no memory, reminder or watch of its own (`docs/agent-roles.md`).

## The principle

- **One owner column, never one table per supervisor.** A store that holds several supervisors' rows
  keeps them in the table it already has, with a column naming the supervisor each row belongs to: the
  supervisor's name, as its row in the `supervisors` table carries it.
- **A value that differs per supervisor is the supervisor's own,** a column of its row in the
  `supervisors` table, never a table of supervisors kept elsewhere and never a branch on a supervisor's
  name.
- **Nothing counts the supervisors.** No code, schema, screen or test assumes a number of supervisors
  or names one of them: every reader takes the supervisor a turn is for and reads that supervisor's
  rows, so a supervisor added as a row has every part below on its own from its first turn.
- **The rows that stood before the split are the supervisor's.** Each owner column is added with a
  backfill naming the supervisor's row — the row of the deployment's agent key, read at the
  settling, never a literal name — so nothing the supervisor held moves or changes.
- **The proof is a third agent.** The tests stand a fake supervisor beside the two real agents and
  check that it gets its own session, reminders, watches, voice setting, usage and gate settings, and
  reads nothing of the others'; its brain is its own by its keys.

## What the supervisor's row carries

Beside its one road, its cache warming per provider and its compaction threshold
(`docs/supervisors.md`, 「What a supervisor row holds」):

| Column | What it says | A supervisor of one session across its rooms | One added on the console |
|---|---|---|---|
| `voice` | the voice its words are spoken in, or none | the briefing voice it speaks in now | none |
| `window_seconds` | how long a room it answered keeps it listening without a call, or none | the conversation window it has now | none |
| `aliases` | its call names, the words that call it beside its label (`docs/supervisors.md`, 「Its call names」) | the owner's other words for it | the call names written for it, or none |
| `session_scope` | one session across its rooms (`agent`) or one in each room (`room`) | `agent` | `room` |
| `turn_shape` | the shape of its turns: the window (`window`) or the pieces (`pieces`) | `window` | `pieces` |
| `prompts` | the folder under `prompts/agents/` a pieces-shaped turn's persona is read from, one plain name, or none | none: the chat line reads its own documents | none, or the folder its seed names |
| `owner_account` | the id of the login account that owns it, or none | none | the one its registration names, or none |

- **A supervisor of an account keeps its prompt apart from every other account's:** its system prompt, in either
  turn shape, is led by a mark made from its owner account alone, so no engine or provider reuses a cached prompt
  prefix across accounts, and no account's words are answered faster for having been read for another. A
  supervisor of no account carries no mark.
- **A supervisor of an account's program holds that account in `BMO_OWNER_ACCOUNT`,** the user's id as a
  decimal number, on every turn shape and road alike, and on the person's node, so what
  it writes for its person names them: a secret request is that person's (`docs/agent-documents.md`). The mark
  stays the prompt's only sign of the account, since the prompt is what a provider reads; the variable is
  the program's environment alone. A supervisor of no account's program holds none.
- **A supervisor with no voice makes no voice.** No briefing, voice chip or reading is made for its
  turns, and a voice given to its row later turns all three on with no other change
  (`voice-briefing.md`).
- **The window and the call names are read by the receiver,** which wakes the supervisor on them; the call
  names are also told to a supervisor of the `pieces` shape in its system prompt.
- **No column locates its memory.** Its long-term memory is its brain, found by its account and its
  key (「Long-term memory」 below). The row's `brain_folder` locates nothing; a supervisor's reset reads it
  only to refuse the supervisor whose value is empty, the brain's root.

## The stores split by an owner column

| Store | Its owner column | Who reads it |
|---|---|---|
| reminders | `agent` | the reminder clock, which wakes the owner when one comes due |
| watches | `agent` | the watch runner, which wakes the owner when its check ends |
| gate options | `agent` | the gates when they run for that supervisor's turn |
| usage runs | `agent` | the usage screens, grouped by supervisor |
| room sessions | `agent` | the runner, for a supervisor of the `room` scope (`docs/supervisors.md`, 「A session in each room」) |
| instructions | `agent` | the supervisor's own conversation: each supervisor's takes only its own rows |

- **Every write names the supervisor it is for.** A tool or a block writes the supervisor of the turn
  it runs in; a script reads the supervisor from the run that calls it, whose environment carries the
  supervisor's name, or from its `--agent`; a write with no supervisor is refused rather than filed under
  a default.
- **A reminder, a watch and a memory event wake their own supervisor:** their quiet instruction is
  queued for the supervisor the row is, and only that supervisor's turns take it, through its own
  conversation in the row's room, whatever its scope (`docs/supervisors.md`).
  Nothing of another supervisor's ever reaches a supervisor's turn.
- **A turn names its supervisor.** Every supervisor's turn carries its name in `BMO_TURN_AGENT`, which
  the reminder and watch scripts and the chat tools read; a script also takes `--agent`, and a call
  naming no supervisor is refused.
- **Every read names the supervisor too.** A turn reads its own supervisor's rows, and a reminder or
  watch wakes its own supervisor. Every room several supervisors share holds rows of each, and each
  sees only its own.
- **An added supervisor's gate options are made when it stands,** copied from an existing supervisor's as
  they are then, so the gates run for it from its first turn and are tuned apart afterwards.

## Long-term memory

- **Each supervisor's long-term memory is a brain of its own,** kept by the brain service and keyed by an
  account and an agent (the brain's design document, 「Whose a brain is」). Agency makes it as the supervisor
  is registered and files it under the id of the login account that owns the supervisor, else the key
  `AGENCY_BRAIN_ACCOUNT` names, and under the supervisor's registered name (`docs/api.md`, 「Calls agency
  makes」). The brain tool and the gates find it by the same two keys.
- **Everything a supervisor reads beside the conversation in front of it is a memory in its brain:**
  episodes, facts, situation rules, a room's rules, its knowledge, its persona and its lore, each a row
  with a type, and the people and rooms they are about as the brain's subjects. A person's sheet is
  rendered from the facts about them and is never a row of its own.
- **Two supervisors' brains share nothing,** and two accounts never see each other's. What several
  supervisors need to know is written into each one's brain by the one that learnt it; no mark makes one
  supervisor's memory another's. A run of the search or browse line writes into the brain of the
  supervisor it runs for (`docs/agent-roles.md`, 「The voyager and the seeker roles」).
- **The persona comes from the brain.** At a chat launch the supervisor's brain renders the layer named
  `persona`, and its placeholders are filled from the deployment's values (`docs/api.md`, 「Calls agency
  makes」); the lore and knowledge reach a turn only as the gates pick them.
- **A brain that cannot be reached leaves the supervisor registered** and the failure logged; with no
  brain address configured, no supervisor has a brain.

## Who writes long-term memory

- **Each supervisor writes its own,** in the turn itself and on its own model: as it reads each message, the live one that woke it and every
  archived message a turn hands it — what arrived while it was away, after a handover or on its
  return to a room — it writes what matters of them into its brain. The discipline its system prompt
  carries from the brain says so (`docs/api.md`, 「Calls agency makes」), and no other agent writes a
  supervisor's memory behind it.
- **It writes from its own first-person view,** into its own brain, so what it remembers is read back
  into its own turns as it wrote it, and no other model's judgement of what is fit to say reaches its
  turns through its memory.
- **A room several supervisors answer is read by each,** and each writes its own.
- **A supervisor on the registry's provider hands its own session over,** the session itself writing its state on its own
  model in one more turn once its context reaches the handoff mark (`docs/supervisors.md`, 「Its
  limit」).

## The console

- **The supervisor tab lists every supervisor,** each under its own label, so another supervisor
  stands there the moment its row does. The console reads them from agency's supervisor list
  (`docs/api.md`, 「supervisors」).
- **A supervisor's memories are read on the brain's own screen,** by the brain its two keys name.
- **Each supervisor's own screen shows its usage,** its week's tokens read from the usage its turns
  write (`web.md`, 「Agent screen」).
