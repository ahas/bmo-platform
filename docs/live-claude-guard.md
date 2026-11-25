# A job opening a real Claude session

## What it costs and who may open one

- **The cost:** opening a new Claude session, copying a session record to continue it as a new
  session, or putting something like `/compact` into a CLI in a pane makes that session write its
  uncached prefix and conversation from the start. For a session with a long conversation, one such
  write exceeds a hundred thousand cache-write tokens.
- **The bot's own sessions** — a supervisor's turns and its keepalive, every role's run, session
  folding — are opened knowing that cost.
- **A job's sessions** for a measurement or an experiment are opened only once the job has said
  what it is opening them for. The owner is not asked: the job opens the guard itself, and what it ran,
  and how many times, is kept and carried into the item's report.
- **Who holds the line:** code, not a sentence. It stops a job from spending without knowing it,
  not a job that knows.

## Which processes are a job's

- **Two marks, inherited together.** A process a job's CLI starts as a tool inherits both:
  - the session mark (`CLAUDECODE`), which Claude Code puts on what its tools start
  - the runner mark (`BMO_TASK_LOCKED`), which the job runner sets in its own environment and hands
    down to the job's pane along with agency's variables
- **Who carries what:**
  - A shell command, and a `bun` script that shell calls, carry both.
  - A tool a room's session starts carries only the session mark.
  - The bot's and the console's services, and the runner through which the bot starts a job's turn,
    carry no session mark.
- **The second layer below stands only for a process carrying both marks.** A room's tools have
  nothing to do with this guard.
- **Tool runs the bot starts as a unit or a pane** (search, browse, speaking and the like) are not
  a job's processes.
  - They receive only the environment the bot picked by name, and inherit neither mark.
  - This holds even when a job called the tool that started them: a tool run is opened by the bot's
    tool, knowing its cost, and so are its own children, such as a search's branches.
  - So they are not what this guard is about.

## First layer: a hook that reads shell commands before they run

