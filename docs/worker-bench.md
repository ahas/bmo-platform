# A repository's workbench

## What a repository holds outside its jobs

Each repository holds three things beside the work of its jobs:

1. the **preamble**, which a job's prompt carries under its title
2. the **workbench folder**, which remains regardless of sessions
3. the **dev server** its row names, which stands in a pane of its own in a job's checkout once that job
   starts it, and which the job starts, restarts and stops through agency's verb, never beside the pane

- **Where they come from:** all three follow from what the repository's row says, and differ per repository.
- **None of them is part of a session's prefix.** What the row puts into the prefix is its list of
  prefix documents (「Prefix documents」 below).

## Prefix documents

- **A field of the repository row, `prefixDocuments`:** a list of paths, in order, set and read through
  agency's repository routes (`docs/api.md`, 「repositories, prefix-checks and room-holdings」). Left empty, there
  are none.
- **A path is the caller's choice and the code decides none.** A relative path is read from the run's
  checkout, the directory its row names; an absolute path is read where it stands, such as the umbrella
  checkout's `STRUCTURE.md` or another repository's document under the umbrella. A path is refused where
  it is empty or climbs with `..`.
- **They stand in the system prompt of every run at the repository's path,** after the worker line's standing
  rules and the repository's rules, one after the
  other in the list's order, each headed by its path as written. A launch reads each fresh from disk; a
  document that cannot be read is left out and the launch says so in agency's log, so a path that moved
  never stops a job.
- **They are part of the prefix.** The run's composed rules file carries them, so the session's prefix
  digest moves when the list or a document's text changes, and the session is folded or opened afresh
  as any other change to the prefix asks (`docs/launch-lines.md`, 「Why a line is held steady」).
- **What they are for:** the documents a small model needs before it touches the repository — its
  structure, the canonical document of the area it works in — read once into the cached prefix rather
  than again by every job.

## A small-model instruction

- **The whole of a small-model instruction is four fields,** the `instruction` a job carries (`docs/api.md`,
  「assignments and jobs」):
  - `repository`: the checkout the change is made in;
  - `documents`: the canonical documents the change answers to, read before anything is touched;
  - `sentences`: the sentences the change must make or keep true, each a statement a reader can check
    against the result;
  - `checks`: the commands whose passing says the change is done.
- **Each field is required and none is empty.** A job that carries an instruction carries all four; one
  missing, empty or naming an empty entry is refused with the field it lacks. A job without an instruction
  is a job as the item's brief states it.
- **The job's prompt renders it first,** under the item's title and ahead of its brief: the repository,
  the documents to read, the sentences, and the checks, each as a list in the order given. The brief,
  work order and notes follow as they stand.
- **A work-order step's text is composed from it,** `POST /instruction-texts`: one line naming the
  repository, the documents, the sentences and the checks, in the same order, so a step saved in the job
  service and the prompt that runs it say the same four things.
- **Why four fields:** a small model changes code safely where it is told where, against what, to which
  statements and how to know it is done, and nothing else; whatever the four do not carry is the item's
  brief, which such a job may leave empty.

## Preamble

### What it is

- **A field of the repository row.**
  - It is written in the console's repository tab through agency's repository routes (`docs/api.md`,
    「repositories, prefix-checks and room-holdings」); `bmo-agency agent` edits the same field.
  - Left empty, there is none.
- **The owner's text, and the owner's choice.** For example: which port the dev server stands on,
  what its outside address is, and to check it rather than start a new one.

### It opens the prompt a job's session is handed

- **Read fresh:** whenever agency hands a job's prompt to a session that does not hold it yet, it reads
  the repository row and the checkout at that moment and composes the bundle anew. A support worktree
  not made yet is made before it is read (`docs/worker-slots.md`, 「Names and places」), so the header
  tells the checkout the job's program is stood in. It is read for:
  - the job's first turn;
  - a turn of the job's review carried into a fresh session, because the session it answered on cannot
    be continued (`docs/job-review.md`), which opens on the job's prompt again.
