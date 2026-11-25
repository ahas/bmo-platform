# A repository's checkouts

- **What this says:** how one repository runs several jobs at once — its base checkout, the support
  checkouts beside it, the worktree each is, the checkouts lent to a job beyond its own and the umbrella
  repository whose job works in them, how a job's work lands on main and how the base's main follows it, and
  what a job may write outside its own checkout. Which role a job runs as and how many of a
  role's runs go at once are `docs/role-slots.md`'s; a checkout bounds only how many jobs of one repository
  stand in its tree at once.

## The base checkout and the support checkouts

- **The base checkout:** the path the repository's row names (`docs/repositories.md`). Agency moves it off
  main only while it is lent to another repository's job, which it stands on that job's branch for (「A
  checkout lent to a job」).
- **Support checkouts:** further worktrees of the same repository, numbered from 2, which exist so that
  the queue can run more than one item of one repository at a time. They are the repository's, never rows
  of their own.
- **The support count is the repository's setting:** `auto`, or a whole number from 0 to 8
  (`policy/workers.toml`). `auto` is every repository's default.
  - **A number** is how many support checkouts stand beside the base, `w2` up to its count. Zero is none,
    and the repository then runs one job at a time. A count lowered leaves the worktrees beyond it on disk,
    unlisted, and a count raised again takes them up as they stand.
  - **`auto`** sets no number. The repository's support checkouts are the ones it has made, each recorded
    by its slot, and a launch that finds every one of them held makes the next (「Growing a checkout at a
    launch」). An idle one is kept for its next job and taken away only after the idle time (「An idle
    support checkout」).
- **Where it is changed:** the repository's settings in the console's repository tab, agency's repository
  routes (`docs/api.md`, 「repositories, prefix-checks and room-holdings」), and `bmo-agency agent --set
  <repository> --support auto|<n>`. The checkouts follow the setting at once: a numbered checkout missing on
  disk is made at its first launch.
- **A TODO item names the repository, never a checkout:** the repository's queue is every one of its
  checkouts' queue.

## Growing a checkout at a launch

- **What grows:** a repository set to `auto` whose base stands on this machine. A launch of one of its items
  that finds every checkout held records the next support checkout and holds it for the item at once, in
  place of refusing `no-worker-free` (`docs/api.md`, 「assignments and jobs」). Its worktree is then made at the
  launch like any support checkout's (「Names and places」). A repository whose base stands on a work node
  grows nothing: this machine's room says nothing of that node's.
- **Its number** is the lowest from 2 that the repository has not recorded and that nothing holds, so a
  slot taken away is the first made again, at the same path and with the sessions kept under it.
- **Two bounds, and nothing else, hold it back:**
  1. **A free agent.** The agents that could carry the item are the one its `agent` label names, or the
     candidates of its role for the model its `model` label names (`docs/role-slots.md`;
     `docs/role-agents.md`, 「Which agent a run takes」). Of those carrying no run now, one must be left over
     the holds of the same role that no job has started on yet, since each of those is a launch about to
     take an agent. With none left the launch is refused 409 `agent-busy`, naming what each candidate
     carries, and no checkout is made. An item naming neither label grows nothing, since no launch would
     seat it.
  2. **Room on the machine.** The same judgement the launcher reads before every launch
     (`docs/agent-control.md`, 「Room on the machine」), the memory the jobs at work are still to grow into
     among it. Where it holds, the launch is refused 422 `no-worker-free`, its sentence naming each reason
     with its figure, and the item waits as it waits for any checkout.
- **One launch grows one checkout at a time.** The repository's row and the candidate agents are held for
  the growth's transaction, so two launches at once never record the same slot nor both count the same free
  agent.
- **A borrowing grows the same way** (「A job writes only in its own checkout」), bounded by the machine's
  room alone, since a borrowed checkout seats no agent.

## An idle support checkout

- **It is kept,** worktree, installs and the sessions kept under its path alike, so the next job of the
  repository starts warm.
- **It is taken away once idle for `support_idle_hours`** (6, `policy/workers.toml`): no assignment of it has
  changed and nothing has held it since, counted from when it was recorded where nothing ever has. Agency
  looks on the heads' clock, every ten minutes, and only on its own data root.
