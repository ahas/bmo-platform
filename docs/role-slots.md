# Role slots

A role's agents are its seats: each run of a role occupies one slot of it while it goes, let in on one agent
of the role, and a run that finds no agent free waits in the role's line until one frees. This document
states how a run asks for its slot, how the line orders the asks, how slots are reserved ahead, and the
standing branch checkouts a co-work branch works in. What a role is, is `docs/agent-roles.md`; what an agent
is and which one a run takes is `docs/role-agents.md`.

## Who takes a slot

- **Every run of a role takes a slot of that role before its program launches:** a job's run, a worker
  ask, a co-work order's builder stage, a co-work branch, a search's or a browse's run and
  its branches, a translation, and an upkeep run. Nothing runs a role's program outside a slot. A subagent a
  run's own program opens is part of that run and takes none (`docs/agent-roles.md`, 「Subagents」).
- **A supervisor's turn takes none.** A supervisor is no role; its turns are bounded by its one program
  (`docs/supervisors.md`, 「Where a supervisor's turn runs」).
- **A word's turn on a worker's own session takes none.** It is a turn spoken to a worker, not a run of
  work: it takes the checkout's agent lock, one turn at a time with a job's, and the worker-words clock
  (`docs/agent-control.md`, 「Words said to a worker」).
- **A slot is asked under a role the store holds,** and is refused 422 under any other: an unknown role is
  an error, never a run on some default.
- **A slot's source and owner are the caller's.** A slot is asked under the source label its caller sends —
  `job`, `ask`, `cowork`, a runner line's name for one the research unit asks, or any other — and under the
  owner and request key its caller names; the label is kept as sent, and no rule of the line reads it. A slot
  asked naming no role is refused 400, whatever its source.
- **A slot is let in on one agent of its role, and goes out on that agent's road** (`docs/role-agents.md`,
  「Which agent a run takes」): the first agent, by its place among the role's, that is switched on, is free
  (`docs/role-agents.md`, 「When an agent is busy」) and whose road can answer now. An ask may name a provider and a model together, or a model alone,
  whose provider is the one the role's first agent of that model is on; it is then let in only on an agent of
  that road and waits for one; an ask naming a road no agent of the role is on is let in on an agent that
  names no road, and is refused where the role holds none. The agent gives
  the effort and the thinking.
- **A slot of a role whose agents name no road goes out on the road of the supervisor that asks it,** the one
  its ask names as `askedBy` (「A role that follows its supervisor」 below). Asked under no supervisor the store holds, it is
  refused 422 with why, and nothing starts. In a role that holds agents of both kinds, a slot naming no road
  is let in on any free agent, on that agent's own road or, for an agent that names none,
  on its asking supervisor's (`docs/role-agents.md`, 「Which agent a run takes」).

## Accounts

- **A slot belongs to an account:** the account of the supervisor its ask names as `askedBy`, as that supervisor's
  row names its owner account, or none, the deployment's own, where no supervisor asked or the one that did belongs
  to none. It is kept on the request as `account`.
- **A slot is let in only on its account's agents,** counted against its account's cap — the number of its role's
  agents of that account that are switched on — and against its account's slots of the role that are let in or
  reserved. A slot of another account never stands in its way in the line, and never takes its seat.

## The cap

- **A role's cap is the most of its runs that are let in at work at once: the number of its agents that are
  switched on.** A run beyond it waits for a slot and none is refused, and none is let in beyond it. A job
  standing in the machine's turn keeps its
  seat and counts against no cap (`docs/role-agents.md`, 「When an agent is busy」), so its agent's next job is
  let in beside it.
- **The cap is the role's alone.** It is counted over every run of the role wherever it works, each run on
  an agent of its own; no path carries a limit of its own.
- **Runs do not take turns at a path.** A run occupies a slot of its role wherever it works; two runs at
  two paths, or two runs a caller sends to one folder, are two slots. What keeps two jobs out of one
  checkout is the repository's checkout, not the slot (`docs/worker-slots.md`).
- **The agents are read as every run asks to be let in,** so an agent added, removed or switched holds from
  the next admission: one more lets in at once the waiting run it frees, and one fewer takes no slot away
  from a run already let in.
- **A role with no agent switched on is closed:** its runs wait until the owner switches one on.

## The line

- **An ask goes into the slot table before the program launches.** One immediate transaction checks the
  role's active slots plus its reserved ones, who stands ahead of the asker, and whether the run's provider
  has quota (`docs/provider-order.md`, 「Whether a road can answer」), and grants a lease.
- **One role's asks are judged one at a time.** The transaction that judges an ask holds its role's row until
  it ends, so two asks woken at once — a slot freed with several waiting — never both read an agent as free
  and both take it: the second is judged once the first is written, and finds the agent carrying its run.
