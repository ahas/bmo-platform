# What agency records of runs

Every agent's own log of what it did, and what is kept of a run that failed. What an agent decided
about a message is not agency's: the judgement records belong to the units that judge.

## Agent log

- Every agent's own internal log, one line per thing it did, written to agency's store as it happens
  (`packages/agent/runlog.ts`).
- Whoever reads a run's stream writes it — the bot for a supervisor's turn, the runner of a role's run
  for that run — so a supervisor's turn and every role's whole stream land in one table under the name
  of the supervisor or the role the run is for. A job's lines stand under its agent, the one its item names or its
  launch took for its item's model, the one that did the work (`docs/repositories.md`, 「An agent an item names」), never under the checkout it ran in, and a role's
  log is read with its agents': the lines under the role's name and the lines of every agent of that role, so a
  role's screen shows the job its agent is running.
- What the stream carries is what is kept: the run's text and thinking, every tool call with its
  arguments and what came back, MCP tools among them under their own names, and the branches a run
  fanned out, each line tied to the `Task` or `research` call it came from so a branch reads
  on its own.
- A Claude Workflow is kept too, and it is kept differently, because the CLI does not run one as a
  chain of tool calls in the stream: it runs it as a background task of its own, and the stream says
  only that a task started, how its branches are getting on, and that it ended. Those events are
  what the branches' own rows are made of — the script the Claude Workflow was given, each branch's
  instruction, model and outcome — and what a branch actually did is read afterwards from the
  transcript the Claude Workflow keeps for it, so its tool calls, their arguments and what came back
  stand in the log like any other branch's.
- That last part arrives when the Claude Workflow ends rather than as it goes, since nothing of it
  is in the stream before then, and one branch is held to a few hundred lines so a fanned-out job
  cannot crowd out its own log.
- **A pane program's record is filed as its lines are.** A provider whose turns go through its own
  pane program (`docs/agent-control.md`, 「What a resident turn reads」) reports what the turn is
  doing in the record it writes: a `progress` line carries the phase, the tool being called and how
  full the session stands, and the `answer` line carries the turn's usage beside its words. Each is
  filed as the line it is, so a room reading the run as it goes sees the turn's progress and its
  spend without waiting for the end.
  - **Its tool calls are filed as a stream's are.** A `progress` line saying a call began is filed
    beside itself as a `tool` line under the tool's name, its text the summary of the call's arguments
    the record keeps — redacted and cut by the terminal client, the line marked cut upstream where the record says
    `inputCut` — and one saying the call settled as the `tool-result` line that answers it, succeeded
    or failed, its text saying how many milliseconds the call ran and how many bytes its result held
    where the record gives `resultBytes`. A settling line carrying `bashEditDiff` — the terminal client's record
    of the files a shell call changed — has it kept as the result line's
    `metadata.toolUseResult.bashEditDiff`, the place a Claude Code tool result's `toolUseResult`
    stands.
  - **A call is matched by its id.** A record line naming its call by `toolCallId` is that call's: its
    first line opens it, a later one of the same id that is not a settling changes nothing, and its
    settling answers it, whatever else is open; the two lines share the id as their reference. A record
    that names no call by an id is matched first in, first out: each settling answers the oldest call
    still open, the two sharing a reference made of the turn's id and the call's place in the turn, and
    a call reported as asked and then as under way is one call.
- **A pane program's error stream is filed too.** Where the program's command keeps its error stream in
  its place, as the terminal client's does (`docs/agent-control.md`, 「What a resident turn reads」),
  the lines the file gained since the last look are written into the run log of the turn that just
  ended, each a note, at most the number `policy/resident-turn.toml` names a turn; and where a chat
  session's server did not attach, one note naming the server and why.
- A line is committed the moment it arrives, so a run killed from outside leaves everything up to
  the kill, and the ids run forward, so a finished run is scrolled back rather than only tailed.
- **A line's moment is in epoch milliseconds,** whichever writer filed it and whatever program the run
  is on: the store keeps it so, and every route answers it as kept.
- **A job's lines keep the metadata their writer filed.** The run log's line shape answers it as
  `metadata`, the JSON object itself or null where none was kept; a pane program's shell call carries
  Claude Code's `toolUseResult.bashEditDiff` there (`docs/api.md`, 「log-lines」).
- It is bounded three ways — a per-line cut, the last few thousand lines per supervisor or role, and
  an age —
  pruned by the writer as it goes, and over the whole table by agency's own clock: by its server as
  it starts and once a day after.
- The prune reads one supervisor's or role's lines newest first only as far as its bounds reach, then
  deletes the lines past them in short batches; each delete statement has a statement timeout, so no pass
  holds one long transaction on the table. Partial indexes on the lines no job wrote carry both the
  per-supervisor and per-role pages and the age; an index by run and id carries a page of one run's lines,
  which the log-lines route reads by the run alone.