- **Every job in a repository's checkout gets it,** whatever its role. A turn said to a session that
  holds the prompt already carries no bundle of its own: a later stage of the work order, or a rework.
  The turn a road that stopped answering hands to the next road repeats the words that went out, the
  bundle with them where they carried it.
- **The owner's text goes in English.** Each time the bundle is composed, the preamble is translated into
  the worker's language, or goes in as written under a mark where no translation came back in time
  (`docs/worker-language.md`, 「What agency translates on the way in」); the row keeps it as the owner wrote
  it.
- **The order of the first prompt:**
  1. the item's title
  2. the bundle
  3. the checkouts lent to the job, where any are (`docs/worker-slots.md`, 「A checkout lent to a job」)
  4. the handoff the last session on the item left, where one did (`docs/sessions.md`, 「The handoff a
     job's session leaves」)
  5. the job's text: its instruction, brief, work order and notes
  6. the reference the gate picked over the job's instruction, where it picked any
  7. the opening of the job's review (`docs/job-review.md`)

  The report trailer is no part of it: it stands in the run's prefix, after its rules
  (`docs/launch-lines.md`).
- **Not kept:** the prompt the job's row keeps is the one without the bundle, so a later session opened
  on it reads the row again rather than repeating an old reading.
- **Editing the field:** the new text stands from the next job prompt handed over.

### What it is not attached to

- **Two kinds of added words:** added words pushed into a running turn, and added words asked after an
  answer, are said to a session that already has the job prompt.
- **So neither** carries the preamble.

### It is not the prefix

- **Where the prefix comes from:** the command line and the files it points at — the CLAUDE.md chain,
  settings, MCP config, plugins and skills (`docs/launch-lines.md`).
- **The preamble is in none of them,** nor in the run line's arguments or environment.
- **So editing the field** leaves agency's prefix digest unchanged: no session fold, no cold cache.

### It is agency's own words

- **The bundle is agency's header and the owner's setting,** not something someone sent. No sanitiser
  reads it, nor any other part of a job's prompt (`docs/cache-fallback.md`, 「No sanitiser stands on a
  session's boundary」).

### The bundle is agency's header plus the owner's text

- **When there is one:** a job's prompt carries the bundle where any of these holds; where none does, it
  carries none at all:
  - the repository has a preamble;
  - the repository has a dev-server command and the checkout stands on this machine;
  - the checkout's directory is not a git checkout;
  - the checkout is a support checkout;
  - the checkout stands on the item's branch, `work/<item>`.
- **The header** opens it, under a heading of its own, saying:
  - which checkout the job runs in: its name, the repository's, and its path;
  - for a support checkout, that it is support checkout `n` of the repository in a worktree of its own,
    and that where the owner's text names the base checkout's workbench, console or ports, it means this
    checkout's own (`docs/worker-slots.md`, 「Dev servers」);
  - for a git checkout on this machine, a base clone and a worktree alike, that it is one, the branch it
    stands on or the commit it is detached at, and the address of its `origin` remote with any user and
    password taken out, or that it has no `origin`; and that the job commits in it and closes as the
    closing steps say;
  - for a directory that is not a git checkout, that there is nothing to commit, push or deploy, so the
    closing steps that need git are skipped and no repository is made;
  - for a checkout on the item's branch, the branch, and that the close lands it, so it is never pushed;
  - for a checkout on this machine, the path of its workbench folder, and where in it what one item
    alone needs goes, the item's folder and its database's name (「An item's scratch」 below);
  - for a dev-server command on a checkout on this machine, that the dev server stands in a pane outside
    the job only once the job starts it, the path of its log file and the three lines that start,
    restart and stop it (「A job starts, restarts and stops it」) — with the rule that a job restarts a
    dev server that is down, failing or serving stale code and carries on, rather than leaving it as
    unfinished.
- **A checkout on a work node** is told neither a workbench nor a dev server: both stand on this machine,
  and a worker on a node runs no dev server (「Agency sets it up」). Its directory is taken to be a git
  checkout, since agency reads no file on the node.
- **Below it** stands the owner's text as written, under a heading of its own.
- **Language:** the header, like the report trailer, is the deployment speaking to the job's run, so it is
  written in English; its lines stand in agency's policy, beside the lent checkouts' heading.
- **What it is composed from:** the repository's preamble and dev-server command on its row, the
  checkout's own row — its support number, its node and its path — what git says of the checkout's
  directory, the branch or commit it stands on and its `origin`, and the checkout's workbench folder
  (「Workbench folder」 below).

## Workbench folder

### What it is

- **Where:** `workbench/<checkout name>/` under the runtime data root, one for each checkout of the repository
  — the base's under the repository's name, a support checkout's under its own.
- **What goes there:** things a job has no reason to remake each time, such as a check copy of the
  data, check logins and the dev-server log.
- **Not the session scratchpad:** that is made fresh per session and never reused, so it is no place for
  these.

### How it is kept

- **Agency creates it:**
  - for every checkout of every repository when it starts
  - for a repository's checkouts when the repository is added or its support count raised
  - for a job's checkout when the job starts, which the job's runner asks of agency with the rest of the
    bench: what a job before it asked of the dev server taken back, and the dev pane brought up where
    the repository has a dev command (`docs/api.md`, 「repositories, prefix-checks and room-holdings」)
  - leaving it as is if it already exists
- **A co-work branch has none:** its place lives one task and stands in no repository's row
  (`docs/role-slots.md`, 「Standing branch checkouts and their list」), so a branch's job makes no workbench and its prompt
  names none.
- **Removing a repository does not delete what the folder holds,** since agency does not know what is
  inside. A workbench no repository's checkout names and that holds nothing goes with the item scratch sweep below.
- **Inside no checkout,** for the same reason the data root is outside every checkout.
- **Left out of deployment backups:** its contents can be remade, and one data copy exceeds a hundred
  megabytes.
- **How a job learns the path:** only from the header of the preamble bundle, which agency composes
  (「The bundle is agency's header plus the owner's text」). It is not passed as an environment variable,
  since the worker line's environment is fixed. Another caller reads the path, with the item scratch
  folders standing under it, from agency's repository routes (`docs/api.md`, 「repositories,
  prefix-checks and room-holdings」, a repository's workbench).

### An item's scratch

- **What one item alone needs goes apart:** a rehearsal's data root, a copy of the data taken for
  that item's check, a check's build or download. It stands in `items/<item number>/` inside the
  workbench folder; a PostgreSQL database made for it on this machine's cluster is named
  `bmo_item_<item number>_<anything>`. The header tells a job both, and the job reads its item number
  off its own item.
- **It is one of the job's own places,** so a confirmation on a command that stays inside it is
  answered by the job's runner, never put to the owner (`docs/agent-control.md`, 「A command the runner allows
  itself」).
- **It goes once no job of the item is open.** Agency's sweep looks every minute, and acts once as its
  process starts and again whenever the set of items with an open job changes: every `items/<n>/` folder
  of every workbench whose item has no open job is removed, and every `bmo_item_<n>_…` database on the
  store's cluster likewise dropped. Which items have an open job is the job unit's, which holds the jobs:
  agency asks it at the open items point (`docs/api.md`, 「Hooks」), and where nobody answers, no item's
  scratch and no database is touched. The same look removes every empty workbench folder no repository's
  checkout names, whoever answers. A database the account cannot drop stays, and agency's journal names it.
- **What outlives the item stays beside it,** in the workbench folder itself: the dev server's data
  copy, logins, a tool's environment several items reuse. Nothing outside `items/` is removed by the
  watch.
- **Why:** a data copy or a rehearsal is gigabytes, and one left behind by each finished item is
  what fills the disk.

## Working directory

- **Named or made.** A repository is added with a directory, or with none; one added with none works in
  `worker-homes/<repository name>/` under the runtime data root, made when the repository is accepted.
  - A repository name holds no path separator and cannot start with a dot, so the directory cannot reach
    outside that folder.
  - A directory left there by a removed repository of the same name is taken over as it stands.
  - Removing the repository does not remove the directory, for the same reason as the workbench.
- **A checkout or not.** Any existing directory is accepted; whether it holds `.git` (a directory, or the
  file a worktree carries) decides what the repository's jobs do.
  - **A checkout:** everything a job does, co-work included.
  - **Not a checkout:** ordinary jobs only.
    - A launch that asks for co-work (the item's flag, `--cowork`, or the sentinel in the text) runs as an
      ordinary job, and the launcher says so; agency refuses a co-work place in such a directory.
    - The job's steps that read git — the work-tree baseline, the commits a job made —
      find nothing and count as nothing, not as a failure.
    - The preamble header tells the job to skip committing, pushing and deploying.
  - The repository tab marks such a repository as not a checkout.
- **The lock stays the directory's.** A made directory is one per repository, so such a repository never
  waits for another.

## Dev server

### The command

- **Another field of the repository row: the dev-server command.**
  - One shell line; left empty, that repository has no dev server.
  - The command decides the port, bind address and base path. Agency does not pick ports.
- **The outside address** is the machine's own web server's, which splits paths onto ports; the command
  must listen on the port that site expects.

### It stands only while a job runs

- **Every checkout alike:** a repository's base checkout, a support checkout and a checkout of any other
  repository, each its own dev server in its own tree.
- **And only once that job has asked for it.** A job's start stands no server; a job that is about to
  look at what the server serves starts it (「A job starts, restarts and stops it」), and a job that
  never does holds none.
  - **The request** is a file in the workbench folder, `dev-server.wanted`. The start and restart
    verbs write it; the pane removes it whenever it takes the server down for good — the job over,
    memory short with nobody on the server, or a stop asked for — and the job's runner removes one left
    from before as the job starts.
  - **Why:** a console dev server holds two to three gigabytes once its pages have been opened, and
    one stood up for every job, the many that never open the console included, ran the machine out
    of memory with five of them standing.
- **What counts as a job running:** a job at work in the checkout, read as a checkout's hold reads one
  (`docs/api.md`, 「assignments and jobs」): a job of that checkout `starting` or `running`, or `reviewing`
  unparked. So it counts through every stage and every review round, the pauses between their turns
  included, and stops counting once the job ends or gives its checkout up while the owner's word is awaited
  (`docs/job-review.md`, 「The owner's word」). A job whose execution went away is ended stopped by agency
  (`docs/api.md`, 「assignments and jobs」), and then counts no more.
- **Why not a lock:** a job's turns run in sessions of their own, one after another, with no process
  standing for the whole job, and the checkout's job lock is taken by a fold and a worker ask too, which
  never look at what a dev server serves.
- **The job ends, the server ends:** once no job is at work in the checkout, the pane stops the server,
  removes the request and closes itself; the next job starts it again where it needs it.
- **No clock ends it:** a server stands for as long as the job that asked for it runs and has not
  taken the request back, however long that is. Its age and how long it has gone unasked end nothing.

### It goes down when memory is short and nobody is on it

- **What counts as being on it:** a client outside the server's process group holding a TCP connection
  to a port the group listens on: a browser with a page open holds the page's hot-reload socket. A
  connection between two processes of the group — the console's server reading its stand-in bot — is
  not one. The pane asks `lsof` for the group's connections only while memory is short, and at most
  every thirty seconds, since one asking costs a third of a second of CPU.
- **When memory is short, the unused go down:** while the machine is short of memory by its launch
  limits (`docs/agent-control.md`, 「Room on the machine」) — less memory available than
  `machine_limit:launch_memory_available_gb`, or the last minute's memory pressure at or above
  `machine_limit:launch_memory_full_pct` — a server no outside client is connected to is stopped, the
  request removed and the pane closed. One a browser is on stays. A job that needs it again starts it
  again.

### It stands in that checkout's fixed workspace

- **Where:** each checkout has one fixed workspace of its own holding one dev-server tab, whose pane runs
  `bmo-agency dev-server pane` with the checkout's name.
- **Why apart from the run's pane:** a job's run stands in a pane of its own that closes with the run
  (`docs/agents.md`, 「A run in a pane of its own」), while the dev server's life is the job's, so it
  stands in a workspace that outlives every run.
- **Only while the dev server does:** the sweep of herdr closes a checkout's workspace whole once neither a
  dev-server pane nor a run at work is in it, a support worktree's past its repository's present count
  among them (`docs/agent-control.md`, 「The whole of herdr is swept of what nothing stands for」); the
  next setup makes it again.

### What the script in the pane does

- **Starts the command.**
  - It runs the row's command with zsh, with the checkout as the working directory.
  - The command gets its own session and process group, and that group number is written to
    `dev-server.pid` in the workbench folder.
- **Clears a leftover first.**
  - On a fresh start, the script first ends the group that file points at if it is still alive.
  - Why: when the pane closed and the script died without cleaning up, a dev server left holding the port
    must not block the new one.
- **Its own environment.**
  - Agency's `BMO_` variables are removed from the dev server's environment, and the workbench folder
    path is added as `BMO_WORKBENCH_DIR`.
  - This keeps agency's secrets from reaching any dev server; if a dev server needs a `BMO_` variable,
    the command writes it itself.
  - This environment is the dev server's, not the job's.
- **Logs.**
  - Output flows into the pane as it is and is also written to `dev-server.log` in the workbench folder.
  - The log rotates at a size limit.
  - A job reads the dev server's state from this file, not through herdr.
- **Restarts.**
  - When the command ends, it starts it again after a few seconds.
  - If it keeps ending right after starting, it widens the interval and says so in the pane.
- **Follows the row.**
  - It rereads the row every few seconds.
  - If the command changed, it ends the running command with its process group and starts the new one.
  - If the command is empty or the repository or the checkout is gone, it ends it and ends itself; herdr collects a pane
    whose command has ended.
  - So editing the field alone changes or brings down the dev server.
- **Follows the job and its use.**
  - On every look it also asks whether a job is at work in the checkout, whether the request still
    stands, and whether the machine is short of memory, and while it is, whether a client is on the
    server.
  - No job, no request, or memory short with no client on the server stops the server, removes the
    request and ends the script, which takes the pane with it.
- **Stops by asking.**
  - It stops a server with TERM to its process group and waits until the group has gone, however long
    its ending takes; it never escalates to KILL.

### Agency sets it up

- **Idempotent:** setting it up ends in the same result however many times it is called.
- **A pane stands only where the request stands** (「It stands only while a job runs」);
  everywhere else setting it up closes any pane it finds.
- **Called at five moments:**
  1. When agency's server starts, for every checkout a job is at work in of a repository with a dev-server
     command.
  2. When the herdr subscription is reattached. If herdr died and came back, the pane front's process is
     dead.
  3. Right after agency's repository routes change a repository's dev-server command, for its checkouts a
     job holds.
  4. When a job starts in that checkout, before the runner hands over the first prompt, with the request
     left from before removed first: the pane a job that died left standing is closed.
  5. When a job starts or restarts the server, by the verb.

#### The setup order

1. **Find the workspace:** find or create the checkout's fixed workspace.
2. **Find the pane:** a pane in that workspace with no run token, which `layout.export` says runs the
   dev-server script under this checkout's name, is the dev-server pane.
3. **Otherwise start one:** in a new tab with `layout.apply` given only `workspace_id`.
4. **No `tab.focus`:** it would steal the screen a person was looking at.

#### The rules around it

- **The binary's version rides in the command arguments.**
  - The version is a hash of the agency binary.
  - A dev-server pane with a different version is closed and started anew.
  - A running pane does not reread its own code, so when a deployment restarts agency, moment 1
    replaces panes of the old version.
- **One setup at a time.**
  - Agency's server, a job's runner and the verb are separate processes and may try to set up the same
    checkout's dev server at once.
  - So setup holds a file lock, one per checkout, in the data root's lock directory; whoever comes later
    finds the pane the earlier one started.
  - If two or more dev-server panes of the same checkout are still seen (say, a person started another),
    only the first in the snapshot is kept and the rest are closed.
  - Inside the lock only one side closes, so there is no disagreement about which to keep.
- **Not closed when the workspace is tidied.**
  - Whoever tidies the workspace closes panes with no run token, but keeps panes whose command is the
    dev-server script.
  - A pane herdr revived has no command, so it is still closed, and moment 2 starts a new one.

### It is not a run

- **No run token:** the dev-server pane has none.
- **So it is outside the run bookkeeping:** it is not in the picture that tracks liveness, not in the run
  list a run writes, and takes no lock a job waits on; it only reads whether a job is at work in the checkout.
- **Tied to the job by that alone:** it outlives no job; a job never starts a second server beside it
  and never ends the pane, but starts, restarts and stops the server through the verb (below).

### A job starts, restarts and stops it

- **The lines:** `<agency binary> dev-server start|restart|stop <checkout>`, given in full
  in the preamble header, since a job's checkout may be any repository's.
- **`start`:**
  1. Refuses a name that is no checkout, a checkout whose repository has no dev-server command
     (`docs/worker-slots.md`), or a checkout no job is at work in.
  2. Leaves the request in the workbench folder, which counts as use as well.
  3. Brings the dev-server pane to the row, as agency does, so a pane that is missing is started.
  4. Where a live server stands already, says so at once. Otherwise waits until the pid file names a
     live process group, then prints it and the log's path. Where the request is taken away or the job
     stops being at work before one stands — the pane closed, or the job ended — it prints the log's tail and
     fails.
- **`restart`:** the same, with a restart request left beside the request, and the wait is for a
  process group other than the one standing.
- **`stop`:** removes the request, then waits until the server's group has gone,
  the pane having stopped it on its next look; a checkout with nothing running is told so. It refuses
  only a name that is no checkout.
- **What the pane does with the request:** on its next look it takes the request away; a running server
  is stopped and started again at once, and a server waiting out the delay after failures is started
  at once with the failure count cleared. A pane that starts fresh drops a request left from before,
  since it starts the server anyway.
- **When a job uses them:** it starts the server just before it first looks at what the server serves —
  it takes about a minute to come up — and restarts it where the server is down, answers with errors,
  or holds code the job has changed in a way hot reload does not take; a dev server it could start or
  restart is never reported as unfinished work. Done looking, it stops it rather than leaving it standing
  until the job ends.
- **What stays off limits:** the system's own standing services are not a dev server,
  and a job does not restart them; the deployment does.

## What a job leaves running

- **Nothing a job's run starts outlives the run.** A `nuxi dev` it ran itself, a browser and the daemon
  driving it, anything it put in the background or detached: every one is taken down when the run's
  program ends, whether the run finished, was stopped or was cut (`docs/run-groups.md`).
- **The checkout's dev server is not among them:** it stands in the checkout's own fixed workspace,
  started by herdr rather than by the run, and carries no `BMO_` variable (「What the script in the pane
  does」).
- **Each stage is a run of its own,** so a server one stage started is gone when the next stage begins,
  and a stage that needs it starts it again.
- **Reading them:** `bmo-agency dev-server list` lists every checkout's dev server and every run's record
  with what it still holds; `--sweep` takes down every run whose program is gone.

## A repository's own bench

- **What a repository's dev-server command stands up is that repository's own:** its console, its
  stand-in services, its ports, its address and its authentication, and the tools a job uses to
  measure or picture a screen there. That repository's documents state them.
- **What agency holds to of any of them** is what this document says: the command is one field of
  the repository row, it runs in a job's checkout from that checkout's workbench folder, it carries no `BMO_`
  variable but its workbench path and its use mark, a support checkout is told its number in
  `BMO_WORKER_SLOT` (`docs/worker-slots.md`, 「Dev servers」), and it stands only while a job in that
  checkout runs and has asked for it.