- **What is never taken away:** a checkout something holds — an assignment `held` or `running`, or a job at
  work in it — or whose checkout lock a job or a fold holds; and a worktree with anything open in it, which
  stays and is named on the error stream each pass. Its branches and commits are the repository's and stay
  with it, and its workbench folder under the data root stays for the slot's next making.
- **The taking away** is `git worktree remove` from the base checkout, then the record dropped. A record
  whose worktree is already gone is dropped alone.
- **A worktree standing at a slot's path with no record,** of a repository set to `auto`, is recorded on the
  same pass, from that moment: one made under a number before the repository went `auto` is taken up, and
  leaves once idle like any other.

## Names and places

- **Name:** `<repository>-w<n>`, with `n` counting from 2 — the base is the first of its repository.
- **Label:** the repository's label followed by `#<n>`.
- **Directory:** a git worktree of the repository at the base checkout's path followed by `-w<n>` — `<base
  path>-w2` for a repository's first support checkout.
- **The path is fixed for the life of the support checkout.** A job does not get a directory of its
  own: the directory is part of every Claude session's prefix, and a session is kept under its
  directory's name, so a path that moved per job would cold-start every session and could resume
  none of them.
- **The worktree is made at the launch, before the job's prompt is composed and the run's program is
  stood, the first time a job lands on it** — on whichever road the launch takes. The prompt's header
  reads the checkout's directory (`docs/worker-bench.md`, 「Preamble」), so it reads the worktree as it
  stands for the job. It is detached at the base's main branch, read
  off the base checkout, and the repository's own setup is then run in it: the script named
  `worktree:setup` in its `package.json`, where there is one. What it installs and links is that
  repository's own; the operations repository's installs its workspace from its lockfiles and links
  its env files, the way the base checkout has them.
  - **The submodules come first,** in every repository and before its script: each submodule the
    commit pins is cloned where it is missing and put on the pinned commit, so a project kept in a
    repository of its own stands in every checkout as it does in the base. A submodule that does not
    stand on its pin has that commit fetched first, from the checkout the place is a worktree of and
    otherwise from the submodule's own remote, before the move is tried again, so a pin standing on a
    branch outside the submodule's default, or one its remote never received, comes up as it does in
    the base. A folder an older commit
    tracked where a submodule now stands keeps only what git ignored in it, installs and builds,
    which would refuse the clone, so it is emptied first. What is open inside a submodule is left
    as it stands, and a submodule git will not move over it fails the setup. A repository's own
    script may bring them the same way before it installs, so a checkout set up by hand has them too.
- **A support checkout whose worktree is missing gets it made again at its next launch,** by the same
  cut: detached at the base's main, the submodules first and the repository's setup after them. A
  directory holding anything that is not a checkout is somebody's and is refused rather than taken
  over, and a worktree whose setup failed is taken away so that the next launch starts it afresh. The
  setup runs until its script exits, however long a cold install takes, and a second launch of the same
  support checkout waits for the first to finish making the worktree, then finds it standing.
- **The setup is run again every time agency moves a checkout's files under its installs:** whenever it
  puts a checkout lent to a job on that job's branch (「A checkout lent to a job」), a support worktree or a
  base checkout alike. Main moves under every checkout as other jobs land, and a lockfile it brought along
  leaves the installs behind the code, so a check would fail on something the job never touched.
- **So a repository's setup is cheap when nothing moved,** and deciding what moved is the
  repository's own: a setup may hash its lockfiles and install only where that hash differs from the
  one written beside its installs when they were last made, or where nothing is installed. A setup that
  fails stops the job from starting, or refuses the loan, and says why.
- **What is shared and what is not:**
  1. The package installs share Bun's install cache, which hard-links the files, so a support
     worktree's `node_modules` costs links rather than copies and is still right for its own branch.
  2. Build output is each worktree's own, because several dev servers write it at once.
  3. Each support checkout has its own workbench folder under its own name, so its dev server reads
     a data copy of its own and never the live data.

## A job's own checkout

- **Agency moves no branch in a job's own checkout while the job holds it;** between jobs it brings an idle
  base's main up to the remote's (「The base checkout's main brought up to the remote's」). The job is handed
  the checkout as it stands: a support
  worktree as its last job left it, or as it was made, detached at main; the base checkout on whatever was
  last checked out there. Nothing is committed, switched or parked there for it. Which branch the job works
  on and what it leaves open are its own, and its close lands what its `HEAD` holds (「Landing on main」).
- **Main is the branch the base checkout stands on,** read off the base checkout — the one the
  repository's row names — and never off a support worktree. Main is what a new worktree is detached at
  and what a lent checkout's job branch is made from (「A checkout lent to a job」).
  - Where the base checkout itself stands on a `work/` branch, main is the branch that `work/` branch was
    put on from, where the repository's git config writes it down beside the branch and that branch still
    exists.
  - Where nothing is written down, it is the branch the remote's own head names, and failing that
    `main`, then `master`.
- **A job launched again stands in the same checkout.** Where a job's road stopped answering and the job goes
  out again on it once it answers (`docs/cache-fallback.md`, 「The road judgement」), its turn goes out in the
  same checkout: what the turn before left open there stays open, and the new turn is told it continues
  work already in the checkout.
- **A verb run inside the worktree works on the worktree.** The job carries agency's
  environment, whose `BMO_SOURCE_DIR` names the base checkout, so every verb that reads or writes
  the checkout's own files asks for the working checkout instead: the worktree the
  command was started inside, where git registered it under the base checkout's repository, and the
  source root everywhere else. The deploy tree is such a worktree too and never counts.

## A checkout lent to a job

- **A job may hold checkouts of other repositories beside its own,** each an assignment held in the job's
  name, `job:<jobId>` (`docs/api.md`, 「assignments and jobs」): it reads held everywhere a held checkout is
  read, and nothing is launched into it. The job writes, commits and closes there as in its own (「A job
  writes only in its own checkout」, 「Landing on main」).
- **Two roads lend one:**
  1. **At the launch,** each repository the item's label `repositories` names, comma-separated as the job
     unit's `todo --add` writes it, other than the job's own repository. The list is read as the job is
     written and held fixed with the rest of what the launch read (`docs/repositories.md`, 「An agent an item
     names」).
  2. **During the job,** `bmo job borrow <repository>` (「A job writes only in its own checkout」).
- **Which checkout,** on the job's own machine:
  1. the checkout of the repository that has the job's branch checked out, where one has: git lets a branch
     stand checked out in one worktree of a repository at a time, so it is the only one that can take the
     branch, and it is waited for while something holds it;
  2. otherwise a free support worktree by number, then the base checkout, passing over one standing on main
     with something open, which is somebody's work;
  3. where none is free, one grown for a repository that grows (「Growing a checkout at a launch」).

  A support worktree not made yet is made first, as a launch makes it (「Names and places」).
- **A launch takes its loans together or waits.** Before the job's first turn every repository its item
  names is lent in one pass. Where any of them has no checkout to give, every loan the pass took is given
  back and the job waits `starting`, its `heldFor` naming the repositories it waits for, told to its item's
  orderer once, with no clock, as a launch waits for its seat (`docs/role-slots.md`). It looks again
  whenever a job or an assignment changes, and at least every `loan_look_seconds` (`policy/workers.toml`).
  A job stopped while it waits goes no further. Taking all or none keeps a launch from holding one
  repository while it waits on another.
- **Each lent checkout is put on the job's branch** before it is handed over, `work/<item>`, on this machine:
  1. where it stands on a branch other than main and the job's with something open, that is committed onto
     the branch it stands on: the earlier job's work, parked;
  2. `work/<item>` is checked out where it exists, and otherwise made from main, with main written down
     beside it in the repository's git config, which is how main is read off a base checkout standing on
     it;
  3. the repository's setup is run on what the branch holds, where the branch was moved.

  A checkout that cannot be put on the branch is given back, and the launch fails or the borrowing is
  refused with what git said. A checkout on a work node, and a directory that is no git checkout, is lent as it
  stands.
- **The job's prompt names them.** The prompt of a job holding loans opens, under its title, with every lent
  repository, its checkout's path and its branch, in the words of `policy/job-prompt.toml`; a review turn
  that took loans again after a pause carries the same list ahead of its words (「Loans across a job's
  review」).
- **How many:** one job holds at most `job_checkouts_most` checkouts (`policy/workers.toml`), its own among
  them. An item naming more is refused at its filing and at its launch, and a borrowing past the bound is
  refused.

## Loans across a job's review

- **A loan stands while the job is at work:** `starting`, `running`, or `reviewing` with its checkout kept
  (`docs/job-review.md`, 「The states of a job under review」), so every round of the review keeps it.
- **A job that gives its checkout up gives its loans up:** its review paused past the owner's due time, or
  the job ended, stopped or failed. Each loan is given back on the next read of the assignments:
  1. on this machine, what is open in the checkout is committed onto the job's branch, parked;
  2. a lent base checkout is put back on main, and the job's branch deleted where main already holds all of
     it; a support worktree stays on the branch;
  3. the loan is released.

  A checkout being given back still reads held, so nothing takes it half parked. A give-back cut short by a
  stop of agency is taken up again once `loan_return_seconds` (`policy/workers.toml`) have passed.
- **A paused job takes its loans again** before its next turn, after its own checkout: every repository it
  held a loan of, chosen as a launch chooses, so a support worktree still on the job's branch is taken again
  where it is free and waited for where it is not.

## An umbrella repository

- **A repository row may be an umbrella:** its path is the folder that holds other repositories' checkouts
  below it, and its own tree is written by none of its jobs. It is marked on its row (`umbrella`,
  `docs/repositories.md`) and registered with the repository verbs like any other; its path is deployment
  data.
- **Its job stands in the folder.** The session opens at the umbrella's path, so the rules each checkout
  below it carries and the owner's documents there are read as files of that folder, and its cached prefix
  is the one row's. Its work is done in the checkouts lent to it, which stand below the folder.
- **It runs one job at a time.** An umbrella row takes no support checkouts and no node checkouts, refused at
  the add and the change, so its one checkout's lock lets one job in.
- **Host work may run there.** An item naming no repository goes to the checkout named `worker` (`docs/api.md`,
  「assignments and jobs」); a deployment whose `worker` row is its umbrella runs that host work in the umbrella
  folder, one job at a time and under the same guard, and one umbrella row stands at a path, never two.
- **Its own tree is guarded, against every job:** the job guard refuses a write to a file the umbrella's git
  tracks and git changing the umbrella's own repository, its own job's as much as any other's, and lets a write
  in a checkout lent to the job through (「A job writes only in its own checkout」; `docs/live-claude-guard.md`).
  The umbrella is no other worker's checkout beyond its tracked files, so a place below its folder that no
  checkout names stays as unguarded as it was.

## A repository whose base is a worktree of another's

- **A repository's path may be a linked worktree of another repository's checkout** — a repository kept
  for one pinned model beside the code tree's own row, `<checkout path>-<name>`. It is a repository of its
  own: its own name, label, pin, rules and queue, and no other repository's settings count it.
- **A repository renamed with its checkout has the worktree moved by agency,** as its server starts and on
  the heads' clock, the rename read off agency's policy (`policy/agent-renames.toml`): git moves it to the
  directory the rename names while agency holds the checkout's job lock, so no job is at work in it, and the
  row is pointed at the new directory only once it stands there. A job holding the lock puts the move off to
  the next pass.
- **Its checkout is its own:** its jobs hold that worktree's lock alone, so a job of the other repository
  never holds it up nor it the other's.
- **It works the way a support checkout does:**
  1. Main is read off the repository's main worktree — the checkout git keeps the repository in — and never
     off the worktree, which cannot have main checked out while that checkout does.
  2. Agency moves no branch in it for its own jobs (「A job's own checkout」), and the close lands what a job's
     `HEAD` holds on that main.
  3. Nothing else writes in the worktree, so everything a job leaves open there is its own.
  4. A loan takes the worktree the way it takes a support checkout (「A checkout lent to a job」), and gives it
     back on its branch, never on main.
- **The worktree is made once, by hand:** `git worktree add --detach <path> main` in the main checkout, the
  repository's setup (`worktree:setup`) run in it, and the row then pointed at it with `bmo-agency agent
  --set <repository> --path <path>`. Nothing makes or removes it for the row.

## Landing on main

- **The job unit's close lands a job's work,** `bmo-job job close`, which a job runs once its work and its
  commits are done; its canonical statement is the job unit's `docs/cli.md`, 「bmo-job job close」. Agency
  holds no part of it: no lock, no line, no rebase and no check of its own.
- **What it does, in short:** it fetches the remote's main, refuses a checkout whose main no longer stands
  under its `HEAD` (it merges by fast-forward alone and never rewrites the checkout), refuses a commit
  message holding Hangul, runs the change check over what changed since main, pushes `HEAD` onto the
  remote's main, and writes where it stands onto the item's labels, a pair for each repository it closed.
  It runs no suite, takes no close line and rebases nothing.
- **Several checkouts in one close:** a job holding loans closes every checkout it changed in one run, each
  named by a repeated `--checkout`, its own and the lent ones alike. Which order they are pushed in, and the
  one check over all of them, are the close's and the testbed's; agency only lent the checkouts.
- **A branch that main moved past** is the job's to bring up — merge or rebase main into it and close again —
  since the close fast-forwards alone.

## The base checkout's main brought up to the remote's

- **The close moves the remote's main alone,** so the repository's main on this machine is agency's to bring
  up. The base checkout is what other repositories build against by path, and main read off it is what a new
  support worktree is detached at (「Names and places」), so a main left behind breaks both.
- **What moves:** the repository's main branch, read off the base checkout (「A job's own checkout」), brought
  up to the same branch on the remote that branch follows, else `origin`, by a fast-forward alone. A main
  ahead of the remote's, or gone apart from it, is left as it stands.
- **Where main stands decides how it moves:**
  1. checked out in the base checkout: the checkout is fast-forwarded in place and what is open there stays
     open. Where the move would overwrite an open file git moves nothing, and the base is named on the error
     stream on every pass until that file is cleared;
  2. checked out nowhere, the base standing on a job's branch or detached: the branch is moved alone,
     guarded by the commit it was read at;
  3. checked out in another worktree: left to the repository whose base that worktree is.
- **Only an idle base is moved:** nothing holds it (an assignment `held` or `running`, or a job at work in
  it), no job or fold holds its checkout lock or its agent's lock, it stands on this machine, and it is no
  umbrella. A base passed over is brought up on a later pass.
- **A base checkout fast-forwarded in place is set up again,** as every checkout agency moves under its
  installs is (「Names and places」); a setup that fails is said on the error stream, and the move stands.
- **When it runs:** as each job's turn ends, from what git on this machine already knows of the remote, since
  a close run here pushes from a checkout of the same repository and git records the remote's main as it
  pushes; and every ten minutes on a clock of its own, with the remote's main fetched first, which brings what
  was pushed from elsewhere, a node's checkout among it. Only on agency's own data root.

## Checkouts on the nodes

- **A repository may also have one checkout on each work node that takes jobs,** named `<repository>@<node>`
  with no number, set in its settings as the list of its node checkouts and never by the support count
  (`docs/api.md`, 「repositories, prefix-checks and room-holdings」; `docs/work-nodes.md`, 「Which nodes take
  jobs」). Its base stands on this machine. Its path is the one the setting names, written from the node's home
  or absolute, else the base's folder name followed by the node's beside the base; one removed from the list
  while no job holds it goes from the row, and its checkout on the node stays there.
- **Its jobs work in it as in any job's own checkout** (「A job's own checkout」), and close it with the job
  unit's close run on the node, which pushes onto the remote's main as every close does (「Landing on main」;
  `docs/work-nodes.md`, 「Ending a job on a node」).
- **A loan of it is lent and given back as it stands:** agency moves no branch on a node.

## Parking

- **Agency parks only what it lent.** A loan given back commits what is open in its checkout onto the job's
  branch (「Loans across a job's review」), and a loan of a checkout standing on another branch than main with
  something open parks that onto its branch first (「A checkout lent to a job」).
- **A job's own checkout is left as the job left it.** What it left open stays there, for its report to name.

## The queue with support checkouts

- **A repository takes as many jobs as it has free checkouts** — the base's, and each support checkout's
  within the count — and each of them waits for a slot of the role it runs as as well
  (`docs/role-slots.md`). The fact that tells the supervisor the queue is waiting counts the free checkouts
  of each repository, and one item is launched per free checkout.
- **A repository that grows counts as free while it may grow:** set to `auto`, its base on this machine, and
  no launch held for the machine's room by the launcher's notice. Every open item of it is then told as
  awaiting a launch, each still waiting on its agent as below, so the queue is offered one item per free
  agent rather than per checkout.
- **The launcher picks the checkout.** It takes the first free of the base checkout and then the support
  checkouts by number, and grows one where none is free and the repository grows. A launch naming a support checkout
  directly takes that one or is refused. Where the repository has checkouts on the nodes, the launch first
  takes its machine from the rotation and then the checkouts there in that order (`docs/work-nodes.md`, 「The
  rotation」).
- **A stopped work order at the head of the queue is its runner's to resume** in the checkout it holds; the
  items behind it are still told as waiting where the repository has another checkout.
- **An item already at work is not launched a second time,** on any checkout of its repository; the refusal
  names the job that has it.
- **A launch meeting a held lock tries the next free checkout** before it refuses.
- **Free checkouts are not room on the machine.** A launch with a free checkout and a free slot is still
  held while the machine is short of memory, threads or job places (`docs/agent-control.md`, 「Room on the
  machine」), and the queue is told again once it has room.

## Dev servers

- **A support checkout's dev server stands only while a job in it runs and has asked for it,** as
  every checkout's does (`docs/worker-bench.md`, 「It stands only while a job runs」). It runs the
  repository's dev command in the support worktree, from its own workbench, and stops once the job is
  no longer at work in the checkout or leaves it unused; a fold holding the checkout's job lock does not
  stand it.
- **It is told its number:** the pane runs it with `BMO_WORKER_SLOT=<n>` in its environment, and a
  dev server that binds ports moves them by that number. The operations repository's bench does
  (`docs/worker-bench.md`); the base's dev server runs with none and keeps its ports.
- **The job's preamble says which checkout it is:** a job in a support checkout carries, under its
  title, the repository's preamble under a header saying it is support checkout `n` of the repository, in
  its own worktree, with its own workbench, that its dev server stands only once the job starts it, the
  lines that start, restart and stop that dev server, and that where the owner's text names the base's
  workbench, console or ports, it means the support checkout's own. A job whose checkout, base or
  support, stands on its item's branch is told the branch, and that the close lands it rather than a
  push (`docs/worker-bench.md`, 「The bundle is agency's header plus the owner's text」).

## A job writes only in its own checkout

- **Every repository's checkout is someone's.** A job may write in its own checkout and in a checkout it
  has borrowed, and in no other checkout of any repository — base or support — whoever holds it. A place no
  repository names is not guarded by this.
- **The job guard refuses the write,** from the file tools and from the shell alike, and the refusal names
  whose checkout it is and the verb that borrows one (`docs/live-claude-guard.md`, 「The same hook keeps a job
  to its own checkout」). It is a reading of text, so the rule stays the job's own to keep where the
  reading cannot see, such as a script file.
- **Borrowing:** `bmo job borrow <repository>`, run inside a job, lends it a checkout of that repository for
  the rest of the job, chosen and put on the job's branch as every loan is (「A checkout lent to a job」), and
  prints its path. The job commits and closes there as in its own checkout (「Landing on main」). A job's own
  repository is refused, and so is a loan past the job's bound of checkouts.
  - **Where no checkout can be given** — every one at work and none to grow — the plain form says so and asks
    nothing to wait.
  - **`--wait`** waits for one instead: the server holds the ask until a checkout comes free, looking again
    whenever a job or an assignment changes, for at most `borrow_wait_seconds` (`policy/workers.toml`), and
    the verb prints a line saying what it waits for each time the server answers that it still waits, then
    asks again, so the call is never quiet long enough for the call clock. It ends lent, or refused for any
    other reason.
- **A job borrows with what its program holds.** A job's program is handed no control token, so the verb
  calls agency's server with the job's session token, and the server lends only to the job running in the
  checkout that token was issued for (`docs/api.md`, 「The control port」). A process holding neither token
  is refused, naming both. Nor is it handed the server's port: the verb reads it from the deployment's
  settings, `AGENCY_CONTROL_PORT` in `env/agency.env` under the data root the job's `BMO_DATA_DIR` names.
- **When the job gives its checkout up the loan ends,** parked onto the job's branch first (「Loans across a
  job's review」).
- **Why:** a loan given back parks everything open in its checkout as the job's own, so a write from any
  other job would leave with it. With every job in checkouts of its own, what is open in a checkout is its
  holder's, and parking needs to sort nothing.