- **Where:** agency's shell hook (「Agency's shell hook closes every command's standard input」) takes every
  Bash command of a program on a line whose declaration says its programs are jobs' (`guards_job_shell` in
  `policy/run-lines.toml`) — the worker line — and judges it before it runs, as the job guard. The place's
  settings start the hook with the word that asks for it, `--guard-job`. On such a line the hook also takes
  every call of the file tools that write — `Edit`, `Write`, `MultiEdit` and `NotebookEdit` — and judges the
  file it names for the two places a job does not write into (「The same hook keeps a job to its own
  checkout」, 「The same hook keeps STRUCTURE.md the owner's」); every other judgement here is of a shell
  command alone.
- **Who judges:** agency, in the hook's own process. A refusal comes back as a deny decision carrying the
  reason and the piece of the command the guard read it in — the program word, the redirect's target, the
  text handed to `eval` or a shell, and where a word was read as a command, why it was read that way. So a
  job reads what was blocked and why instead of guessing at the rule and writing around it.
- **In what order:** first what no opening lets through — print mode (below), a removal the CLI would stop
  on and the shell habits that stall a job (「The same hook refuses a removal the CLI would stop on」, 「The
  same hook refuses the shell habits that stall a job」), a change to `STRUCTURE.md` the job's item does not
  carry the owner's word for and a write into another worker's checkout — then a heavy command started
  outside the testbed (「The same hook sends a heavy command to the testbed」), then the four things below
  unless the job has opened the guard. A command none of them refuses goes to the tool with its standard
  input closed, and a file tool's call none refuses goes to the tool as it came.

### The four things it refuses

1. **A command that writes into the CLI's record directory.**
   - Every login keeps its own record directory: the `projects` directory under that login's
     configuration directory. The one this machine's own login uses, `~/.claude/projects`, and the
     word `CLAUDE_CONFIG_DIR` are always read as the records; so is the `projects` directory under
     every account's configuration directory, spelled as an absolute path, from `~`, from `$HOME`
     or from `${HOME}`.
   - Which configuration directories there are is agency's store's: the accounts the owner registers through
     agency's account routes (`docs/api.md`, 「accounts, rest and usage」). The hook reads them off the
     store's database where the command's text holds the word `projects`, which every record directory's
     path ends in, and judges against them; an endpoint account keeps its records in its own configuration
     home, which is listed like a directory account's; a token account keeps its records in the machine's
     directory, and the machine's own directory in the hook's environment is added beside them. A store with
     no account, or one the hook cannot read, leaves the guard with the machine's spellings alone, which it
     refuses exactly as before.
   - Among commands that mention the records anywhere: those that copy, move, link, delete or
     create (`cp`, `mv`, `ln`, `rm`, `rsync`, `tee`, `touch` and the like), edit in place
     (`sed -i`), or redirect to anything other than the null device or a literal path outside
     every configuration directory.
   - A target written as a variable the command did not itself assign could be anywhere, so inside
     a command that mentions the record directory it counts as a write.
   - Reads (`grep`, `jq`, `tail`, `ls`) are not refused.
2. **Calling the `claude` executable.**
   - The same whether it stands as one stage of a pipeline; after an assignment or `env`,
     `command`, `exec`, `timeout`, `nohup` or `xargs`; written as a path; or inside the string of
     `bash -c` or `zsh -c`.
   - Also code that writes `"claude"` as one slot of a command line (such as
     `Bun.spawn(["claude", …])`). A name in backquotes counts only inside such a list's brackets;
     standing in prose it is a markdown code span naming the program, not an argument of one.
   - `.claude/` inside a path, or the word standing as an argument, is not a call.
   - A call with `-p` or `--print` among its words is print mode, which no agent of this system runs
     (`docs/agents.md`, 「No print mode」): it is refused whatever the job has opened.
3. **Putting words into a session in a pane.** Code that writes herdr's `agent.prompt` call, and
   driving agent or pane placement with the `herdr` command.
4. **The flag that forks a session:** `--fork-session`.

### How it reads a command

- **As the shell splits it.** A command line is read by brush-parser's tokenizer and word parser,
  which keep each word's quoting as written and give a heredoc's body apart from the commands, as
  the text it carries. A line they cannot read — a quote left open, a construct of zsh's alone —
  reads as no command at all: the shell refuses the first, and the second is out of the hook's sight
  as a script file is.
- **One layer deeper, the way the shell reads it.** A variable the command itself assigned is read as
  its value, so `A=claude; $A`, and a copy with the record directory split across two variables,
  are judged the same.
- **Text run as a command.** Where a command has such a place — a string passed to `eval`, `source`
  or `.`, a shell reading standard input or a process substitution with no script file, a variable
  or command substitution standing in the command position — every word inside is read as a
  command too.
- **A backquote is a command substitution only where the shell opens one.** Inside single or double
  quotes it is a character of the word, so nothing in a quoted report, commit message or judgement
  is read as a program: ``--report 'added the node (`bot/claude`)'`` names a path in a report and starts
  nothing, while ``(`echo claude`)`` outside quotes still does. A `$(` is read the same way.
- **Never caught as a live Claude session:** `bun run test`, `bun test`, the deploy unit's commit,
  and git commands. Some of them still have to ask for a wait that covers
  them (「The same hook refuses the shell habits that stall a job」).

### When a reading cannot be made

- A testbed that does not answer reads the command as light. A store the hook cannot reach leaves the
  records to the machine's spellings, and the job's opening unread, so a command of the four things above is
  refused as it is before an opening, and leaves the checkouts unread, so no write is refused as another
  worker's. An item that cannot be read leaves the owner's word unread, so a change to `STRUCTURE.md` is
  refused. A hook that cannot read its input, or fails, lets the command go to the tool as written.
- One hook failing to run must not stop every shell command of every job, and the second layer
  stands behind it.

### The hook moves the worker's prefix

- The hook stands in the place's settings agency writes for the worker line, so starting it with another word
  moves the mark of the worker's prefix.
- A role's kept session stands on the prefix it was opened on, so a run after a deployment that changed
  these settings resumes a session whose prefix moved once, and pays that move once
  (`docs/sessions.md`, 「A role's session」).

## The same hook refuses a removal the CLI would stop on

- **Why:** Claude Code asks a person before some removals even in bypass mode, and a job's pane has
  nobody to answer (`docs/agent-control.md`, 「A confirmation in the middle of a turn」). A removal
  through an empty variable is also a real danger: `rm -f $S/*.md` becomes `rm -f /*.md` and
  `rm -rf $S/` becomes `rm -rf /`.
- **What it refuses:** an `rm` or `rmdir` — behind assignments and runners, inside `bash -c`,
  `zsh -c` and `eval`, and on any line of the command, heredoc lines included — one of whose
  targets
  - holds a command substitution, `$(…)` or a backquote, or
  - holds a parameter expansion that does not stop on an empty value: anything other than
    `${NAME:?…}` or `${NAME?…}`. Text in single quotes is literal and not read.
- **What it lets through:** a literal target, a guarded one (`"${S:?}"/*.md`), and a removal whose
  targets come from elsewhere (`xargs rm`, `find … -delete`).
- **Nothing opens it.** The refusal says how to write the removal instead: guard the
  variable, quote the path, or name the path outright, and leave files under `~/.claude` alone.

## The same hook keeps a job to its own checkout

- **Why:** a job writes only in its own checkout and in one it has borrowed (`docs/worker-slots.md`, 「A job
  writes only in its own checkout」).
- **Whose checkouts these are,** read off agency's store. The job is the latest of agency's jobs working in
  the program's run, `BMO_RUN_NAME`, read only in a process carrying both of a job's marks (「Which processes
  are a job's」). Its own checkout is the one that job runs in, and the ones it borrowed are those the
  assignments held or running under its own holder name, `job:<jobId>`, name. Every other checkout of every
  repository the store holds, on the machine the job's own checkout stands on, is another worker's, unless it
  stands at the job's own path. A checkout's path counts as written as it stands and from the home as `~`,
  `$HOME` or `${HOME}`, and only at a boundary, so a support worktree whose path begins with its base's is
  not read as the base. Where one checkout stands inside another's directory, a path is the deepest
  checkout's that holds it, so a write in the job's own checkout is never read as the enclosing one's. A program that is no job's, or a store the hook cannot read, makes no checkout
  another worker's.
- **What it refuses of a shell command** that names such a checkout, read the way the shell splits it —
  behind assignments and runners, and inside the text handed to a shell's `-c` or to `eval`:
  - a redirect into it;
  - a program that copies, moves, links, creates or removes files (`cp`, `mv`, `ln`, `rm`, `rsync`, `tee`,
    `install`, `truncate`, `dd`, `touch`, `mkdir`, `rmdir`, `unlink`) with a word naming it — of `cp`,
    `rsync`, `install` and `ln` their destination alone, so a copy out of the checkout is no write into it;
  - an in-place edit there: `sed`, `perl` or `ruby` with `-i`;
  - git changing it — `add`, `commit`, `rm`, `mv`, `checkout`, `switch`, `restore`, `reset`, `apply`,
    `stash`, `merge`, `rebase`, `revert`, `cherry-pick` or `pull` — named by a path or after a `cd` into it;
  - a script an interpreter is handed on its command line or by a heredoc (`python3 -c`, `node -e`,
    `perl -e`, `python3 - <<'EOF'` and the like) whose write call names a file there, or that names the
    checkout and makes a write call whose file the reading cannot resolve.
  After a `cd` or `pushd` into the checkout, every relative path counts as being there.
- **A relative path is read from where the shell stands:** the working directory the hook is handed with the
  call, followed through each `cd` and `pushd` the command makes. A relative word that lands inside another
  worker's checkout is that checkout's, by the same deepest-checkout reading, so a job standing in a folder
  that holds other checkouts below it writes into none of them by a relative path either.
- **What it refuses of a file tool:** a file whose path lies inside such a checkout.
- **Every umbrella's folder is sealed, for every job** (`docs/worker-slots.md`, 「An umbrella repository」). For
  each umbrella on the job's machine, its own job's among them, the files its git tracks are read as the hook
  needs them, and the hook refuses, read the way the checkouts' writes are and from where the shell stands: a
  write naming one of those files however its path is spelled, absolute or relative; and git changing that
  repository itself — one of the git subcommands above naming the folder, or run where the shell stands in the
  folder or in a directory of it holding files it tracks, in no checkout below it. Of a file tool, a file the
  umbrella tracks. An umbrella is no other worker's checkout beyond those files: a write in a checkout below
  the folder is that checkout's, by the deepest-checkout reading, and a place below it that no checkout names,
  or a file its git does not track, is not guarded. `STRUCTURE.md` keeps its own rule below, in the umbrella as
  anywhere. The refusal names the folder and says that a job's work goes in its own checkout and the ones lent
  to it.
- **What it lets through:** a read (`cat`, `grep`, `git diff`, `git log`), any write in the job's own
  checkout or in one it borrowed, and a write where no checkout stands.
- **The refusal** names the checkout, whose it is, and the verb that borrows one, `bmo job borrow
  <repository>`. Nothing opens it.

## The same hook keeps STRUCTURE.md the owner's

- **Why:** `STRUCTURE.md` is the owner's statement of the system, and only the owner's own request naming it
  may change it. Where the code differs from it, a job reports the difference instead.
- **What it refuses:** a shell command that names `STRUCTURE.md` and writes it — a redirect into it, a
  program of the writing ones above with a word naming it, an in-place edit of it, git's `rm`, `mv`,
  `checkout`, `restore` or `apply` naming it, a script that writes it — read as the checkout's writes are,
  but with no `cd` followed; and a file tool's call on a file named `STRUCTURE.md`, in any directory.
- **Whose word opens it:** a job whose item the owner asked for and whose title or brief names
  `STRUCTURE.md`. The item is the one agency's row of the job names; the hook reads it from the job service,
  `GET /items/{itemId}`, at the address and with the token the program's environment names (`BMO_JOB_URL`,
  and `BMO_JOB_TOKEN` or the file `BMO_JOB_TOKEN_FILE` names). The owner asked for it where it carries the
  label `asked-by=owner`, which agency alone writes, on an item a supervisor files from a turn whose every
  message is the owner's (`docs/api.md`, 「assignments and jobs」). An item's `requesterName` is a name its
  filer writes, which no service ties to the owner's sender, so it is not read.
- **It is read only where a write of `STRUCTURE.md` is,** and an item that cannot be read, a job that cannot
  be found, or a program that is no job's leaves the change refused. Nothing else opens it.

## Agency's shell hook closes every command's standard input

This is what the hook does on every line, whether or not it judges as the job guard there.

- **Why:** the Bash tool opens a command's standard input on `/dev/null` only while it reads no input
  redirection in the command's text. Where it reads one — a `<`, `<<` or `<<<`, a `<` inside a quoted
  word among them (measured: `'"<tool_call>"|<function='` does it) — the tool hands the whole shell a
  socket instead, which stays open for the whole run and is never written to. A `</dev/null` given to
  one part of a compound then leaves every other part on that socket, and a reader given nothing to read
  — `rg` with no path, a path held in a variable that came back empty — waits on it with no end. The
  program offers no setting that changes this.
- **What it does:** agency's verb `agent-status shell` is a `PreToolUse` hook on the shell of every
  Claude Code program agency starts on the worker or the chat line: a job's, a role's run, a co-work
  branch's and a supervisor's, and the subagents each opens. Every Bash call goes to the tool rewritten
  with `exec </dev/null` on a line of its own ahead of the command, the rest of the call as it was. The
  shell's own standard input is then `/dev/null` for everything the command runs, a part sent to the
  background included, and a reader given nothing ends at once: `cat` or `grep` reads an empty input,
  and `rg` given no path searches the working directory, as it does from a terminal. A pipe, a
  redirect, a heredoc and a here-string still feed the command they stand on.
- **Where it stands.** It is among the hooks of every place's settings: a supervisor's place, a standing
  worker's, a worker session's, and the thrown-away place a run in a pane of its own is given for its hooks
  where its line holds its stops, as the worker line does. On a supervisor's program it also refuses a
  command that starts Claude Code (`docs/agents.md`, 「No print mode」), and on a job's it judges as the job
  guard (「First layer」 above), where the settings name the file tools that write beside the shell among
  the calls it takes. A run in a
  pane of its own on a line that holds no stops, and a runner line's program, which is handed no settings,
  hold no such hook.
- **The tool's wait does not end such a command.** The worker's settings set the tool's wait, the
  default and the most a call may ask, to the largest the program takes, so a call that names no
  `timeout` waits for its command to end. A call that names a shorter one has the tool move the command
  to a background task past it, where it runs on. So closing the input is what ends a reader's wait,
  and where a reader still waits, the call clock stops it (`docs/agent-control.md`, 「A run's tool calls
  are held to a clock」).
- **When the hook cannot run,** the command goes to the tool as written.

## The same hook refuses the shell habits that stall a job

- **Why:** a job's shell is a fresh `zsh -c` for every call, its command line holds the command's
  own text, its standard input is closed (above), and zsh refuses a glob that matches nothing. Each
  habit below is a command that then runs for ever, runs past the call that started it, answers
  nothing, or never runs at all, and the refusal arrives at the moment the command is written, with
  the way to write it.
- **What it refuses**, read the way the shell splits the command — behind assignments and runners,
  inside `bash -c`, `zsh -c`, `eval` and command substitutions, and never inside a heredoc body,
  which is text:
  - **A known long command with a short wait asked for.** The close (`bmo job close`), the check of a
    change (`bmo check`) and the testbed's cargo verb, the whole suite (`bmo test suite`, `bmo test full`,
    or `bun test` with no file), the web console's typecheck or build, asking the owner (`bmo job ask`), a
    testbed run (any verb of the testbed but its status and its classify) and `git push`, in a Bash call
    that names a `timeout` under five minutes and does not run in the background. Past its wait the tool
    moves the command to the background and answers at once, so what depends on it would start before it
    ends. A call that names no `timeout` waits until the command ends (below), and is let through.
  - **A reader named nothing to read.** `ls` or `eza` given no path, and `cat`, `head`, `tail`,
    `wc`, `sort`, `grep` (not recursive), `sed`, `jq` (not `-n`), `python3`, `tr` and `xargs` given
    nothing to read, where neither a pipe, a redirect, a heredoc, a here-string nor a `while` loop's
    own input feeds them. On the closed standard input it would answer nothing, and an empty answer
    reads as one that found nothing.
  - **A `-f` match that finds its own shell.** `pkill -f` in any form, and `pgrep -f` other than a
    listing (`-a`, `-l`): the pattern stands in the command line of the shell running the call, so
    the kill takes that shell and a test or a wait built on the match never falls.
  - **A pattern the command expands for itself, left unquoted.** Any `--option=value` word whose
    value holds `*`, `?` or `[` outside quotes, and the value after `-name`, `-iname`, `-path`,
    `-ipath`, `-wholename`, `-lname`, `--glob`, `--iglob`, `--include`, `--exclude`, `--exclude-dir`,
    or `rg -g`.
  - **A sleeping wait with no bound.** A `while` or `until` loop in a command that also sleeps,
    unless the loop runs under `timeout`.
- **What it lets through:** the same commands with a path, a file, a redirect, a quoted pattern, no
  `timeout` or a covering one, `run_in_background`, `pgrep -af`, a `for` loop of fixed turns, and a loop
  under `timeout <secs> zsh -c '…'`.
- **Nothing opens it.** Each refusal names what the command would have done and how to write it.

## The same hook sends a heavy command to the testbed

- **Why:** one heavy test or build command holds the machine at a time (the testbed unit's `testbed.md`), and a
  job's shell is where another repository's `cargo test` or `npm test` starts. The hook stands on
  every worker's line, whatever checkout the job works in, so it is the one place every such command
  passes.
- **What it refuses:** a command some language of the testbed claims as heavy, read the way the shell
  splits it as the habits are, that is not itself a run of the testbed. The hook asks the testbed's
  `classify --json` verb (the testbed's `docs/boundary.md`), found on the path, with the command's whole
  text.
- **What it answers with:** the whole command written after the testbed's run verb and
  `--`, as the testbed's `classify` verb writes it, to be run with no `timeout` or the largest.
- **What it lets through:** a light command no language claims, the testbed's own run, and a verb of the checkout's own
  that takes the turn itself — the close, the suite's script, a cargo wrapper.
- **Nothing opens it.**

## Second layer: the guard in agency's code

- **Why it exists:** the shell hook cannot see what a script file calls. If a job writes a script
  that uses agency's code to open a session or put words into a pane, the command line shows only
  `bun <file>`. So agency's code blocks it once more.
- **One guard for every launch.** Every place that starts a Claude Code process — directly through a
  pipe or inside a herdr pane — and every place that puts words into a CLI in a pane goes through
  one guard. In a job's process the guard throws with the reason, and the process does not start.
- **The herdr client asks too.**
  - Calls that put words or keys into a pane, and calls that open a pane carrying the CLI as its
    command, are blocked before they reach the socket.
  - The function that opens a pane asks before it creates the workspace.
  - So a script that calls the pane-opening function or the herdr client directly, bypassing the
    function that runs a turn, meets the same guard.
- **What it blocks:** the lines that hold a session or write a prefix the next run will read — job,
  room, a pieces-shaped turn, search, browse, plain, and a session inside a pane.
- **A pane holding another provider's program** — Codex's, standing in a supervisor's or a run's pane — is
  still a pane a job does not put words into, so the job refusal holds there too. No Claude session is
  written by it, so Claude being off does not refuse it and nothing is recorded as a Claude run.
- **Agency's server's resident door asks the same marks.** In a test it reaches only a stand-in herdr under
  the test's home or the temporary directory; in a process carrying both of a job's marks it refuses,
  since the opening a job makes is read by the bun guard alone, except a run its procedure requires
  that the verb declared first (「A run the procedure requires declares itself」); and Claude Code switched off
  closes its turns on a Claude Code program as the bun guard does.
  - **It reads the marks off the ground the turn stands on.** Both of a job's marks and the test mark are
    read from the environment of the ground's machine, never from the process beside it. A service or a
    verb stands its ground on the process's own environment, so a job's process is refused as above. A
    test stands its ground on an environment of its own that carries neither of a job's marks, so a test
    the testbed runs from inside a job's shell is not a job; the shared test ground also names the test's
    home in the test mark, which holds it to the stand-ins under that home.
- **A test holds the list of files that start the CLI.** A file that starts the CLI without going
  through the guard fails that test.
- **Tests:** a process carrying the test mark follows the test rules instead of this one
  (the operations repository's `test-suite.md`).

### A run the procedure requires declares itself

- A verb whose run the procedure requires declares the purpose of its run to the guard in agency's server
  code before it opens anything, and the guard opens for the process that declared it.
- A test holds the list of files allowed to declare. Any other file that declares fails the test.

## A job opens it by saying what for

- **The way in:** when the guard blocks, the refusal names it. The job runs
  `bmo-agency live-claude --purpose '<what it measures and why>'` and goes on; nobody is asked
  and nothing waits.
- **What it lets through:** the interactive program, a fork, a write into the records and words put
  into a pane. Print mode stays refused, and its refusal names no way in.
- **Scope:** the opening stands for the one job whose run name it was made under, found by that
  name as for everything else here. The stages of a work order are separate jobs, so each stage
  opens it separately. A process outside a job has nothing to open, and the script refuses it.
- **Both layers read the same opening.** An opening is kept in agency's store under the run name it
  was made under; the guard looks for an opening of its run's name there. Which job and item a run is,
  agency does not read: a job's rows are the job unit's.
- **A copy is still the same job.** A script a job points at a copy of the store — `BMO_DB_PATH`
  or `BMO_DATA_DIR` naming a workbench copy — reads the store it was named first, and where that
  holds no job of the run's name, agency's own store, which holds the job and its opening. The
  opening is made there too, and every run it lets through is recorded where it was found.
- **Agency's own store is found where agency reads it**, whatever data root the script is
  pointed at: the PostgreSQL database the own data root's store setting names, and that root's
  `bmo.db` file only where the setting names none.
- **Every run it lets through is recorded** against that opening: the line or the command, and
  when. The hook records each shell command it passes because of the opening; agency's code
  records each launch or pane prompt it passes, a launch once however many places on its way ask
  the guard about the same command line.
- **The count reaches the report.** `bmo-agency live-claude --list` prints what this job has
  opened and run, and agency's run routes serve the same to the job unit, whose close adds one line
  to the report
  for each opening made by any of the item's jobs — its purpose and how many runs it let through —
  so the record says what was spent even when the job's own words leave it out.

## What the guard is not

- It is a wall against spending without knowing, not a sandbox.
- It cannot stop code written to get around it, and getting around it is spending without
  the record the opening keeps.