- **An ask can be judged without being made.** Whether a first ask would be let in now is answered by the
  same rule an ask is admitted by, for a request standing behind every one in the line that has not asked
  before, and nothing is written: no request, no place in the line, no number of an ask. A job is written
  with the wait this answers (`docs/api.md`, 「assignments and jobs」).
- **Within one role, waiting asks stand in the order they were asked,** by a global increasing sequence, and
  a slot goes to the first of them whose owner is asking now.
  - **An owner asks for its waiting request by asking for its admission,** again and again while it waits
    (`docs/api.md`, 「slots, reservations and places」). Every ask is numbered in one increasing order of
    the line's own, which counts no time.
  - **A request ahead of the asker stands in its way while its owner is asking:** a reservation's slot
    always does, and any other request does where its owner has asked since the asker's own last ask.
  - **A request whose owner is not asking keeps its place and is passed over, never dropped.** It leaves the
    line only by its token's release or cancel, or where the evidence below shows its owner ended.
  - **A request whose provider has no quota is passed over** while it has none, keeping its place, so a run
    waiting for quota never holds up a run of the same role that could go; it is next again once its
    provider has quota.
  - **A request that could not take the asker's agent is passed over,** keeping its place: one that names a
    road the agent does not stand on, one that names an agent by name and not this one, or one no supervisor
    asked where the agent names no road (`docs/role-agents.md`, 「Which agent a run takes」). A run waiting for
    an agent of its own road, or for one agent by name, never holds up a run that a free agent could take. A reservation's slot holds a seat by count
    and stands in the way whatever the agent.
- **Request keys:** the key a caller asks under names one request for good. An ask repeated with that key
  while its request stands — waiting, reserved or active — is answered with that request, unchanged, so a
  caller that lost an answer asks again safely. Once the request is given back, released or cancelled, the
  key is spent: an ask under it is refused 422 and opens nothing, and the caller's next run asks under a key
  of its own.
- **A place key:** a request may hold one, and only one active request per key exists. A job's run is keyed
  by its checkout, so one checkout carries one run at a time; a request blocked by its key does not block
  another request behind it.
- **A lease** preserves a hard-to-guess token, a stable owner and request key, the role, the path, the
  chosen provider, model and effort, the cap then, and the time. Changing a role's row does not change this
  snapshot for a request already waiting or let in.