- The per-line cut falls only on what a machine leaves behind: a tool's arguments, what a tool
  handed back, and a prompt handed to a model are cut, while what a run or the bot wrote in its own
  words — a job's report, a failure reason — is kept whole, since those are the
  lines a person reads and they are a small share of what fills the table.
- The console reads the log through agency's run-log route, by supervisor or role and from an id onward
  (`docs/api.md`, 「log-lines」), so a finished run is scrolled back page by page.
- **A job's lines are numbered in order within the job,** whichever of its writers files them: the turn's own
  log, the lines of a word said to the job, a note in the bot's own words. A writer whose next number another
  writer took meanwhile reads again where the job's lines left off and files its line once more, so no line is
  lost to a word said into the job while its turn writes.
- **A job's lines are read by the job,** across every run its turns were logged under: its last line,
  and its top-level tool calls and their results with how many model requests it made and how often
  its context was compacted, which is what the job unit's figures and its watch on a running job are
  made of.
- **A note in the bot's own words** is written into a run's log across the boundary by whoever carries
  the run, such as the job unit telling which road a turn went out on or why the run ended. It stands
  beside the run's own lines, under the run's supervisor or role, room and job, and is kept whole.
- **A job never stands running with an empty log.** Whatever its run waits on before its program
  starts, it says in one note of its log as the wait starts and in another as it goes out: a road to
  open, a slot of its role or the agent its item names, another turn in the standing program it goes into,
  and the renewal of its login by another run (`docs/cache-fallback.md`, 「A login that is about to end」),
  as well as its own renewal of that login.
- **A job's log names every Claude account its turn was passed over on,** refusing it or with a window full, in a
  note written as the walk passes it over: the account, its own words, and the account the turn goes out on
  next, or that none is left
  (`docs/cache-fallback.md`, 「The Claude Code road is a list of accounts」).
- **A room forgotten takes its lines with it,** but a job's: a job's turns are logged under its room
  and belong to the job, which outlives the room (`docs/api.md`, 「sessions」, forgetting a room).

## The run ledger

- **A run of the ledger is one row of `runs`:** its supervisor or role, its `work_unit` — the name its log
  lines are written under, a job's run name or a turn's unit, which names work and is no organisation's unit —,
  its title where its opener names one, its room, its depth, when it started and finished in epoch
  milliseconds, its outcome, the tokens its context carried as it ended, how many tool calls its log lines
  held under its unit as it closed, every `tool` line under that name, a branch's among them, and whether it
  is the run of a turn agency carries. A run keeps the first closing it was given (`docs/api.md`, 「runs」).
- **A run stands under its agent's owners:** the account or the organisation unit the supervisor's or the
  seat's row is filed under, written into the run's `owner_account` and `owner_unit` as it opens, neither for
  a run of the platform's own agents. The reads of the ledger narrow by these (`docs/api.md`, 「Who a caller
  acts for」); a run opened before the columns stood names neither and stands with the platform's.
- **A run's cost and its source are kept beside its tokens:** what it cost in millionths of a US dollar, summed
  over the models that answered — a Claude Code turn's the status line's `total_cost_usd` the turn added, a
  gateway road's the sum of the gateway's request records of the run's program while the run went — or none where its road reported none;
  and the supply source it went out on, `personal`, a platform source the supply hook named, or `platform`
  (`docs/supply.md`, 「The ledger」). A job keeps the sum over its turns and the source its last turn took.
- **The job unit's runs** are opened and closed by the job unit across the boundary.
- **A translation through the gateway is a run of the translator role,** opened by agency as the request goes
  out and closed as it is answered or fails: its unit is the name its log lines are written under, it names
  no room, its outcome is `answered` or `failed`, and what it spent is the tokens the gateway's answer
  counts. Its log lines are its start, naming the registration it went out on; one note, naming the purpose,
  the target language and how many characters went in and came back; and its end, with why it failed where
  it did. They hold none of the text translated.
- **A chat turn's run is opened by agency** as a turn into a room's or a supervisor's session starts, the
  turn naming its room and carrying no work parts: its unit is the name the turn's log lines are written
  under, its room the room the turn answers, its round the round the turn's context names, and its message
  the latest message the turn answers. It is closed as the turn ends — by the server that picks the turn
  up where agency restarted while it went (`docs/sessions.md`, 「A restart hands the session over」),
  which also files the lines of the program's record the log does not yet hold — with how the turn ended,
  the tokens its context carried and what the turn spent: its input, output, cache-read and cache-write
  tokens, each summed over the models that answered it, and none where its road reported no figures.
- **A job's turn is a run opened by agency's job runner** (`docs/api.md`, 「assignments and jobs」) as the
  turn goes out: its role the job's role, its unit the job's run name, the one its log lines are written
  under, and its room the job's. It is closed as the turn ends, with how it ended, the tokens its context
  carried and what it spent, as a chat turn's is, and the job keeps the run and the same four figures.
