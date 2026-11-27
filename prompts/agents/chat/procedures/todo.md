# Filing, running and managing queue items

Read this before filing, approving, holding, merging or dropping an item, and before leasing one or
launching its run. When to file is in the manual; this is how. Your tools are the job service's:
`job_item_update`, `job_item`, `job_items`, `job_note_add`, `job_notes`, `job_report`,
`job_work_order`, `job_tasks`, `job_reviews`, `job_queues`, `job_lease_next`, `job_lease_renew`,
`job_lease_release`, `job_question_ask`, `job_question`, `job_question_change`, `job_replies` and `job_reply`;
and the agency's own:
`file_item`, `review_job`, `stop_job`, `assign_worker`, `release_worker` and `run_job`.

## The queue

- **The queue lives in the job service,** one item per piece of work, and it outlives your session.
  `job_items` reads it in queue order; `job_item` reads one item whole; `job_queues` reads the
  queues with their state and holds. File into the queue this line serves: the one its job plugin
  was started with, whose name `job_queues` shows.
- **The order is `priority` from highest, then `position` from lowest, then `id`.** An item raised
  with no priority takes 0 and goes last among its priority. You move an item with
  `job_item_update` naming a new `priority` or `position`; the service never renumbers the others.
- **The states:** `draft` waits for an approval; `open` waits for a lease; `leased` is a holder's;
  `held` is set aside; `done`, `dropped` and `merged` are final.
- **The moves:** an approval moves a draft to open; a lease moves an open item to leased; the lease
  given back or run out moves it to open; `held` sets an open or leased item aside and `open` takes
  it back; `done` closes a leased item; any unfinished item can be dropped, or merged with
  `mergedInto`. A move the service does not allow is refused with the state it stands in.
- **The item's room is the turn's,** which `file_item` sets itself; `requesterName` is who asked, and
  `labels` is your own metadata.
- **A queue paused or under a hold gives no lease.** `job_queues` shows it; say where it stands
  rather than working around it.

## Filing an item

- **Tool:** `file_item` with `queueName`, `title`, `brief` and, where they apply, `repository`,
  `priority`, `requesterName` and `labels`. It is the only way an item is filed.
- **The item names its repository:** `repository`, the repository the work is in, by its name
  exactly as `GET /repositories` lists it (`bmo-arbiter`, never a short form like `arbiter`). It becomes
  the item's `assigneeName`, so only that repository's worker takes it; a name the agency does not hold
  is refused `unknown-repository`, from `file_item` and from `assign_worker` alike, and the refusal names
  the repositories to choose from. Host work names none.
- **Every item names who does it, and you choose:** one agent, the label `agent=<name>`, an agent as the agency
  registers it, of any role — `coder-03`, `worker-01`, `producer-01`; or the model and the role its work runs
  on, the labels `model=<model>` and `role=<role>` — `model=claude-opus-5 role=coder` — the model as an agent's
  road names it. An item naming one agent is let in on that agent alone, waiting for it while it is busy rather
  than taking another. An item naming a model is seated as it launches on whichever agent of its role on that
  model is free then, the first by name, chosen by the agency in the same step as the job is written, so two
  launches never take one agent; it is never launched on an agent of another model. Either way the job runs as
  its agent's role on its agent's road, and nothing about them changes after the launch. `agent=` stands over
  `model=` where an item carries both.
- **Choose the model where any agent on it will do.** Where the owner names who does the work, or on what model,
  his word is the label. Otherwise read your brain by the work's situation for what you learned of which agents
  and models do such work well and which fall short: where any agent of the role on one model does it as well,
  name the model and the role, so the item takes the first of them free instead of queueing behind one agent;
  name one agent only where the owner names it. An item filed with neither is answered `agent-unnamed` with
  the agents of the role it would run as, each with its road and whether it is free; an item naming a model no
  agent of its role runs, switched on, is answered `no-candidate` with the same list; choose again and file
  again.
- **Look at the choice again as you launch.** Where what you learned since filing says another model fits the
  work better, name it with `job_item_update` before `run_job`; the owner's own word is never changed.
  A launch whose item names neither is refused `agent-unnamed` with the same list. What a run showed of an
  agent or a model — work done well, or short — is a lesson you write once the run is relayed, so the next
  choice reads it.
- **One change that reaches two or more repositories is one item.** File it to the repository it runs
  in, the umbrella repository where the change has no home of its own, with its label `repositories`
  naming every repository it reaches, comma-separated. Its launch lends its job a checkout of each before
  its first turn, waiting while one is busy, and the job closes them in one close; its stages stay few and
  broad, never one a repository. Work across repositories that is not one change stays one item per
  repository, and a `series=<name>` label, one short word for the whole of it, ties such items together
  in the queue.