- **Release and cancel** are done by the holder of the token, and by the owner for everything it holds
  (「An owner's release」 below), and finish once however many times called. They finish at once when
  success, failure, a launch refusal or a cancel is confirmed. Each given-back request keeps who gave it
  back: `holder`, the token's holder; `owner`, its owner's release; or `run-ended`, the line on evidence its
  run ended.
- **Giving back a lease bound to a role session ends that session,** whether its holder finishes it or its
  owner releases it: a turn running in the session is cut, ending `stopped` with the reason `released`
  (`docs/api.md`, 「sessions」), and the session is ended once that turn has ended. Nothing goes on running on
  a slot given back.
- **A lease is bound to what its run stands as:** a systemd unit, a herdr pane or an open execution by its
  name, or a role session by its id. A role's start that names the slot binds the lease to the session it
  opens, whether or not the lease was bound before (`docs/api.md`, 「sessions」), and a binding naming the id
  of a session agency holds is bound to that session.
- **A slot carries several sessions in turn.** Each start naming the slot moves its lease onto the session
  that start opens, and the session the lease was bound to before is bound to it no longer. A consumer that
  runs sessions one after another on one slot — a first answer, its rounds, a review — starts the next
  session on the slot before it ends the one before; ending the session the lease is still bound to gives
  the lease back.
- **A job's lease is the job's seat, held from its first turn's admission to the job's end.** A job's turns
  each start and end a session of their own, and its review waits between them with none running, so the
  lease is bound to no run: no turn's end, no pass of the line and no restart of agency gives it back, and it
  stands in the store across a restart as it stood before. Every later turn of the job — a stage handed over,
  a question or a rework of its review — goes out on that lease without asking again, so no request of
  another job, however long it has waited, is let in on the job's agent between the job's turns
  (`docs/role-agents.md`, 「Which agent a run takes」). Only while the job stands in the machine's turn —
  testing, waiting to test, deploying or waiting for its deployment — is another job let in on its agent
  beside it, the job keeping its lease all the same (`docs/role-agents.md`, 「When an agent is busy」).
- **A job's lease is given back when that job ends,** however it ended: its turn's end, its stop
  (`docs/api.md`, 「assignments and jobs」) or its reap as a job whose process went away. Every write of a
  job's end gives back every request asked under the job's owner, as that owner's release gives it back
  (「An owner's release」), with the reason the end carries, so a job that has ended never keeps a slot of its
  role.
- **The one pause that gives a job's lease back before its end is its review's,** waiting on the owner past
  its due time (`docs/job-review.md`, 「The owner's word」). Its agent is then free for other work, and before
  its next turn the job asks for a slot again, in the line as any ask stands there.
- **An admitted lease bound to nothing is given back only by its holder or its owner:** the line reads no end
  of a run that never named itself, so such a lease stands until its holder finishes it or its owner's release
  gives it back, however long that is.
- **A bound lease is given back only on an observed end, never for a span of time:**
  - a lease bound to a session, once that session ends while the lease is still bound to it: ended by its
    caller, by its holder's finish or by an owner's release, or gone with the agency process that held it,
    since a role session is held in memory from its start to its end and a restart ends every one. A
    session the lease has moved off ends without touching the lease;
  - a lease bound to a unit, a pane or an execution, once the line's pass has seen that run standing and then
    sees no systemd unit, herdr pane, open execution or turn kept in flight under its name holding it. A run
    not yet seen standing is never read as ended, however long it takes to come up. The pass reads from the
    server's start and each minute after; it is a reading, not a bound on any lease;
  - a lease bound to a turn the process before left open, its program still going, which the server that
    starts next adopts (`docs/sessions.md`, 「A restart hands the session over」): the turn stands as seen the
    moment it is adopted, so its end, or its program found gone, gives the lease back;
  - the token holder's release, or the owner's, the end of the job whose run holds it among them.
- **An active lease is never taken away for its age:** only these ends give one back.
- **An admission answers what the request is now:** `waiting` while it waits in the line, `active` once let
  in, and `released` or `cancelled` once given back, with who gave it back and why. A caller asks again only
  while it waits; a request given back by anyone stays given back.
- **Every change of a request is published** as `slot-request.changed` (`docs/api.md`, 「Events」): its token,
  source and state, with the fields of its lease as far as they are set.

## An owner's release

- **An owner gives back everything it holds in one ask** (`docs/api.md`, 「slots, reservations and places」):
  every request it asked that still waits, stands reserved or is let in — a waiting or reserved one
  cancelled, an active one released — every reservation it holds, and every role session bound to one of its
  leases, ended, a turn running in it cut first (「The line」). The ask is answered once the requests and
  reservations are given back; the sessions' ending is carried on by agency after the answer, so a consumer
  asking at its start is never held on a program's turn that is slow to stop. A consumer asks it as it starts
  and as its run ends, so it keeps no list of its own holds to give back, and a lease let in whose run never
  started a session is given back with the rest.
- **The release is the owner's word, read as given:** agency judges nothing of why, and a request asked under
  another owner is not touched.

## Prior and yielding requests

- **A request is a prior request or a yielding request.** The caller decides which; the line does not read
  the reason. Every request now asked is a prior request; yielding is a rule the line holds for work that
  can defer behind another's.
- **A yielding request enters only** where no prior request of the same role stands in its way and the
  role has a free slot; an already-active yielding request is not taken away. A waiting yielding request
  may be raised to a prior one, keeping its sequence.

## Reserving slots

- **Whoever will soon launch runs of a role reserves slots of it:** "this owner will soon use so many slots
  of this role". A co-work runner reserves, while a stage runs, the slots the next stage's branches will
  use.
- **Each reserved slot is a request carrying its reservation's token,** queued in the role's line, and
  reserved — not active — when its turn comes and the cap has room. The cap counts active and reserved
  slots together, so a reserved slot cannot be taken by another request, and the owner's actual request
  takes it over in one transaction without queuing again.
- **Reserving again with the same key** only changes the count: raising queues the missing slots, lowering
  releases the latest-queued first.
- **A reservation releases only** when its owner releases it or there is evidence the owner ended — the
  co-work runner by its order record, the research unit by its own run — never for time passing.
- **A reservation holds slots and warms nothing.** No role is warmed: a run's prefix is read again by the
  next run while it is still cached, and nothing pings it in between (`docs/keepalive-schedule.md`).
- **The line's own pass** runs when a slot frees, when a cap changes, and every five seconds, raising to
  reserved every reserved slot no request stands in the way of while there is room.

## A role that follows its supervisor

- **A role whose agents name no road follows the supervisor that asks it** — the `coder`, as seeded. A slot of
  it asked naming no road carries the provider, model and effort of the supervisor its ask names as
  `askedBy`, as they stand when it is asked — the supervisor's one road — and waits, never moves, while that
  road cannot take it.
- **Every turn of a run on such a slot goes out on that supervisor's entry,** on whatever provider's road the
  run goes: a session started on the slot carries the supervisor its `askedBy` names, and the model, effort,
  thinking and answer room each of its turns takes are that supervisor's for the road, on Claude Code and on a
  provider's program standing in a place of its own alike.
- **A slot of such a role that no supervisor asks is refused** 422 with why, and nothing starts, since it has
  no road to go out on.
- **An agent the owner sets a road goes out on that road instead,** like any role's agent; in a role holding
  agents of both kinds, only a slot let in on an agent that names none follows its asking supervisor.
- **An agent that follows its asker while naming a road of its own** goes out on its asking supervisor's road
  where one asked and that road can answer now, and on its own road otherwise, a slot no supervisor asks
  included (`docs/role-agents.md`, 「An agent that follows its asker」).

## A co-work branch

- **A plan holds as many stages, and a stage as many tasks, as its design asks for.** How wide a stage is
  belongs to the design, and how many run at once belongs to each task's role. Tasks a slot is not free for
  remain pending, having started neither the model process nor any clock; when one ends and releases its
  lease, the next tick launches the front task of the same role.
- **A branch's provider is its role's agent's,** never its parent's: the dispatcher takes the role a task
  names and is let in on one of that role's agents.
- **Inside a branch, a provider that fails does not cross straight to another:** the branch is closed as
  failed, its lease released, and a fresh request taken for the same role, which goes out on the role's next
  provider as a separate attempt.

## Standing branch checkouts and their list

- **The places branches work in** are standing checkouts under a runtime directory keyed by the pool and the
  place, and one place takes one task at a time. A standing checkout is only a place to reuse a prompt
  cache, never a slot: the list below and the role's line do not stand in for each other.
- **Which places are taken is the consumer's to say.** A consumer asking for a place, or ending an order,
  hands in the places its tasks sit in now, and agency counts those as busy; it reads none of the consumer's
  orders or jobs. The ask names the order's checkout, the task's selection and its role, and is answered
  with the place's number and its folder.
- **A branch's turn reaches its place as a run whose folder is the place's,** naming the task's item and role
  (`docs/api.md`, 「sessions」).
- **A pool is one repository:** the order checkout's git common directory sets it, and its name is that
  repository's name plus a short hash of the common directory path. Places stand only as that repository's
  worktrees; place numbers, busy places, list lines and clearing are counted within it.
  - A place numbered directly under the root, not under a pool, belongs to no pool and is cleared when
    picked.
  - A pool nothing can reach any more — none of the repositories' checkouts names it — is swept on the same
    pass: each place is taken off its repository, cleared and its lines deleted, and the pool's directory
    goes once nothing of it is left. A place a task still sits in keeps its pool until that task ends.
- **On taking work,** the place is rewound to the order's HEAD — tracked files rewound, untracked files
  deleted, ignored files kept, submodules forced onto the HEAD's pins — and the order checkout's uncommitted
  changes and new files are laid on top. A place whose repository differs or cannot be read is detached and
  remade; a place registered but gone is remade.
- **A branch's run in a place is a run like any other:** a pane of its own labelled `#<item> <role> <nn>`,
  closed with the run. A place keeps no program between branches.

### Why a place is reused: the prefix

- The prompt cache attaches only to a prefix with not one character different, and that prefix holds the
  absolute path of the place the checkout's own instructions were read from. Reusing the same place turns
  that part of the prefix into a read; what a provider holds is the prefix, so the next branch that takes
  the place opens a new session yet does not rewrite it.

### The list

- **One line per pool, place, provider and model,** holding the provider, the role that last answered
  there, the last answer time and the prefix fingerprint then; an OpenRouter line holds the answering
  upstream too.
- **Liveness** is the provider's declared cache lifetime for that model (for OpenRouter, the answering
  upstream's) less one margin subtracted in one place (`policy/pool-warm.toml`). A line past its lifetime
  is deleted, and so is a Claude Code line whose fingerprint differs from what the place would now open; a
  Codex or OpenRouter line has no fingerprint and dies by clock alone, and a provider announcing no lifetime
  leaves no line.
- **When a stage launches several branches of one role,** the list answers which places to use: an empty
  place within its lifetime and with a matching fingerprint first, the shortfall opened as cooled or new
  places, up to the most branches one stage launches at once.

## On a registration the arbiter stands

- **A slot on a registration whose engine the arbiter stands** asks its demand once the line lets it in
  (`docs/own-llm.md`, 「An engine the arbiter stands」). The role's agents are the ceiling and the master's
  grants are what run: a run waits for its grant as it waits for its slot, and gives its demand back with
  its lease. A slot asked from a run on such a registration asks within that run's demand, and a slot whose
  caller hands in a granted demand of its own stands on that demand and asks none. A session started on a
  slot stands on the demand its lease holds, its turns asking none of their own. A slot's demand is asked at the
  rank and in the pool its admission names, else at the rank of the supervisor that asked the slot.