- **A role session's turn is a run opened by agency** as the turn goes out, wherever the turn is no job's
  and no gate's: a worker ask and any other turn a caller sends into a role's session. Its role
  is the session's agent; its unit the name its log lines are written under, the turn's unit where it names
  one and its run name otherwise; its title the turn's title; its room the room the turn names, none where it
  names none; and its round the round the turn's context names. It is closed as the turn ends, with how it
  ended, the tokens its context carried and what it spent, as a chat turn's is, and by the server that picks
  the turn up where agency restarted while it went.
- **A job's report ends with the words said into its turn while it ran,** read off its run's log lines named
  `word`, each with what became of it (`docs/agent-control.md`, 「Words that arrive while a turn is
  working」), so what a job was told mid-run stands beside what it came to.
- **A run whose turn has ended without closing it is closed as `cut`.** A turn can end with nothing left to
  close its run: a job ended or stopped from outside while its turn went, its turn then lost to agency's
  restart, or a turn agency carries whose process went away before a server picked turns up again. A pass at
  agency's start and on the executions' clock closes every such run, writing in its log the line that says
  why: a job's run whose job has ended, no turn of it carried by this process nor kept in flight; and the run
  of a turn agency carries, a chat turn's or a role session's, begun more than two minutes ago and carried by
  no turn kept in flight, which every turn going is. Any other run is left as it stands.
- **Every turn of a round is a run of its own:** the round's own turn and each question, item and
  check of the receiver's draft process go out as turns naming the same round, so each opens its run
  under that round and keeps its own spend. A round's usage is the sum of its runs', and how it divides
  among the questions is read run by run.
- **The message its answer went out as** is attached by the receiver once it files the answer, which
  replaces the message the turn answered; an attach leaves the closing as it stands.
- **What reads it:** the console's chip line under a message finds the round's runs by the round and
  reads each run's log lines by the run's id: what the turn spent, the tools it called and what it wrote.
  The usage records group the chat turns' spend by round (`docs/api.md`, 「accounts, rest and usage」),
  beside the jobs' grouping by model, role, repository, holder and room.

## The roads a turn walked

- **Each turn the session contract carries on a supervisor's line walked one road:** the
  provider and model it went out on, how it ended, how long it held the run and what it spent. The road
  records are the gate unit's table, so agency writes none of them: it publishes each road as
  `road-walked` as the turn ends (`docs/api.md`, 「Events」), and the unit that keeps the road records
  writes the line.
- **What a road carries:** its stage, `chat`, the stage a conversational turn's run is written down at
  (`policy/supervisor-runner.toml`), the room, the run, the provider and the model, its outcome, `answered`
  or `error`, how long it held the run in milliseconds, its tokens summed over the models that answered,
  none where the road reported none, and how full the session's context window stood once it ended.
- **A turn a reset overtook walked no road,** and none is published for it.

## What a failed run leaves behind

- A run that died says why on its last lines, and everything above them is the program starting up —
  a welcome banner, an ASCII drawing, a rule across the terminal, a warning about the mode it was
  launched in. So what is written down of it is its **tail and never its head**
  (`packages/agent/failure-text.ts`): escape sequences go, then every line carrying no letter and no
  digit, so a drawing and a row of dots take no room from the cause; what is left is cut to two
  thousand characters from the end, on a line boundary, with a `…` where the head went, and output
  that was decoration and nothing else keeps its own tail rather than leaving the record empty.
- **One length, everywhere a failure is written down** — the reader of a run's output, a supervisor's
  keepalive, a compaction, a search's branch, a role's run and the job runner — because a record cut to two
  hundred characters in one place and five hundred in another is read as two different failures.
- None of this decides anything: whether a run failed and which failure it is are read off what was
  printed whole, so a login gone and a session the CLI no longer holds are recognised before any
  shortening happens.
- A failed job's report is kept whole rather than clipped, since the job listing its caller prints is
  where anybody goes looking for a cause.

## Why a run died

- A run that died is diagnosed from what was read where it ended and nothing else: how it
  ended — an exit with its code, an exit by a signal, an error handed back in place of an answer, no
  answer where one was owed, a pane or host that could not be had, or nothing readable — what the
  program said back, cut to one line, and the last tool call its run log holds, with the command or
  file it was given. A cause nobody could read is said to be unknown, and a run that held together and
  said nothing is not counted as broken.
- The account of a dead run is two lines in the words of `policy/chat-health.toml`: what broke and
  why, then what it was last doing and where the rest is — the run's log by its name, or, for a failure
  no run was had for, the journal of the unit the caller names.
- **The failure standing now is kept in agency's settings** under `doctor_standing_failure`, so a
  server that went down over its own failure still knows of it after a restart. It holds the cause,
  what tells this cause from another, the room, how many times in a row the same cause came, and
  whether the owner has heard of it; once heard of, it stays heard of until a run answers again,
  whatever cause the latest failure had.
- **A run that answers again while a failure stands is a recovery,** published as `run-recovered`: the
  facts of the failure and of the answer, what saying it is for, the kind of notice it is and how long
  it stays worth saying (`docs/api.md`, 「Events」). Agency voices nothing; the consumer that owns the
  room says it in its own manner, and the standing failure is cleared.