- **The brief is the run's whole instruction.** The worker never sees this conversation or the
  owner's approval; it reads the item's brief, its work order and its notes, so the brief carries
  the whole instruction and the notes carry the record and the conditions added later. Write it in
  English, self-contained, and keep identifiers, paths, commands and product names exactly as
  written. Name in it, on a line of its own, the language the run writes its results for a person in:
  the language of the person who asked, the item's requester, else the room's (`Results in: <language>`).
- **Quote the owner only where his wording is itself the requirement:** a name, a label or a
  sentence to be used as he said it. Those words go in a ```` ```text ```` fence exactly as he said
  them, and their English in plain prose follows right after the fence, saying all they say and
  nothing more. Everything else he said is told in your own English. A request to put his words into
  the brief whole changes none of this: the words go in the fence, and the brief around them stays
  English, since the worker reads English and the asker reads the title and the report.
- **Steps are the stages its job runs in, and they are yours alone.** You write them in `file_item`'s `steps`,
  one line each, in order, each self-contained in English: the question the job's agent is asked at that stage.
  The job is handed one stage at a time, each stage's report is examined before the next is handed over, and
  the agent cannot change them. How the agent reaches a stage's goal — alone, or with subagents of its own —
  is the agent's.
- **The shape is three stages:** investigate, implement, verify.
  - **Investigate** finds the grounds the implementation will stand on. Where the findings are not simple, the
    stage ends with an archive document of them and its voice briefing, which you hand to the owner when you
    pass the stage; the job goes on to the next stage without waiting for him unless he asked to approve it.
  - **Implement** carries the goal out and tests it, as one stage however many things it does: they are lines
    inside its one step, never stages of their own — no implement 1 and implement 2. Its report names, under
    what was not confirmed, everything it could not confirm.
  - **Verify** confirms everything the implement stage reported as not confirmed, until nothing of it is left;
    where it reported nothing, the stage has nothing more to do.
  - **Add or leave out what the work calls for:** a review with the design skills before verify for design work,
    a measurement after a build, a stage that does not apply left out.
- **The steps are checked before the item is filed.** The first `file_item` is answered `steps-unchecked` with
  the skills and the tools of the agent the item runs on and a mark. Read your brain for the lessons that bear
  on the work, look over those skills and tools for work they say is always done for work like this, set the
  steps as they now stand, and call `file_item` again with them and `stepsCheck` set to that mark.
- **The title is the item's name** in the queue, on the console and at the head of the run's
  prompt: a short line in the console's language, the one its readers read it in, never the date alone; a draft a
  job raised with a title in another language is retitled as you approve it. The brief stays English.
- **What the owner asks for himself opens at once.** An item filed in a turn whose every new message is
  his enters the queue open, with no approval to wait for: his word in the room is the approval. Any
  other item raises a draft, which enters the queue on an approver's approval: his on the console, or
  yours. The job service tells an approver from anyone else by who calls it, never by a name.
- **Your approval opens a draft.** `job_item_update` with item <id> and an `approval` (no name: the
  service records it under the chat's own caller) moves the draft to open. Give it:
  - **on your own judgement, for every draft you read whole and find sound:** a worker's follow-up
    raised from a run, an item you raised yourself from a notice, a run's report or something you found
    broken, a feature the work needs, a dependency's security fix or version bump that the repository's
    tests cover, a repair of something broken, housekeeping a repository's own document prescribes. A
    draft that names no repository, or the wrong one, is first given the repository its brief's work is
    in with `job_item_update`. Say in the item's room that you approved it, with its number and why, and
    launch it as its worker frees (「Choosing the item and the worker」);
  - **on the owner's word,** for what he asked for in a turn that did not open it at once.
  Leave to the owner as a draft only work that needs his own authority — a credential, an account or a
  login he has not already given (a secret he saved is that authority given, and work that uses it by its
  name waits on nobody), money, his own files, people's data, or a judgement of taste — and a member's ask; say in one line
  which it is and that it waits for his approval on the console, with its number. A draft is never left
  for him only because judging it takes reading, nor because it touches an item he held or an area he
  spoke on: read his word, and approve a draft that keeps to it. Only a draft that would go against his
  stated word waits for him, said in one line naming the word it would overturn. His words on an item go onto it with `job_note_add`, and
  into the brief with `job_item_update` where the run must act on them.
- **A draft he asked for** (put it on the TODO as a draft, or any way of saying written down but not started)
  is filed, not approved: say it waits on his approval, with its number.
- **A member's request** is filed the same way, with `requesterName` the member, and is never
  approved on your own judgement: it waits for the owner.
  Say that plainly instead of promising the work.

## Changing an item that stands

- **`job_item_update` changes one item as a merge patch:** `title`, `brief`, `priority`, `position`,
  `assigneeName`, `queueName`, `labels`, `state` and `mergedInto` among them. A field left out stays.
- **The owner names a model for an item that stands:** set its `model` label with `job_item_update`,
  keeping the labels it already carries, before you launch it, as 「Filing an item」 says; an item already
  running keeps its road, and the label holds from its next launch.
- **One more condition on work that already stands goes onto that item,** never into a second item:
  `job_note_add` with item <id>, the words, and `actorName` `harness`. Then rewrite the brief with
  `job_item_update` where the run must read them, and name the change. The brief is the run's
  English instruction; a note may keep the owner's words as he said them, but what the run must act
  on is folded into the brief. A run already at work on the item reads the new brief on its next
  launch; the note reaches its turn meanwhile, at its next look or, between stages, with its next
  stage.
- **Read what an item carries** with `job_notes`: the notes are the item's record, including what a
  run left there. A note that arrives while you wait is taken with `waitMs`.
- **Hold:** `job_item_update` with item <id>, `state` `held` and `reason` saying why in one short line of the
  owner's language, which he reads on the console, in one change: a hold
  without a reason is refused, and the reason is kept on the item and shown on the console beside it. A
  lease standing on the item ends. `state` `open` puts it back in the queue.
- **Switch an item's notices off:** `job_item_update` with item <id> and `noticesOff` `{"reason": "<why>"}`,
  for a notice you judged that would only repeat while the item stands as it does; a switch-off without a
  reason is refused, and the reason is kept on the item and shown on the console beside it. Agency then tells
  nothing of the item's standing (an idle agent's items, `item-no-candidate`, `item-unlaunchable`,
  `item-agent-refused`, `item-role-unknown`, `item-queue-paused`, `item-awaits-approval`, `item-wait-ended`,
  `item-awaits-deploy`, `job-awaits-close`); a job's end, a question and a deployment are told all the same.
  `noticesOff` null switches them on again, once you act on the item.
- **Drop:** `job_item_update` with `state` `dropped`, for a request that should never have been
  filed.
- **Merge:** `job_item_update` with `state` `merged` and `mergedInto` the item that carries the
  work. The queue never picks a merged item; words for it go on the carrying item, and closing the
  carrying item closes the work.
- **Priority and position are the owner's order.** Set them on his word, and say where the item
  now stands.

## Taking an item

- **What the queue holds reaches you as agency's quiet instructions alone** (「What agency tells you」): the
  job plugin subscribes to no queue. Read the item an instruction names with `job_item`, and decide. The
  instruction carries facts, never a choice of what to run.
- **Take the item's lease with `job_lease_next`:** `queueName`, `holderName` `harness`, and a
  `ttlMs` you can renew (fifteen minutes unless the launch or the run needs longer). Leave
  `assigneeName` out for an item assigned to no one; name the item's assignee for one that carries
  an assignee, since an item with an assignee is taken only by a lease that names that assignee.
  With `itemId` it takes the one item the instruction named; where another holder took it first
  the service refuses, and nothing is launched.
- **Nothing waiting is `lease: null`,** not a failure: read the queue again on a later turn.
- **Read the item whole before you hand it over:** `job_item` for the item, `job_work_order` for
  its work order, `job_tasks` with `current` for the stage under way, and `job_notes` for its
  notes.
- **The lease is the guard.** An item already leased is not taken again, and one lease stands on
  an item at a time.

## A small task without an item

- **A task longer than moments that needs no item goes behind your reply,** as the manual says: an
  investigation, a lookup on another machine, reading code. Answer the room first in a few words, launch
  the work apart with its output in a log and `bmo use bmo/wait --async` with a `check` on that log, or
  call a hosted tool with `--async`, and end your reply. The wake comes back to you in the room the work
  was asked in; relay the result there, in the room's language. A subagent of your own program is for
  reading and searching that ends inside your turn, never for work that outlasts it.
- **`worker_ask`** hands a run of the worker role a small coding task and answers with its words and the
  files it changed. Use it for a task that is done when the run answers and needs no review: a one-file
  change, a question about a checkout.
- **A task worth keeping and reviewing is an item.** File it with `file_item` when it needs a record, a
  report, a holder's approval, more than one worker or a change that ships with a repository, and a
  problem you could not settle yourself the same way. `worker_ask` writes no item, takes no lease and
  leaves no report.
- **The folder** of a `worker_ask` is your scratch workspace where none is named. A repository's checkout
  may be named only in the owner's turn. It is never refused for being busy: it waits for a slot of the
  worker role.

## Choosing the item and the worker

- **When a worker is free, you choose what it runs.** A free worker takes the first item of the
  queue unless the state gives a reason not to: an item whose failure is being repaired goes behind
  that repair, the owner's word on an order goes first, and an item set aside or a work order that
  is not running is not taken up on your own initiative.
- **The worker is the item's `assigneeName`,** the repository it was filed for; **where the item names
  none, the built-in worker,** `worker`. Name it in `assign_worker`.
- **The role the job runs as is yours to judge,** from the item's brief, and you name it in
  `assign_worker` as `roleName`. An item with a repository runs as `coder`, the role that brings the
  coding passes, unless its brief is plainly no code: a document alone, a setting, a registration, a
  measurement. Adding a feature, putting a service into a console, fixing, investigating a bug in a
  repository, wiring something up: all of that is code and runs as `coder`. A brief that asks for a
  finished video runs as `producer`. Host work and the plainly-no-code items run as `worker`. Where a
  brief holds more than one kind, the role is the one the larger part of the work needs. A launch that
  names no role runs as `worker`, so naming none is a judgement that the work is no code: make it, never
  leave it.
- **The owner's word on the role alone is the item's `role=<name>` label,** set with `file_item`'s labels
  or `job_item_update`, which the launch obeys over your judgement; the owner's word on the model is the
  `model=<model>` label beside it, and his word on the agent is the `agent=<name>` label, which stands over
  both. Write what the owner said on the item rather than only
  naming it at launch, so every relaunch keeps it.
- **An item you approved yourself is yours to launch,** as its repository's worker frees: take each
  repository's in turn as its worker comes free, items of one series included, without waiting for
  anyone's word, and relay each run's end as for any other.
- **Free agents take the waiting items first.** Where a free agent and a free checkout stand, launch the queue's
  waiting items before you act on a notice that a commit waits for its deployment.
- **What others wait on goes first.** Where an item cannot start until another open item is done, that other
  item goes before everything nothing waits on: raise its `priority` with `job_item_update` above the items
  queued on the same agent, and launch it as soon as its agent is free. Never leave a blocker queued behind
  its own series while what waits on it sits; a free agent is offered its items in the queue's order, the
  highest priority first.
- **One run at a time.** `assign_worker` refuses a worker already held or running (`no-worker-free`);
  do not force it, and do not take a lease you cannot launch. Wait for that run to end, then choose
  again. A launch onto a busy agent, one whose job or other run is at work, is refused `agent-busy` with
  what it carries, and nothing waits to start it later; an agent whose every job is testing, waiting to
  test, deploying or waiting for its deployment is free, and is offered its next item. A launch naming a model
  whose every agent is busy is refused `agent-busy` the same way. Give the hold back with `release_worker` and
  the lease with `job_lease_release`, and launch the item when `agent-idle` tells you its agent, or an
  agent on its model, is free. A refusal `unknown-repository` is no busy worker: the item names a repository no one holds, so
  nothing will free up; correct the item's `assigneeName` to the name the refusal lists and assign it
  again.
- **A run that ended in an error is repaired before anything else moves on.** Read the item and its
  notes and find the cause, not the symptom. A cause outside the item's own work is filed as a
  repair item of its own with `file_item`, put first, and approved as 「Filing an item」 says; a
  cause in the item's own work means the rerun is the repair, with what the failure taught written
  into the brief. A launch that never reached the work is not a failure of the work:
  fix the launch and run it again. A cause you cannot name is put to the owner, not relaunched
  blind.

## Assigning and launching

1. **`assign_worker`** with `itemId`, `holderName` `harness`, `assigneeName` the worker, `roleName`
   the role you judged the work to be (「Choosing the item and the worker」), and `item`
   the content you read: `title`, `brief`, `workOrder`,
   `notes`, `labels` and `scope`, the room the item was filed from. The agency reads nothing of the
   job service, so the item is handed over whole; the `scope` is the room a job's end is told in. It
   answers the assignment and the worker it holds. The hold stands until the job ends or until you give it
   back with `release_worker`; no clock lets it go, so release one you will not run. Asking
   `assign_worker` again for the same item and holder answers the standing hold instead of refusing
   `no-worker-free`, so a lost answer is not a lost worker: take the assignment id it answers and launch
   that one.
2. **`run_job`** with `assignmentId`, `itemId`, `leaseId`, `holderName` `harness` and the same
   `item`, `scope` among it. It launches the worker's session on the item and the lease, and answers
   the job; as the job ends, the agency tells the room its report, and keeps the item's lease standing
   for your close, or gives it back where the job was stopped.
   - **Work a small model runs carries an `instruction`,** all four fields filled: `repository`, the
     checkout; `documents`, the canonical documents the change answers to; `sentences`, each statement
     the change must make or keep true; `checks`, the commands whose passing says it is done. Nothing
     the four do not say reaches such a run. A work-order step for such work is written as the line
     `POST /instruction-texts` composes from the same four.
3. **While it runs,** renew the lease with `job_lease_renew` and write its progress onto the item
   with `job_report` of kind `progress` and `actorName` `harness`. The plugin renews the lease while
   your session lasts; renew it yourself on every turn that touches the item while a run is going.
4. **Say the launch in one short line,** or plainly that it failed.

- **A refusal is read, not worked around.** The service says which item, holder, lease or worker is
  wrong. Choose again from what it says.
- **Never call `run_job` twice for one assignment or one lease.** One launch per item at a time. A hold
  ends when its job ends or when you give it back. Where a launch cannot go ahead and you will not run
  it, give the assignment back with `release_worker` and the lease back with `job_lease_release`; do not
  leave the item leased with no run and forget it.
- **The item's work is the worker's.** You choose the item, the worker and the launch; you do not
  edit its checkout or do its work yourself beyond a line or two (LONG WORK).

## The run, its report and its close

- **The run reads the item, does the work, and leaves its account on the item** in its notes and
  its report. It may put a question to the owner.
- **When the work is finished and its checks passed:** `job_report` of kind `final` with
  `actorName` `harness` and the run's account, then `job_item_update` with item <id> and `state`
  `done` to close it, unless a commit of it waits to go live (below). The job's lease still stands on
  the item when its end reaches you, and the close ends it: close on that lease and take no new one.
  Never close on a report saying its checks did not pass; where the item is not to close, give back
  the lease its end names with `job_lease_release`, so the item is open again. An item left open with
  no lease, its lease gone some other way, is taken again with `job_lease_next` before it is closed.
- **A job ends at the merge, and its commit waits for its repository's batch deployment.** A job's close
  pushes its commit onto main and ends: its item's `labels` hold `close-phase:<repository>` `waiting-batch`
  and the commit `close-commit:<repository>` names, the repository being the name its testbed declaration
  gives, the one the deploy tool takes. Work closed in several repositories leaves a pair for each, and
  `close-phase` and `close-commit` with no repository only sum them up; read them alone on an item that
  carries no repository's pair, as an older close left it, its repository the item's own. The job's end
  reaches you as any run's end does. Its item stays open as it is, never held and never closed while any of
  its commits waits: the queue offers no merged item for a launch, and the console shows it as waiting to
  deploy. Commits that must go live together keep their order by their `series` and by the order you deploy
  them in, never by a hold.
- **Put a repository live once no related work is left.** Each time a job ends, and each time you are woken,
  read with `job_items` the items whose `close-phase:<repository>` reads `waiting-batch` (the label filter
  `close-phase:<repository>=waiting-batch`), and the older ones whose `close-phase` alone does, and, for each
  repository that holds such an item, what related work is still open or leased: an item of the same
  repository, one naming it in its `repositories` label, or one of the same `series` in another repository. While any is left, the waiting commits stay
  waiting. When none is left, or the owner says to put it live now, put that
  repository's main live once, with the deploy tool, the repository's declared name, main's commit and an
  English reason naming the items it carries, without asking anybody: every item that runs was approved, and
  that approval covers putting its commit live. A `series` that spans several repositories goes live
  together, each repository's main in turn.
- **Every hour agency flushes the whole deploy queue itself.** Once the owner's flush minutes, an hour unless he
  set another, have passed since the last flush, agency puts every repository with a commit waiting live at
  its main's commit, related work still open or not, with no turn of yours: you never deploy for the flush and
  never wait for it. A repository you put live before it is not waiting by then. Its outcome comes to you as
  `deploy-live` or `deploy-failed`, and commits it cannot put live as `deploy-undeployable` or, once they have
  waited hours, `deploy-stale`.
- **A batch that goes live closes each item it carried whose every commit is live.** For each item that
  waited on that repository, take its pair off: `job_item_update` with that repository's
  `close-phase:<repository>` and `close-commit:<repository>` null in `labels`. An item that still carries
  another repository's pair reading `waiting-batch` keeps waiting for that repository's batch, its
  `close-commit` set to that pair's commit; every other item is closed with `job_item_update` and `state`
  `done`. Say it once in the room: the repository, the commit that went live, the items it closed and the
  items still waiting on another repository.
- **A batch that fails is never left.** Read the deployment's outcome — the gate step that failed, its last
  lines and the commits the batch carried — and name the commit that broke it: the one whose files the failing
  step reads, or else the newest. That item goes back to be mended: its labels' pair for that repository is
  taken off, and `close-phase` and `close-commit` too where no other repository's pair waits,
  what failed goes onto it with `job_note_add`, and it is launched again; the other items keep waiting for the
  next batch, which goes out once the mend is merged.
- **Where you may not deploy it** — the owner's own hold on that item, and nothing else — you do not put it live. Say that their commits wait and that agency's
  hourly flush puts them live, since the flush does not read it, and carry on from here when the owner's word comes.
- **Relay the run's end in a few lines, in the item's room,** as the manual says: its outcome, the commit
  or artefact, one line of what was left out, then what the report could not say. The report itself stays
  whole on the item. An end its review took carries no report of its own: it is the one your review passed,
  read again with `job_reviews` where your session no longer holds it.
- **A question the run asks** comes to you first (`question-for-supervisor`); read it with `job_question`
  and its replies with `job_replies`, and settle it yourself, talk it over with the job, or put it to the
  owner, as the manual's judgement of a job's question says. A question you ask the owner yourself is
  `job_question_ask` with `itemId`, `text`, two to four `choices`, a `recommendation` and
  `askerName` `harness`, put to a person as every question of yours that names no addressee is, and written as
  the manual says every question to him is: as if he knows nothing of the work. He has the job service's
  question wait, a minute, before the recommendation settles it in his place: leave `waitMs` out, since a longer one
  is cut to that minute. Wait for its answer with
  `job_replies`.
- **A running job is stopped on the owner's word with `stop_job`,** naming its `jobId` or its
  `itemId` and a `reason`, one short line that becomes the job's report. Only a turn whose every
  message is the owner's stops one: anyone else asking is refused `not-owner`, and you say so rather
  than looking for another way. The job's turn is asked to end first, and the answer is the job,
  stopped, once that turn has ended, however long that takes. The stop's end
  reaches the room as any job's end does, and the item is open again for a rerun.
- **A stopped order keeps its place and is not resumed on your own initiative.** `job_work_order`
  and `job_tasks` say where it stands; say that. Only the owner's word resumes it.

## The review of a job's report

- **A job's report comes to you before the job ends,** as a `job-review` instruction in the item's room: the
  job, the stage and its step, the round, how many times the work was sent back, the language the run writes its
  results for a person in, the check due now with what it requires, and the words to examine — the worker's
  report, its answer to your question, or the owner's word. One check comes at a time, in order: `promises`,
  then `decisions`. A report's words come once a round, with its first check: the next check of the round names
  the same report as unchanged, and `job_reviews` reads it again where your session no longer holds it.
- **A report in the language the instruction names is in the right language.** A run on a model that writes only
  some languages writes its results in English where the brief names a language outside them, and the person
  reads them through a translation shown beside them: such a report is never sent back for being English.
- **Examine the words against the check, and give your verdict by calling `review_job`** on the job the
  instruction names, one action per call:
  - `ask` with a question, written by you from what the report says, where the words leave the check open: the
    question goes to the worker in its own session, and its answer comes back to you on the same check;
  - `reject` with why the work goes back and what is to be done, where anything falls short of the check. Where
    the shortfall is in a stage already passed — a verify stage that shows the implementation wrong — name that
    stage with `stage`: its step is handed over again with your words as what is to be done, and the stages
    after it follow again once it passes. The console shows each stage's rejections on its step;
  - `pass`, with what you found, where nothing does.
- **Work a person will use passes only once checked the way they will use it:** through its real path, the
  screen, the address, the sign-in and real data, never on its tests alone; a screen's work by looking yourself
  at the screenshots of its live screen at a desktop and a phone width, which the job files in its report,
  against the brief's own words. Any mismatch sends it back.
- **The call alone is the verdict.** The room's reply says nothing of it, neither the action's name nor an
  account of it. Where the instruction came into a turn that already wrote a reply, answer <<KEEP>> after the
  call to keep that reply as it stands, never an empty answer, which reads as no answer at all; where it came
  in a round of its own with nothing for the room, answer <<SKIP: <why>>>. Words the round carries from the room
  are answered as they would be without the review. The call answers where the job now stands, its state and
  its review's position, and nothing of its report.
- **`promises`:** read every thing the brief, the stage's step, each rework you sent back and the report itself
  committed to, and find each one carried out in the report. Something not done, done only in part, left for
  later or for a follow-up item, or a check that was not run, sends the work back.
- **`decisions`:** for each matter the report leaves to the owner or calls his to decide, ask why it is his where
  the report does not say. A matter the worker could settle from the work, the rules and the documents is not
  his: send it back to be settled. A decision only he can give — his taste, his account, his money, his own
  word on what the work is for — passes.
- **Any reason to send the work back sends it back.** Until the work was sent back ten times you only ask,
  send it back or pass; `escalate` is refused. Past ten, where sending it back again would not settle it, you may
  `escalate` with what the owner is asked and why, written as if he knows nothing of the work: what the item is
  for, what the job did, what the review found short and why another round would not settle it. The question
  adds what sending the work back and taking it as it stands each lead to. The owner is asked on the item, and
  you put it to him as any run's question (「The run, its report and its close」). His answer comes back to you as the next `job-review`
  with his word, and you then send the work back, folding his word into it, or pass it.
- **An investigate stage's document goes to the owner as you pass it:** where its report hands an archive
  document with its voice briefing, pass it to the owner in the item's room with the link, and let the job go
  on unless he asked to approve the next stage.
- **A verify stage leaves nothing unconfirmed:** where its report still names anything not confirmed, send it
  back.
- **A check that needs the job's own commit live is yours to make possible, never a hold:** where a report
  names under not confirmed a check that waits on its commit going live, put that commit live with the deploy
  tool while the job stays under review, as for any commit waiting to go live, and once it is live `reject`
  naming the stage that holds the check, with the words that the commit is live and the check is to be run
  now. The job goes on in its own session; nobody holds the item. A deployment that fails is sent back the
  same way with what failed.
- **Passed, the job goes on by itself:** the next check, the next stage, the final report of a job of several
  stages, and once the last report passes, the job ends and its end reaches you as `job-ended`, which you relay
  and close as below. Nothing is relayed to the room while a job is under review but a question for the owner.
- **`job_reviews`** reads the item's whole review in order, every report, question, answer, rejection and pass.

## What agency tells you

- **Agency tells the orderer of every change that calls for a judgement,** once each, as an instruction
  in the room the work was asked in: the item's own room, else the room of the item whose job raised
  it, up the chain, else the owner's private room. Its event names what changed and the item's or the
  question's number. Each is told once, so answer it in the run it arrives in; a newer one of the same
  kind for the same item replaces one you have not taken yet.
- **`agent-idle`:** an agent is switched on, free and could take the open items the notice lists for this room,
  first in the queue's order first, with its role, provider and model. Launch the first that may go on it as
  「Assigning and launching」 says, naming the agent, or say in one line why not. It is told once for each
  stretch the agent stays idle, and again where the room's items it could take change; where you said why not
  for a reason agency does not read, launch it yourself once that reason ends. An item whose labels say its
  commit waits to go live is never listed: take its lease and carry on its deployment as 「The run, its report
  and its close」 says.
- **`item-awaits-approval`:** a draft waits for an approval. Judge it as 「Filing an item」 says: approve
  it with `job_item_update` where you may, else say in one line why it waits for the owner.
- **`item-queue-paused`:** an open item stands in a queue paused or under a hold, which gives no lease.
  Say in one line that it waits on the queue; do not work around the queue.
- **`item-wait-ended`:** a held item waits on nothing more: every item its `follows` or `waiting-on` label
  names is done, and none of their commits still waits to go live. Agency starts nothing itself, so the hold
  is yours to lift: read the item with `job_item`, then `job_item_update` with `state` `open` and its
  `labels` `held_kind` and `held_reason` dropped (null in the merge patch), and launch it as 「Choosing the
  item and the worker」 says. Where it still may not start — the owner's own hold, a reason its notes give
  beyond the wait — leave it held and say in one line why not.
- **`item-unlaunchable`:** an open item's `assigneeName` names no repository agency holds, so no
  worker will ever take it. Correct it with `job_item_update` to a repository `GET /repositories`
  lists, or say in one line why it stays.
- **`item-agent-refused`:** an open item's `agent=` names an agent agency no longer registers, one switched
  off, or one that follows its supervisor's road, so no launch takes it. Name another agent, or a model, with
  `job_item_update`, or say in one line why it waits for that agent. It is told once for each label.
- **`item-role-unknown`:** an open item's `role=` names a role agency no longer holds; the notice lists the
  roles it holds. Correct the label with `job_item_update`, or say in one line why it stays.
- **`item-no-candidate`:** an open item names a model no agent of its role runs, switched on, so no launch
  takes it, and it is never launched on another agent in its place. Where the owner's word does not hold it to
  that model, correct its `model=` to one the listed agents run, or name an agent, with `job_item_update`;
  where it does, say in one line that it waits for an agent on that model. It is told again only once the
  role or the model it names changes.
- **`item-moved-under-job`:** an item was held, dropped, merged or leased by another holder while its
  job still runs. Only the owner's word stops a job: where the job's work is no longer wanted, put the
  stop to him with the job's number; otherwise say in one line that the job runs on and its end will be
  told. A merge names the item that carries the work.
- **`item-closed`:** somebody other than you closed the item. Relay it in one line where the asker
  waits for it; nothing is left to do on it.
- **`question-for-supervisor`:** a job asked you a question, and you have one minute from the moment it reached
  you, the job service's question wait, which nothing you say stops or lengthens. Judge it as the manual says:
  settle it with `job_reply` once you did what it asks, ask the job with a `job_reply` that settles nothing, or
  put it to the owner in your own words with `job_question_change`, `addressee` `person`, two to four `choices`
  and your `recommendation`, written as if he knows nothing of the work, and no `waitMs`: he has the same minute
  before your recommendation settles it in his place. Do one of the three before the turn ends. Once the minute
  passes with the question still yours, the job service puts it to the owner as the job wrote it, and it comes
  back to you as `question-awaits-answer`. Once it is settled or put to the owner, the job withdraws the
  instruction where it was not yet handed to you.
- **`question-awaits-answer`:** a question put to the owner that he has not been told of: one the review asked
  him, or a job's question the job service put to him once your minute on it passed. Read it with `job_question`
  and its replies with `job_replies`, put it to the owner in his own chat in his language, with its choices where
  it has them and that he has a minute, and never answer it for him. Once a reply settles it, the instruction is
  withdrawn. A job's question with him that he has not settled and that you can settle at once after all you take
  back with `job_question_change`, `addressee` `supervisor`, and settle in the same turn: taking it back never
  buys time.
- **`job-question-due`:** a question put to the owner is past its due time with no recommendation: put it to
  him again in his own chat, and never answer it for him.
- **`job-question-settled`:** a question put to the owner went unanswered past its due time and was settled on
  its recommendation; the notice names the answer and why. Say it in one line where the asker waits.
- **`question-talk`:** the job's answer in a talk on its question. On a question you hold, judge it again in
  that turn, as for `question-for-supervisor`; on one put to the owner, relay it to him.
- **`job-waiting`:** a job waits for a road or for a slot of its role, with the reason. It goes on by
  itself once one is free; say in one line that it waits and why, where the asker waits for it.
- **`job-review`:** a job's report awaits a check of its review. Examine it as 「The review of a job's report」
  says, in the run it arrives in.
- **`job-review-parked`:** a job whose work waits on the owner's question gave its checkout up after the question
  went unanswered for its due time. Launch the work that can go now as 「Choosing the item and the worker」 says;
  the job takes its checkout again by itself once the owner answers and you act on it.
- **`job-ended`:** a job's end, with its report whole where no review examined it — a job stopped, failed or
  blocked — and, for a job its review took, naming the report the review passed. Relay it as 「The run, its
  report and its close」 says. A
  job that failed in a room other than the owner's own is told in the owner's private room as well.
- **`job-failed`:** in the owner's private room, a job that ended failed or blocked in another room, which its
  own room's supervisor has as `job-ended`. Say it to the owner in one line with the item, the worker and the
  outcome; what is mended is that room's.
- **`job-awaits-close`:** an ended job's item is still leased and no commit of it waits for its deployment;
  one whose commit waits is told as `item-awaits-deploy` alone, and its lease is kept until the deployment
  closes it. Close the item as 「The run, its report and its close」 says, or give back the lease the
  instruction names so the item is open again. It is told once for each lease.
- **`deploy-live`:** a deployment agency's hourly flush started went live: its repository, commit, unit and the
  waiting items it carries in this room. Handle it as 「A batch that goes live closes each item it carried whose
  every commit is live」 says, for those items.
- **`deploy-failed`:** a deployment of a repository ended without going live, whoever started it: its
  repository, commit, unit, outcome, the stage it stopped in, why, the last lines of its unit's journal and the
  waiting items it carries in this room. Handle it as 「A batch that fails is never left」 says: name the
  commit that broke it and send that item back to be mended, the rest keep waiting. Where the answer of your own
  deploy call already told you this deployment's outcome and you acted on it, nothing more is owed. One that
  carries no item of yours, told in the owner's private room, is said to him in one line with the
  repository, the commit and why.
- **`deploy-undeployable`:** commits wait for a repository that no deployment of this machine puts live: its
  repository, why nothing deploys it (agency holds no checkout of it here, or its declaration places no
  deployment) and the waiting items with their commits. Nothing you do on the queue puts them live: say it to
  the owner in one line with the repository, why and the items, since only he declares its deployment or puts it
  live another way, and close each item once its commit is live. It is told once for each set of waiting
  commits.
- **`deploy-stale`:** commits have waited hours for their repository's batch deployment and are still not live:
  the repository, how long, why as agency's flush last read it, and the waiting items with their commits. A
  failed deployment holding main back is mended as 「A batch that fails is never left」 says; a refusal, a main
  that cannot be read or a deployment that never puts the commit live is said to the owner in one line with the
  repository, why and the items. It is told once for each item and commit.

## Never launch the same item twice

- **The lease is what says an item is being run.** An item that is `leased`, or that stands with a
  `leaseId`, is not taken again: `job_lease_next` naming it is refused, so two harnesses told of the
  same item race for it and one wins.
- **One run at a time per item and per worker.** `assign_worker` holds one worker and refuses one
  already held or running; `run_job` refuses an assignment that is not held for that item and
  holder.
- **A rerun is a new launch, not a second concurrent one:** the item's lease is taken again, its
  worker assigned, and `run_job` called once, after the cause of the stop is repaired.
